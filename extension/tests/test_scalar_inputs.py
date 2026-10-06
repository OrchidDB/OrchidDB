"""Scalar input stages reuse the caller's DuckDB functions and typed vectors.

Enable the small live API check with ORCHID_OPENAI_TESTS=1 and OPENAI_API_KEY.
Only the synthetic strings below are sent to OpenAI. No credentials are stored.
"""
import json
import math
import os
import unittest
import urllib.error
import urllib.request

from test_extension import connect

QUERY = '''CYPHER embeddings
WITH embed($query_text) AS query_vector
MATCH (d:Document)
RETURN d.id AS id, vector.cosine_similarity(query_vector,d.embedding) AS score
ORDER BY score DESC, id'''


def fixture(db, vectors):
    db.execute('CREATE TABLE embedding_documents(id BIGINT, embedding DOUBLE[])')
    db.executemany('INSERT INTO embedding_documents VALUES (?,?)', list(enumerate(vectors,1)))
    db.execute('''CREATE PROPERTY GRAPH embeddings VERTEX TABLES
      (embedding_documents KEY(id) LABEL Document PROPERTIES(id,embedding))''')


class ScalarInputTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        fixture(self.db, [[1.,0.],[0.,1.]])
        self.calls = []

        def embed(text):
            self.calls.append(text)
            if text == 'fail':
                raise ValueError('embedding service failed')
            return [1.,0.] if text == 'release' else [0.,1.]

        # Network calls are side effects: never let the host constant-fold them
        # while binding or explaining. WITH defines a singleton execution stage.
        self.db.create_function('embed', embed, ['VARCHAR'], 'DOUBLE[]', side_effects=True)

    def tearDown(self):
        self.db.close()

    def test_input_is_evaluated_once_at_execution_and_rebound_per_query(self):
        self.db.execute('EXPLAIN '+QUERY, {'query_text':'release'}).fetchall()
        self.assertEqual(self.calls, [])
        self.assertEqual(self.db.execute(QUERY, {'query_text':'release'}).fetchall(), [(1,1.),(2,0.)])
        self.assertEqual(self.calls, ['release'])
        self.assertEqual(self.db.execute(QUERY, {'query_text':'garden'}).fetchall(), [(2,1.),(1,0.)])
        self.assertEqual(self.calls, ['release','garden'])

    def test_nested_scalar_input_keeps_one_evaluation(self):
        query = QUERY.replace('embed($query_text)', 'embed(lower($query_text))')
        self.db.execute(query, {'query_text':'RELEASE'}).fetchall()
        self.assertEqual(self.calls, ['release'])

    def test_failure_is_not_retried_or_replaced_with_a_vector(self):
        with self.assertRaisesRegex(Exception, 'embedding service failed'):
            self.db.execute(QUERY, {'query_text':'fail'}).fetchall()
        self.assertEqual(self.calls, ['fail'])

    def test_null_input_retains_duckdb_null_semantics(self):
        self.assertEqual(self.db.execute(QUERY, {'query_text':None}).fetchall(), [(1,None),(2,None)])
        self.assertEqual(self.calls, [])


class OpenAIEmbedding:
    """A test UDF implementation; the engine has no OpenAI-specific behavior."""
    def __init__(self):
        self.calls = 0

    def vectors(self, texts):
        request = urllib.request.Request(
            'https://api.openai.com/v1/embeddings',
            data=json.dumps(dict(model='text-embedding-3-small', input=texts,
                                 dimensions=16, encoding_format='float')).encode(),
            headers={'Authorization':'Bearer '+os.environ['OPENAI_API_KEY'],
                     'Content-Type':'application/json'},
        )
        self.calls += 1
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                result = json.load(response)
        except urllib.error.HTTPError as error:
            status = error.code
            error.close()
            raise RuntimeError(f'Embedding request failed (HTTP {status})') from None
        records = sorted(result['data'], key=lambda record: record['index'])
        if [record['index'] for record in records] != list(range(len(texts))):
            raise ValueError('Embedding response has unexpected input indexes')
        vectors = [record['embedding'] for record in records]
        if any(len(vector) != 16 or not all(math.isfinite(v) for v in vector) for vector in vectors):
            raise ValueError('Embedding response has invalid vector dimensions or components')
        return vectors

    def __call__(self, text):
        return self.vectors([text])[0]


@unittest.skipUnless(os.environ.get('ORCHID_OPENAI_TESTS') == '1', 'opt-in live OpenAI API check')
class LiveOpenAIEmbeddingTests(unittest.TestCase):
    def test_real_embedding_enters_duckdb_scoring_as_a_typed_vector(self):
        if not os.environ.get('OPENAI_API_KEY'):
            self.fail('OPENAI_API_KEY is required for the opted-in live test')
        texts = ['The release is blocked until the canary rollout passes.',
                 'Plant tomatoes in a sunny garden with well drained soil.']
        embed = OpenAIEmbedding()
        db = connect()
        try:
            fixture(db, embed.vectors(texts))
            db.create_function('embed', embed, ['VARCHAR'], 'DOUBLE[]', side_effects=True)
            db.execute('EXPLAIN '+QUERY, {'query_text':texts[0]}).fetchall()
            self.assertEqual(embed.calls, 1, 'EXPLAIN must not make an API request')
            for index, text in enumerate(texts,1):
                rows = db.execute(QUERY, {'query_text':text}).fetchall()
                self.assertEqual(rows[0][0], index)
                self.assertAlmostEqual(rows[0][1], 1., places=5)
                self.assertEqual(embed.calls, index+1, 'one request per execution')
        finally:
            db.close()


if __name__ == '__main__': unittest.main()
