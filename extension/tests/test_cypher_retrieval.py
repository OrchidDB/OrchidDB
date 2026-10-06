"""Ordinary parameterized Cypher reuses mapped scans and existing scoring kernels."""
import math
from pathlib import Path
import unittest

from test_extension import connect

SCRIPT = (Path(__file__).resolve().parents[2] / 'examples/07_parameterized_rag.sql').read_text()
QUERY = SCRIPT[SCRIPT.index('CYPHER knowledge'):SCRIPT.index(';\n-- Detailed guide')]
PARAMETERIZED = QUERY.replace("'duckdb'", '$query_text').replace('[1.0,0.0], c.embedding', '$query_embedding, c.embedding').replace('[[1.0,0.0],[0.0,1.0]]', '$query_tokens')
PARAMETERS = dict(query_text='duckdb', query_embedding=[1., 0.], query_tokens=[[1., 0.], [0., 1.]])


class CypherRetrievalTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        self.db.execute(SCRIPT)

    def tearDown(self):
        self.db.close()

    def test_hybrid_candidates_reranking_and_traversal(self):
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [('Detailed guide', 'duckdb', 2.)])
        self.assertEqual(self.db.execute(PARAMETERIZED, dict(PARAMETERS, query_text='gardening', query_embedding=[0.,1.])).fetchall(), [('Gardening', 'gardening flowers', 10.)])
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [('Detailed guide', 'duckdb', 2.)])
        # A later filter must not refill an earlier candidate/reranking limit.
        query = PARAMETERIZED.replace('RETURN document.title', 'WHERE document.id = 10 RETURN document.title')
        self.assertEqual(self.db.execute(query, PARAMETERS).fetchall(), [])

    def test_corpus_independent_of_filters_limits_aliases_and_join_multiplicity(self):
        query = "CYPHER knowledge MATCH (c:Chunk) RETURN c.id, text.bm25($q,c.text) AS score ORDER BY c.id"
        expected = dict(self.db.execute(query, {'q':'duckdb'}).fetchall())
        self.assertAlmostEqual(expected[1], math.log(1 + 1.5/2.5) * 2 * 2.2 / (2 + 1.2*(.25 + .75*2/(5/3))))
        for prefix in [
            'MATCH (c:Chunk) WHERE c.id=1',
            'MATCH (c:Chunk) WITH c ORDER BY c.id LIMIT 1',
            'MATCH (original:Chunk) WITH original AS c WHERE c.id=1',
            'MATCH (original:Chunk) WITH original.text AS body, original.id AS id WHERE id=1',
            'MATCH (c:Chunk) WITH c, {x:c.id} AS payload WHERE c.id=1',
            'MATCH (c:Chunk) WITH c,count(*) AS n WHERE c.id=1',
            'MATCH (c:Chunk) WITH DISTINCT c WHERE c.id=1',
        ]:
            expression = 'body' if ' AS body' in prefix else 'c.text'
            with self.subTest(prefix=prefix):
                score = self.db.execute(f'CYPHER knowledge {prefix} RETURN text.bm25($q,{expression})', {'q':'duckdb'}).fetchone()[0]
                self.assertAlmostEqual(score, expected[1])
        self.db.execute('INSERT INTO has_chunk VALUES (4,20,1)')
        rows = self.db.execute("CYPHER knowledge MATCH (:Content)-[:HAS_CHUNK]->(c:Chunk) RETURN c.id,text.bm25($q,c.text) ORDER BY c.id", {'q':'duckdb'}).fetchall()
        self.assertEqual(rows, [(1,expected[1]),(1,expected[1]),(2,expected[2]),(3,expected[3])])

    def test_vectors_use_existing_functions_and_parameter_codec(self):
        self.assertEqual(self.db.execute('CYPHER knowledge RETURN vector.dot($v,$v)', {'v':[1.,0.]}).fetchone(), (1.,))
        for name, expected in [('dot',1.),('cosine_similarity',1.),('l2_distance',0.)]:
            with self.subTest(function=name):
                self.assertAlmostEqual(self.db.execute(f'CYPHER knowledge MATCH (c:Chunk {{id:1}}) RETURN vector.{name}($v,c.embedding)', {'v':[1.,0.]}).fetchone()[0], expected)
        self.assertIsNone(self.db.execute('CYPHER knowledge MATCH (c:Chunk {id:1}) RETURN vector.cosine_similarity($v,c.embedding)', {'v':[0.,0.]}).fetchone()[0])
        with self.assertRaises(Exception):
            self.db.execute('CYPHER knowledge MATCH (c:Chunk) RETURN vector.dot($v,c.embedding)', {'v':[1.,2.,3.]}).fetchall()
        with self.assertRaises(Exception):
            self.db.execute(PARAMETERIZED, dict(PARAMETERS, query_tokens=[])).fetchall()

    def test_bm25_empty_query_null_document_and_empty_source(self):
        self.db.execute('INSERT INTO chunks VALUES (4,NULL,NULL,NULL)')
        rows = self.db.execute("CYPHER knowledge MATCH (c:Chunk) RETURN c.id,text.bm25($q,c.text) ORDER BY c.id", {'q':''}).fetchall()
        self.assertEqual(rows, [(1,0.),(2,0.),(3,0.),(4,None)])
        self.db.execute('DELETE FROM chunks')
        self.assertEqual(self.db.execute(PARAMETERIZED, PARAMETERS).fetchall(), [])

    def test_bm25_rejects_missing_source_lineage(self):
        with self.assertRaisesRegex(Exception, 'requires a mapped vertex text property'):
            self.db.execute("CYPHER knowledge MATCH (c:Chunk) RETURN text.bm25($q, upper(c.text))", {'q':'duckdb'}).fetchall()
        # An edge type may share a vertex label; it must not borrow that
        # vertex's corpus based solely on the spelling of the label.
        self.db.execute("ALTER TABLE has_chunk ADD COLUMN text VARCHAR DEFAULT 'duckdb'")
        ddl = SCRIPT[SCRIPT.index('CREATE PROPERTY GRAPH'):SCRIPT.index('CYPHER knowledge')]
        self.db.execute(ddl.replace('GRAPH knowledge','GRAPH mixed').replace('LABEL HAS_CHUNK','LABEL Chunk PROPERTIES(text)'))
        with self.assertRaisesRegex(Exception, 'requires a mapped vertex text property'):
            self.db.execute("CYPHER mixed MATCH ()-[e:Chunk]->() RETURN text.bm25($q,e.text)", {'q':'duckdb'}).fetchall()

    def test_grouped_node_retains_scoring_and_traversal(self):
        query = PARAMETERIZED.replace('MATCH (c:Chunk)', 'MATCH (c:Chunk) WITH c, count(*) AS copies', 1)
        self.assertEqual(self.db.execute(query, PARAMETERS).fetchall(), [('Detailed guide','duckdb',2.)])
        rows = self.db.execute('CYPHER knowledge MATCH (c:Chunk) WITH c,count(*) AS copies RETURN c.id,copies ORDER BY c.id').fetchall()
        self.assertEqual(rows, [(1,1),(2,1),(3,1)])

    def test_mapped_view_and_live_corpus_in_caller_transaction(self):
        self.db.execute('''CREATE VIEW searchable AS SELECT id, upper(text) AS words FROM chunks WHERE id<3;
          CREATE PROPERTY GRAPH view_search VERTEX TABLES(searchable KEY(id) LABEL Passage PROPERTIES(id,words))''')
        query = 'CYPHER view_search MATCH (p:Passage) WHERE p.id=1 RETURN text.bm25($q,p.words)'
        before = self.db.execute(query, {'q':'duckdb'}).fetchone()[0]
        self.assertAlmostEqual(before, math.log(1 + .5/2.5) * 2 * 2.2 / (2 + 1.2*(.25 + .75*2/1.5)))
        self.db.execute('BEGIN')
        self.db.execute("UPDATE chunks SET text='gardening flowers' WHERE id=2")
        self.assertGreater(self.db.execute(query, {'q':'duckdb'}).fetchone()[0], before)
        self.db.execute('ROLLBACK')
        self.assertEqual(self.db.execute(query, {'q':'duckdb'}).fetchone()[0], before)


if __name__ == '__main__': unittest.main()
