"""Real SpiceDB integration. Set ORCHID_SPICEDB_BINARY to a local release binary."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest
import urllib.error
import urllib.request

from test_extension import connect, quote

BINARY = os.environ.get('ORCHID_SPICEDB_BINARY')
SCHEMA = '''definition user {}
definition team {
    relation member: user
}
caveat region_matches(region string) {
    region == "us"
}
definition channel {
    relation viewer: user | team#member | user with region_matches
    permission view = viewer
}'''


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


@unittest.skipUnless(BINARY, 'set ORCHID_SPICEDB_BINARY to test against real SpiceDB')
class AuthorizationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.log = open(Path(cls.tmp.name)/'spicedb.log', 'w+')
        cls.endpoint = f'http://127.0.0.1:{free_port()}'
        cls.metrics_endpoint = f'http://127.0.0.1:{free_port()}'
        cls.process = subprocess.Popen([
            BINARY, 'serve', '--datastore-engine', 'memory', '--grpc-preshared-key', 'orchid-test',
            '--grpc-addr', f'127.0.0.1:{free_port()}', '--http-enabled',
            '--http-addr', cls.endpoint.removeprefix('http://'), '--metrics-addr', cls.metrics_endpoint.removeprefix('http://'),
            '--telemetry-endpoint', '',
        ], stdout=cls.log, stderr=cls.log)
        for _ in range(100):
            try:
                cls.api('schema/write', {'schema': SCHEMA})
                break
            except (urllib.error.URLError, ConnectionError):
                if cls.process.poll() is not None:
                    cls.log.seek(0)
                    raise RuntimeError(cls.log.read())
                time.sleep(.1)
        else:
            cls.process.terminate()
            raise RuntimeError('SpiceDB did not become ready')

    @classmethod
    def tearDownClass(cls):
        cls.process.terminate()
        cls.process.wait(timeout=15)
        cls.log.close()
        cls.tmp.cleanup()

    @classmethod
    def api(cls, path, data):
        req = urllib.request.Request(cls.endpoint+'/v1/'+path, data=json.dumps(data).encode(),
            headers={'Authorization': 'Bearer orchid-test', 'Content-Type': 'application/json'})
        try:
            with urllib.request.urlopen(req, timeout=10) as result:
                return json.load(result)
        except urllib.error.HTTPError as error:
            body = error.read().decode()
            error.close()
            raise RuntimeError(body) from error

    def relationship(self, resource, rid, relation, subject, sid, subject_relation='', delete=False, caveat=None):
        rel = {'resource': {'objectType': resource, 'objectId': rid}, 'relation': relation,
               'subject': {'object': {'objectType': subject, 'objectId': sid}, 'optionalRelation': subject_relation}}
        if caveat:
            rel['optionalCaveat'] = {'caveatName': caveat}
        return self.api('relationships/write', {'updates': [{'operation': 'OPERATION_DELETE' if delete else 'OPERATION_TOUCH',
                                                            'relationship': rel}]})['writtenAt']['token']

    def secret(self, db, endpoint=None, token='orchid-test'):
        db.execute(f"CREATE OR REPLACE SECRET auth (TYPE SPICEDB, ENDPOINT {quote(endpoint or self.endpoint)}, TOKEN {quote(token)})")

    def identity(self, who='alice', db=None, **extra):
        (db or self.db).execute('CALL orchid_set_authorization(?)', [json.dumps({'subject_type':'user', 'subject_id':who, **extra})])

    def setUp(self):
        self.db = connect()
        self.secret(self.db)
        self.relationship('team', 'engineering', 'member', 'user', 'alice')
        self.relationship('channel', 'eng', 'viewer', 'team', 'engineering', 'member')
        self.relationship('channel', 'private', 'viewer', 'user', 'bob')
        self.db.execute('''CREATE TABLE messages(id BIGINT, channel_id VARCHAR, body VARCHAR);
          INSERT INTO messages VALUES (1,'eng','one'),(2,'eng','two'),(3,'private','three'),(4,'hidden','four'),(5,NULL,'null');
          CREATE TABLE replies(id BIGINT, src BIGINT, dst BIGINT);
          INSERT INTO replies VALUES (1,1,2),(2,2,3),(3,3,4);
          CREATE PROPERTY GRAPH slack
          VERTEX TABLES(messages KEY(id) LABEL Message PROPERTIES(id,body))
          EDGE TABLES(replies KEY(id) SOURCE KEY(src) REFERENCES messages(id)
                      DESTINATION KEY(dst) REFERENCES messages(id) LABEL REPLY)
          AUTHORIZATION(PROVIDER auth, DEFAULT DENY,
                        VERTEX Message RESOURCE channel KEY(channel_id) REQUIRE view)''')

    def tearDown(self):
        self.db.close()

    def rows(self):
        return self.db.execute('CYPHER slack MATCH (m:Message) RETURN m.id ORDER BY m.id').fetchall()

    def test_user_and_team_channel_access_and_connection_reset(self):
        with self.assertRaisesRegex(Exception, 'session authorization'):
            self.rows()
        self.db.execute("SET GRAPH AUTHORIZATION (SUBJECT_TYPE 'user', SUBJECT_ID 'alice')")
        self.assertEqual(self.rows(), [(1,), (2,)])
        self.assertEqual(self.db.execute('GREMLIN slack g.V().values("id").order()').fetchall(), [(1,), (2,)])
        self.identity('bob')
        self.assertEqual(self.rows(), [(3,)])
        self.identity('carol')
        self.assertEqual(self.rows(), [])
        self.db.execute('RESET GRAPH AUTHORIZATION')
        with self.assertRaisesRegex(Exception, 'session authorization'):
            self.rows()

    def test_standalone_edges_counts_and_paths(self):
        self.identity()
        self.assertEqual(self.db.execute('GREMLIN slack g.E().count()').fetchone(), (1,))
        rows=self.db.execute("SELECT __orchiddb_value_json(q.current) FROM orchid_gremlin('slack',?) q",
                             ["g.V().values('id').fold().unfold().order()"]).fetchall()
        self.assertEqual([json.loads(row[0])['value'] for row in rows], [1,2])
        rows=self.db.execute("SELECT __orchiddb_value_json(q.current) FROM orchid_gremlin('slack',?) q",
                             ["g.V().fold().unfold().values('body').order()"]).fetchall()
        self.assertEqual([json.loads(row[0])['value'] for row in rows], ['one','two'])
        self.assertEqual(self.db.execute('CYPHER slack MATCH (a)-[r:REPLY]->(b) RETURN a.id,b.id').fetchall(), [(1,2)])
        self.assertEqual(self.db.execute('CYPHER slack MATCH (a:Message) RETURN count(a)').fetchone(), (2,))
        self.assertEqual(self.db.execute('CYPHER slack MATCH (a:Message {id:1})-[:REPLY*2..3]->(b) RETURN b.id').fetchall(), [])
        self.identity('bob')
        self.assertEqual(self.db.execute('GREMLIN slack g.E().count()').fetchone(), (0,))

    def test_prepared_queries_identity_switch_and_revocation(self):
        self.identity()
        self.db.execute("PREPARE visible AS SELECT * FROM orchid_cypher('slack','MATCH (m:Message) RETURN m.id ORDER BY m.id')")
        self.assertEqual(self.db.execute('EXECUTE visible').fetchall(), [(1,), (2,)])
        self.identity('bob')
        self.assertEqual(self.db.execute('EXECUTE visible').fetchall(), [(3,)])
        token = self.relationship('channel','private','viewer','user','bob',delete=True)
        self.identity('bob', at_least_as_fresh=token)
        self.assertEqual(self.db.execute('EXECUTE visible').fetchall(), [])

    def test_caveats(self):
        self.relationship('channel','conditional','viewer','user','alice',caveat='region_matches')
        self.db.execute("INSERT INTO messages VALUES (6,'conditional','conditional')")
        self.identity()
        with self.assertRaisesRegex(Exception, 'caveat context'):
            self.rows()
        self.identity(context={'region':'us'})
        self.assertEqual(self.rows(), [(1,), (2,), (6,)])
        self.identity(context={'region':'eu'})
        self.assertEqual(self.rows(), [(1,), (2,)])

    def test_fail_closed_and_secret_redaction(self):
        self.identity()
        self.secret(self.db, token='wrong')
        with self.assertRaisesRegex(Exception, 'SpiceDB permission request failed'):
            self.rows()
        self.secret(self.db, endpoint='http://127.0.0.1:1')
        with self.assertRaisesRegex(Exception, 'failed or timed out'):
            self.rows()
        self.secret(self.db)
        self.assertNotIn('orchid-test', str(self.db.execute('SELECT secret_string FROM duckdb_secrets()').fetchall()))
        self.db.execute('DROP SECRET auth')
        with self.assertRaisesRegex(Exception, 'Missing SpiceDB secret'):
            self.rows()

    def test_read_only_and_advanced_mapping_rejection(self):
        self.identity()
        for query in ["CYPHER slack MATCH (m:Message) SET m.body='changed'", 'GREMLIN slack g.V().drop()']:
            with self.subTest(query=query), self.assertRaisesRegex(Exception, 'read-only'):
                self.db.execute(query)
        with self.assertRaisesRegex(Exception, 'named property graphs'):
            self.db.execute("SELECT * FROM orchid_query('missing', 'RETURN 1')")
        self.assertEqual(self.db.execute('SELECT count(*) FROM messages').fetchone(), (5,))

    def test_functions_and_macro_source_bypass(self):
        self.identity()
        self.db.execute('CREATE MACRO double_id(x) AS x*2')
        self.assertEqual(self.db.execute('CYPHER slack MATCH (m:Message) RETURN double_id(m.id) ORDER BY m.id').fetchall(), [(2,), (4,)])
        self.db.execute('CREATE MACRO steal() AS (SELECT string_agg(body) FROM messages)')
        self.db.execute('CREATE MACRO indirect() AS steal()')
        for function in ['steal()', 'indirect()', 'main.steal()']:
            with self.subTest(function=function), self.assertRaisesRegex(Exception, 'subqueries|additional relations'):
                self.db.execute('CYPHER slack RETURN '+function).fetchall()

    def test_alter_validation_and_default_deny(self):
        self.identity()
        for rule in ['VERTEX Missing PUBLIC', 'VERTEX Message RESOURCE channel KEY(missing) REQUIRE view', 'VERTEX Message PUBLIC, VERTEX Message PUBLIC']:
            with self.subTest(rule=rule), self.assertRaises(Exception):
                self.db.execute(f'ALTER PROPERTY GRAPH slack SET AUTHORIZATION (PROVIDER auth, DEFAULT DENY, {rule})')
        self.assertEqual(self.rows(), [(1,), (2,)])
        self.db.execute("PREPARE policy_change AS SELECT * FROM orchid_cypher('slack','MATCH (m:Message) RETURN m.id')")
        self.db.execute('ALTER PROPERTY GRAPH slack SET AUTHORIZATION (PROVIDER auth, DEFAULT DENY)')
        self.assertEqual(self.db.execute('EXECUTE policy_change').fetchall(), [])
        self.assertEqual(self.rows(), [])

    def test_computed_rag_before_candidates_and_ranking(self):
        from test_computed_edges import SCRIPT, QUERY
        self.db.execute(SCRIPT)
        self.db.execute("ALTER TABLE documents ADD COLUMN channel_id VARCHAR; UPDATE documents SET channel_id=CASE id WHEN 101 THEN 'eng' WHEN 102 THEN 'private' ELSE 'hidden' END")
        self.db.execute('''ALTER PROPERTY GRAPH rag SET AUTHORIZATION (
          PROVIDER auth, DEFAULT DENY, VERTEX Question PUBLIC, VERTEX Author PUBLIC,
          VERTEX Document RESOURCE channel KEY(channel_id) REQUIRE view)''')
        self.identity()
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Quick overview','Alice',1.0)])
        lexical = self.db.execute('CYPHER rag MATCH (:Question {id:1})-[r:RELEVANT_TO]->() RETURN r.lexical').fetchone()
        self.db.execute("INSERT INTO documents SELECT 1000+i,10,'hidden','duckdb',[[100,100]],'hidden' FROM range(100) t(i)")
        self.assertEqual(self.db.execute('CYPHER rag MATCH (:Question {id:1})-[r:RELEVANT_TO]->() RETURN r.lexical').fetchone(), lexical)
        self.assertEqual(self.db.execute("GREMLIN rag g.V().has('Question','id',1).out('RELEVANT_TO').values('title')").fetchall(), [('Quick overview',)])
        self.identity('bob')
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Detailed guide','Bob',2.0)])

    def test_parameterized_cypher_retrieval_authorized_corpus_and_revocation(self):
        from test_cypher_retrieval import SCRIPT, PARAMETERIZED, PARAMETERS
        self.db.execute(SCRIPT)
        self.db.execute("ALTER TABLE chunks ADD COLUMN channel_id VARCHAR; UPDATE chunks SET channel_id=CASE id WHEN 1 THEN 'eng' WHEN 2 THEN 'private' ELSE 'hidden' END")
        self.db.execute('''ALTER PROPERTY GRAPH knowledge SET AUTHORIZATION (
          PROVIDER auth, DEFAULT DENY, VERTEX Content PUBLIC,
          VERTEX Chunk RESOURCE channel KEY(channel_id) REQUIRE view)''')
        with self.assertRaisesRegex(Exception, 'session authorization'):
            self.db.execute(PARAMETERIZED, PARAMETERS).fetchall()
        self.identity()
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [('Quick overview','duckdb duckdb',1.)])
        score_query = 'CYPHER knowledge MATCH (c:Chunk) RETURN text.bm25($q,c.text)'
        score = self.db.execute(score_query, {'q':'duckdb'}).fetchone()
        self.db.execute("INSERT INTO chunks SELECT 100+i,'duckdb',[1,0],[[100,100]],'hidden' FROM range(100) t(i)")
        self.assertEqual(self.db.execute(score_query, {'q':'duckdb'}).fetchone(), score)
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [('Quick overview','duckdb duckdb',1.)])
        self.identity('bob')
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [('Detailed guide','duckdb',2.)])
        self.identity('nobody')
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [])
        self.identity()
        self.relationship('team','engineering','member','user','alice',delete=True)
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [])

    def test_persistence_and_separate_connections(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'auth.duckdb'
            db = connect(path)
            db.execute('CREATE TABLE docs(id BIGINT, channel_id VARCHAR); INSERT INTO docs VALUES (1,\'eng\')')
            db.execute('CREATE PROPERTY GRAPH g VERTEX TABLES(docs KEY(id) LABEL Doc) AUTHORIZATION(PROVIDER auth, DEFAULT DENY, VERTEX Doc RESOURCE channel KEY(channel_id) REQUIRE view)')
            db.close()
            db = connect(path)
            self.secret(db)
            other = db.cursor()
            try:
                self.identity(db=db)
                self.assertEqual(db.execute('CYPHER g MATCH (d:Doc) RETURN d.id').fetchall(), [(1,)])
                with self.assertRaisesRegex(Exception, 'session authorization'):
                    other.execute('CYPHER g MATCH (d:Doc) RETURN d.id').fetchall()
            finally:
                other.close()
                db.close()

    def test_many_rows_and_invalid_resource_ids(self):
        self.identity()
        self.db.execute("INSERT INTO messages SELECT 100+i,'eng','many' FROM range(5000) t(i)")
        before = self.permission_requests()
        self.assertEqual(self.db.execute('CYPHER slack MATCH (m:Message) RETURN count(m)').fetchone(), (5002,))
        self.assertEqual(self.permission_requests()-before, 1)
        resource = "workspace/channel-123"
        self.relationship('channel', resource, 'viewer', 'user', 'alice')
        self.db.execute('INSERT INTO messages VALUES (99999,?,?)', [resource, 'quoted'])
        self.assertEqual(self.db.execute('CYPHER slack MATCH (m:Message {id:99999}) RETURN m.body').fetchall(), [('quoted',)])
        self.db.execute('INSERT INTO messages VALUES (99998,?,?)', ["invalid'object", 'invalid'])
        with self.assertRaisesRegex(Exception, 'SpiceDB permission request failed'):
            self.db.execute('CYPHER slack MATCH (m:Message {id:99998}) RETURN m.body').fetchall()

    @unittest.skipUnless(os.environ.get('ORCHID_EXTERNAL_TESTS') == '1', 'enable real Iceberg/Lance storage tests')
    def test_lance_vertices_and_iceberg_edges(self):
        import pyarrow as pa
        from pyiceberg.catalog.sql import SqlCatalog
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            catalog=SqlCatalog('fixture',uri='sqlite:///'+str(root/'catalog.db'),warehouse=root.as_uri())
            catalog.create_namespace('fixture')
            schema=pa.schema([('id',pa.int64()),('src',pa.int64()),('dst',pa.int64())])
            edges=catalog.create_table('fixture.replies',schema=schema)
            edges.append(pa.table({'id':[1,2,3],'src':[1,2,3],'dst':[2,3,4]},schema=schema))
            self.db.execute('INSTALL iceberg; LOAD iceberg; INSTALL lance; LOAD lance')
            directory=root/'lance';directory.mkdir()
            self.db.execute(f'ATTACH {quote(directory)} AS vectors (TYPE lance)')
            self.db.execute('CREATE TABLE vectors.main.messages(id BIGINT, channel_id VARCHAR, body VARCHAR)')
            self.db.execute('INSERT INTO vectors.main.messages SELECT * FROM messages')
            self.db.execute(f'CREATE VIEW external_replies AS SELECT * FROM iceberg_scan({quote(edges.metadata_location)})')
            self.db.execute('''CREATE PROPERTY GRAPH external_graph
              VERTEX TABLES(vectors.main.messages AS messages KEY(id) LABEL Message PROPERTIES(id,body))
              EDGE TABLES(external_replies KEY(id) SOURCE KEY(src) REFERENCES messages(id)
                DESTINATION KEY(dst) REFERENCES messages(id) LABEL REPLY)
              COMPUTED EDGES(NEAR SOURCE Message DESTINATION Message
                WHERE(source.id<>target.id) ORDER BY(target.id DESC) LIMIT PER SOURCE 1)
              AUTHORIZATION(PROVIDER auth, DEFAULT DENY,
                VERTEX Message RESOURCE channel KEY(channel_id) REQUIRE view)''')
            self.identity()
            self.assertEqual(self.db.execute('CYPHER external_graph MATCH (a)-[:REPLY]->(b) RETURN a.id,b.id').fetchall(),[(1,2)])
            self.assertEqual(self.db.execute('CYPHER external_graph MATCH (a:Message {id:1})-[:NEAR]->(b) RETURN b.id').fetchall(),[(2,)])
            self.identity('bob')
            self.assertEqual(self.db.execute('GREMLIN external_graph g.V().values("id")').fetchall(),[(3,)])
            self.assertEqual(self.db.execute('GREMLIN external_graph g.E().count()').fetchone(),(0,))

    def test_readme_example(self):
        self.relationship('channel', 'engineering', 'viewer', 'team', 'engineering', 'member')
        db=connect()
        try:
            script=(Path(__file__).resolve().parents[2]/'examples/06_authorization.sql').read_text()
            db.execute(script.replace('http://127.0.0.1:8448',self.endpoint).replace('orchid-local-test','orchid-test'))
            with self.assertRaisesRegex(Exception,'session authorization'):
                db.execute('CYPHER slack MATCH (m:Message) RETURN m.id')
            self.identity(db=db)
            self.assertEqual(db.execute('CYPHER slack MATCH (:Message)-[:HAS_CHUNK]->(c) RETURN c.id ORDER BY c.id').fetchall(),[(10,),(20,)])
        finally:
            db.close()

    def test_denied_rows_are_not_evaluated_by_throwing_user_predicates(self):
        self.identity()
        self.db.execute("CREATE MACRO hidden_error(x) AS CASE WHEN x=4 THEN error('hidden-row-evaluated') ELSE true END")
        self.assertEqual(self.db.execute('CYPHER slack MATCH (m:Message) WHERE hidden_error(m.id) RETURN m.id ORDER BY m.id').fetchall(),[(1,),(2,)])

    @classmethod
    def permission_requests(cls):
        with urllib.request.urlopen(cls.metrics_endpoint+'/metrics', timeout=10) as response:
            lines=response.read().decode().splitlines()
        return sum(float(line.rsplit(' ',1)[1]) for line in lines
                   if line.startswith('grpc_server_handled_total{') and 'grpc_method="CheckBulkPermissions"' in line)

    def test_multiple_permission_batches_share_revision(self):
        # More unique resources than DuckDB's vector size exercises exact-snapshot
        # follow-up requests, rather than only cache hits for shared channels.
        for start in range(0,2100,700):
            updates=[{'operation':'OPERATION_TOUCH', 'relationship': {
                'resource':{'objectType':'channel','objectId':f'batch-{i}'}, 'relation':'viewer',
                'subject':{'object':{'objectType':'user','objectId':'alice'}}}} for i in range(start,start+700)]
            self.api('relationships/write',{'updates':updates})
        self.db.execute("INSERT INTO messages SELECT 10000+i,'batch-'||i,'batch' FROM range(2100) t(i)")
        self.identity()
        before=self.permission_requests()
        self.assertEqual(self.db.execute('CYPHER slack MATCH (m:Message) RETURN count(m)').fetchone(),(2102,))
        requests=self.permission_requests()-before
        self.assertGreaterEqual(requests,2)
        self.assertLess(requests,10)
