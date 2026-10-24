"""Production query shapes through registered schemas and real Orchid helpers."""
import json
import unittest
from test_extension import connect


class LakeQueryTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        self.db.execute('CREATE TABLE artifacts(id BIGINT, kind VARCHAR, bytes BIGINT, run_id BIGINT)')
        self.db.execute("INSERT INTO artifacts VALUES (1,'video',10,1),(2,'video',20,1),(3,'frames',30,2),(4,NULL,NULL,2)")
        self.db.execute('CREATE TABLE runs(id BIGINT)')
        self.db.execute('INSERT INTO runs VALUES (1),(2),(3)')
        schema = {'tables': [{'name': 'artifacts'}, {'name': 'runs'}], 'nodes': [
            {'label': 'Artifact', 'table': 'artifacts', 'id': 'id', 'properties': {'kind': 'kind', 'bytes': 'bytes'}},
            {'label': 'Run', 'table': 'runs', 'id': 'id', 'properties': {'id': 'id'}}],
            'edges': [{'label': 'RUN', 'table': 'artifacts', 'id': 'id', 'source_label': 'Artifact',
                       'source': 'id', 'target_label': 'Run', 'target': 'run_id'}]}
        self.db.execute('CALL orchid_register_schema(?, ?)', ['lake', json.dumps(schema)])

    def tearDown(self):
        self.db.close()

    def query(self, text, parameters=None):
        rows = self.db.execute('SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_query(?, ?, parameters := CAST(? AS JSON))',
                               ['lake', text, json.dumps(parameters or {})]).fetchall()
        def plain(value):
            kind, data = value['type'], value.get('value')
            if kind == 'map':
                return {plain(k): plain(v) for k, v in data}
            if kind == 'list':
                return [plain(v) for v in data]
            if kind in ('double', 'float'):
                return float(data)
            return data
        return [tuple(plain(json.loads(value)) for value in row) for row in rows]

    def test_group_filter(self):
        self.assertEqual(self.query('MATCH (a:Artifact)-[:RUN]->(r:Run) WITH a.kind AS kind, count(a) AS artifacts, sum(a.bytes) AS bytes, count(DISTINCT r.id) AS runs WHERE artifacts > 1 RETURN kind, artifacts, bytes, runs ORDER BY bytes DESC LIMIT 5'), [('video', 2, 30, 1)])

    def test_collect_maps_and_null_field(self):
        self.assertEqual(self.query('MATCH (a:Artifact) WITH a.kind AS kind, count(a) AS n WITH collect({kind: kind, n: n}) AS groups UNWIND groups AS g RETURN g.kind AS kind, g.n AS count ORDER BY kind'), [('frames', 1), ('video', 2), (None, 1)])

    def test_string_distinct_uses_cypher_key_helper(self):
        self.assertEqual(self.query('MATCH (a:Artifact) RETURN count(DISTINCT a.kind) AS n'), [(2,)])
        self.assertEqual(self.db.execute("SELECT __orchiddb_lang_cypher_key(NULL) IS NULL, __orchiddb_lang_cypher_key(1) = __orchiddb_lang_cypher_key(1.0), __orchiddb_lang_cypher_key('1') = __orchiddb_lang_cypher_key(1)").fetchone(), (True, True, False))

    def test_cypher_keys_preserve_decimal_precision(self):
        self.assertEqual(self.db.execute("SELECT __orchiddb_lang_cypher_key(9007199254740992.01::DECIMAL(20,2)) = __orchiddb_lang_cypher_key(9007199254740992.02::DECIMAL(20,2)), __orchiddb_lang_cypher_key(1.00::DECIMAL(20,2)) = __orchiddb_lang_cypher_key(1)").fetchone(), (False, True))

    def test_distinct_collection_uses_runtime(self):
        self.assertEqual(self.query('MATCH (a:Artifact) WITH collect(DISTINCT a.kind) AS kinds UNWIND kinds AS kind RETURN kind ORDER BY kind'), [('frames',), ('video',)])

    def test_correlated_aggregates_keep_empty_inputs(self):
        self.assertEqual(self.query('MATCH (r:Run) WITH r ORDER BY r.id LIMIT 3 CALL { WITH r MATCH (a:Artifact)-[:RUN]->(r) RETURN count(a) AS n, sum(a.bytes) AS bytes, min(a.bytes) AS lo, avg(a.bytes) AS mean } RETURN r.id AS id,n,bytes,lo,mean ORDER BY id'), [(1, 2, 30, 10, 15.), (2, 2, 30, 30, 30.), (3, 0, 0, None, None)])

    def test_correlated_empty_collections(self):
        self.assertEqual(self.query('MATCH (r:Run) CALL { WITH r MATCH (a:Artifact)-[:RUN]->(r) RETURN collect(a.kind) AS kinds } RETURN r.id AS id,kinds ORDER BY id'), [(1,['video','video']), (2,['frames']), (3,[])])

    def test_call_preserves_duplicates_and_empty_rowsets(self):
        self.assertEqual(self.query('UNWIND [1,1,2] AS x CALL { WITH x RETURN x+1 AS y } RETURN x,y ORDER BY x'), [(1,2),(1,2),(2,3)])
        self.assertEqual(self.query('UNWIND [1,2,3] AS x CALL { WITH x WITH x WHERE x > 1 RETURN x+1 AS y } RETURN x,y ORDER BY x'), [(2,3),(3,4)])

    def test_call_keeps_unimported_outer_fields_and_per_row_limits(self):
        self.assertEqual(self.query('UNWIND [1,1,2] AS x WITH x, x+10 AS z CALL { WITH x UNWIND [1,2,3] AS n RETURN x+n AS y ORDER BY y DESC LIMIT 2 } RETURN x,z,y ORDER BY x,y'), [(1,11,3),(1,11,3),(1,11,4),(1,11,4),(2,12,4),(2,12,5)])
        self.assertEqual(self.query('UNWIND [1,1,2] AS x CALL { RETURN 7 AS y } RETURN x,y ORDER BY x'), [(1,7),(1,7),(2,7)])
        self.assertEqual(self.query('UNWIND [null,null,2] AS x CALL { WITH x RETURN coalesce(x,7) AS y } RETURN x,y ORDER BY y'), [(2,2),(None,7),(None,7)])

    def test_explicit_schema_does_not_probe_unrelated_sources(self):
        schema = {'tables': [{'name':'runs', 'columns':[{'name':'id','data_type':'int64'}]},
                             {'name':'unavailable_remote_table', 'columns':[{'name':'id','data_type':'int64'}]}],
                  'nodes':[{'label':'Run','table':'runs','id':'id'},
                           {'label':'Other','table':'unavailable_remote_table','id':'id'}]}
        self.db.execute('CALL orchid_register_schema(?, ?)', ['explicit', json.dumps(schema)])
        self.assertEqual(self.db.execute("SELECT * FROM orchid_query('explicit', 'MATCH (r:Run) RETURN count(r)')").fetchall(), [(3,)])
        with self.assertRaises(Exception):
            self.db.execute("SELECT * FROM orchid_query('explicit', 'MATCH (r:Other) RETURN count(r)')").fetchall()

    def test_timezone_aware_schema(self):
        self.db.execute("CREATE TABLE events(id BIGINT, occurred_at TIMESTAMPTZ)")
        self.db.execute("INSERT INTO events VALUES (1, TIMESTAMPTZ '2026-10-21 01:02:03+02')")
        self.db.execute('CALL orchid_register_schema(?, ?)', ['events', json.dumps({'tables': [{'name':'events'}], 'nodes': [{'label':'Event','table':'events','id':'id','properties':{'at':'occurred_at'}}]})])
        actual = self.db.execute("SELECT typeof(happened), happened = TIMESTAMPTZ '2026-10-20 23:02:03+00' FROM orchid_query('events', 'MATCH (e:Event) RETURN e.at AS happened')").fetchone()
        self.assertEqual(actual, ('TIMESTAMP WITH TIME ZONE', True))

    def test_json_parameters_and_nested_call(self):
        self.assertEqual(self.query('WITH $value AS x CALL { WITH x CALL { WITH x RETURN x+1 AS y } RETURN y+1 AS z } RETURN z', {'value': 7}), [(9,)])
        self.assertEqual(self.query('RETURN $value AS value', {'value': "O'Reilly; } //"}), [("O'Reilly; } //",)])
        self.assertEqual(self.query('WITH 2 AS x CALL { WITH x // a comment\n RETURN x AS y } RETURN y'), [(2,)])
