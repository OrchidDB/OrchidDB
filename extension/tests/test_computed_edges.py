"""Native computed-edge DDL reuses the existing retrieval and ranking planner."""
import os
from pathlib import Path
import tempfile
import unittest

from test_extension import connect, quote

EXAMPLE = Path(__file__).resolve().parents[2] / 'examples/05_rag.sql'
SCRIPT = EXAMPLE.read_text()
DDL = SCRIPT[SCRIPT.index('CREATE PROPERTY GRAPH rag'):SCRIPT.index('\n-- BM25')].strip().rstrip(';')
QUERY = '''CYPHER rag MATCH (q:Question)-[r:RELEVANT_TO]->(d:Document)-[:WRITTEN_BY]->(a:Author)
WHERE q.id = 1 RETURN d.title AS title, a.name AS author, r.score AS score'''


class ComputedEdgeTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        self.db.execute(SCRIPT)

    def tearDown(self):
        self.db.close()

    def test_rag_candidates_reranking_and_stored_edges(self):
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Detailed guide', 'Bob', 2.0)])
        self.assertEqual(self.db.execute("GREMLIN rag g.V().has('Question','id',1).out('RELEVANT_TO').values('title')").fetchall(), [('Detailed guide',)])
        self.assertEqual(self.db.execute('CYPHER rag MATCH (q:Question)-[r:RELEVANT_TO]->(d) RETURN q.id,d.id,r.score ORDER BY q.id').fetchall(), [(1,102,2.0),(2,104,20.0)])
        self.assertGreater(self.db.execute('CYPHER rag MATCH (q:Question {id:1})-[r:RELEVANT_TO]->() RETURN r.lexical').fetchone()[0], 0)
        self.assertEqual(self.db.execute("DESCRIBE PROPERTY GRAPH rag").fetchall()[-1], ('computed_edges','RELEVANT_TO',None,None,'Question','Document'))
        self.assertEqual(self.db.execute('SELECT count(*) FROM written_by').fetchone(), (4,))
        plan = str(self.db.execute('EXPLAIN '+QUERY).fetchall()).upper()
        self.assertIn('WINDOW', plan)
        self.assertIn('DOCUMENTS', plan)

    def test_reverse_traversal_and_downstream_filter_do_not_refill_top_k(self):
        self.assertEqual(self.db.execute('CYPHER rag MATCH (d:Document {id:102})<-[r:RELEVANT_TO]-(q) RETURN q.id').fetchall(), [(1,)])
        self.assertEqual(self.db.execute('CYPHER rag MATCH (:Question {id:1})-[:RELEVANT_TO]->(d) WHERE d.id=101 RETURN d.title').fetchall(), [])
        self.assertEqual(self.db.execute("GREMLIN rag g.V().has('Document','id',102).in('RELEVANT_TO').values('id')").fetchall(), [(1,)])

    def test_live_sources_and_caller_transaction(self):
        self.db.execute('BEGIN')
        self.db.execute('UPDATE documents SET tokens=[[3,3]] WHERE id=101')
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Quick overview','Alice',6.0)])
        self.db.execute('ROLLBACK')
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Detailed guide','Bob',2.0)])

    def test_question_parameters_and_source_updates(self):
        query = 'CYPHER rag MATCH (q:Question)-[:RELEVANT_TO]->(d) WHERE q.id=$question RETURN d.id'
        self.assertEqual(self.db.execute(query, {'question':1}).fetchall(), [(102,)])
        self.assertEqual(self.db.execute(query, {'question':2}).fetchall(), [(104,)])
        self.db.execute('BEGIN')
        self.db.execute("CYPHER rag MATCH (q:Question {id:1}) SET q.text='gardening'").fetchall()
        self.assertEqual(self.db.execute('SELECT text FROM questions WHERE id=1').fetchone(), ('gardening',))
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Unrelated','Alice',10.0)])
        self.db.execute('ROLLBACK')
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Detailed guide','Bob',2.0)])

    def test_invalid_definitions_fail_atomically(self):
        invalid = [
            DDL.replace('SOURCE Question','SOURCE Missing'),
            DDL.replace('source.text','source.missing'),
            DDL.replace(' AS score',' AS lexical').replace('(score DESC)','(lexical DESC)'),
            DDL.replace('ORDER BY (score DESC)',''),
            DDL.replace('RELEVANT_TO SOURCE','WRITTEN_BY SOURCE'),
            DDL.replace('LIMIT PER SOURCE 1','LIMIT PER SOURCE -1'),
            DDL.replace('vector.maxsim(source.tokens, target.tokens)','random()'),
            DDL.replace(' AS score',''),
            DDL.replace('LIMIT PER SOURCE 2',''),
            DDL.replace('WHERE (source.tenant_id = target.tenant_id)','WHERE (source.tenant_id = target.tenant_id) WHERE (true)'),
        ]
        for sql in invalid:
            with self.subTest(sql=sql), self.assertRaises(Exception):
                self.db.execute(sql.replace('CREATE PROPERTY','CREATE OR REPLACE PROPERTY',1))
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Detailed guide','Bob',2.0)])

    def test_simple_computed_edge_without_stored_edges_and_quoted_names(self):
        self.db.execute('''CREATE PROPERTY GRAPH nearby
          VERTEX TABLES (documents KEY(id) LABEL Document PROPERTIES(id,tenant_id))
          COMPUTED EDGES (
            "Same tenant" SOURCE Document DESTINATION Document
              WHERE (source.tenant_id = target.tenant_id AND source.id <> target.id)
              PROPERTIES (abs(source.id - target.id) AS "id gap")
              ORDER BY ("id gap" ASC NULLS LAST) LIMIT PER SOURCE 1 RETRIEVAL EXACT,
            NEVER SOURCE Document DESTINATION Document
              ORDER BY (target.id) LIMIT PER SOURCE 0
          )''')
        self.assertEqual(self.db.execute('CYPHER nearby MATCH (a:Document {id:102})-[e:`Same tenant`]->(b) RETURN b.id,e.`id gap`').fetchall(), [(101,1)])
        self.assertEqual(self.db.execute('CYPHER nearby MATCH ()-[:NEVER]->() RETURN count(*)').fetchone(), (0,))

    def test_computed_edges_are_read_only(self):
        for query in (
            'CYPHER rag MATCH ()-[r:RELEVANT_TO]->() SET r.score=99',
            'CYPHER rag MATCH (q:Question)-[r:RELEVANT_TO]->(d:Document) DELETE r',
            "GREMLIN rag g.E().hasLabel('RELEVANT_TO').property('score',99)",
        ):
            with self.subTest(query=query), self.assertRaisesRegex(Exception, 'read-only'):
                self.db.execute(query)
        self.assertEqual(self.db.execute(QUERY).fetchall(), [('Detailed guide','Bob',2.0)])

    def test_reopen_and_schema_revalidation(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'rag.duckdb'
            db = connect(path)
            db.execute(SCRIPT)
            db.close()
            db = connect(path)
            try:
                self.assertEqual(db.execute(QUERY).fetchall(), [('Detailed guide','Bob',2.0)])
                db.execute('ALTER TABLE documents DROP COLUMN tokens')
                with self.assertRaises(Exception):
                    db.execute(QUERY)
            finally:
                db.close()


@unittest.skipUnless(os.environ.get('ORCHID_EXTERNAL_TESTS') == '1', 'enable real storage fixtures')
class ComputedEdgeStorageTests(unittest.TestCase):
    def test_rag_over_lance_documents_and_iceberg_stored_edges(self):
        import pyarrow as pa
        from pyiceberg.catalog.sql import SqlCatalog
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalog = SqlCatalog('fixture',uri='sqlite:///'+str(root/'catalog.db'),warehouse=root.as_uri())
            catalog.create_namespace('fixture')
            schema = pa.schema([('id',pa.int64()),('document_id',pa.int64()),('author_id',pa.int64())])
            edges = catalog.create_table('fixture.written_by',schema=schema)
            edges.append(pa.table({'id':[1,2,3,4],'document_id':[101,102,103,104],'author_id':[1,2,1,2]},schema=schema))
            db = connect()
            try:
                db.execute('INSTALL iceberg; LOAD iceberg; INSTALL lance; LOAD lance')
                location = root/'lance';location.mkdir()
                db.execute(f'ATTACH {quote(location)} AS vectors (TYPE lance)')
                script = SCRIPT.replace('CREATE TABLE documents(', 'CREATE TABLE vectors.main.documents(').replace('INSERT INTO documents ', 'INSERT INTO vectors.main.documents ')
                script = script.replace('CREATE TABLE written_by(id BIGINT PRIMARY KEY, document_id BIGINT, author_id BIGINT);',f'CREATE VIEW written_by AS SELECT * FROM iceberg_scan({quote(edges.metadata_location)});')
                script = script.replace("INSERT INTO written_by VALUES (1,101,1), (2,102,2), (3,103,1), (4,104,2);",'')
                script = script.replace('documents KEY (id) LABEL','vectors.main.documents AS documents KEY (id) LABEL')
                # Lance does not enforce DuckDB primary-key constraints.
                script = script.replace('CREATE TABLE vectors.main.documents(id BIGINT PRIMARY KEY','CREATE TABLE vectors.main.documents(id BIGINT')
                db.execute(script)
                self.assertEqual(db.execute(QUERY).fetchall(), [('Detailed guide','Bob',2.0)])
                plan = str(db.execute('EXPLAIN '+QUERY).fetchall()).upper()
                self.assertIn('LANCE',plan)
                self.assertTrue('ICEBERG' in plan or 'PARQUET' in plan,plan)
                # The same physical sources also support request parameters and
                # ordinary Cypher ranking, without a computed relationship.
                retrieval = '''CYPHER rag MATCH (d:Document) WHERE d.tenant_id=$tenant
                  WITH d, text.bm25($query_text,d.body) AS lexical
                  ORDER BY lexical DESC LIMIT 2
                  WITH d, vector.maxsim($query_tokens,d.tokens) AS score
                  ORDER BY score DESC LIMIT 1
                  MATCH (d)-[:WRITTEN_BY]->(a:Author)
                  RETURN d.title,a.name,score'''
                parameters = dict(tenant=10,query_text='duckdb',query_tokens=[[1.,0.],[0.,1.]])
                self.assertEqual(db.execute(retrieval,parameters).fetchall(), [('Detailed guide','Bob',2.)])
                self.assertEqual(db.execute(retrieval,dict(parameters,tenant=20)).fetchall(), [('Other tenant','Bob',20.)])
            finally:
                db.close()


if __name__ == '__main__': unittest.main()
