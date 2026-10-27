"""Required live integrations. Run with the execution-integration fixture runner."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from test_extension import connect, quote


@unittest.skipUnless(os.environ.get('ORCHID_EXECUTION_INTEGRATION') == '1', 'run execution-integration/run.py')
class ExecutionExtensionTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()

    def tearDown(self):
        self.db.close()

    def register(self, name, table):
        schema = {'tables': [{'name': table}], 'nodes': [{'label': 'Doc', 'table': table, 'id': 'id',
            'properties': {'id': 'id', 'tenant': 'tenant'}}]}
        self.db.execute('CALL orchid_register_schema(?, ?)', [name, json.dumps(schema)])

    def test_postgres_extension_and_caller_session(self):
        self.db.execute('INSTALL postgres; LOAD postgres')
        dsn = os.environ['ORCHID_TEST_PG_DSN']
        self.db.execute('ATTACH ' + quote(dsn) + ' AS pg (TYPE POSTGRES, READ_ONLY)')
        self.register('pg_graph', 'pg.public.documents')
        result = self.db.execute("SELECT * FROM orchid_query('pg_graph', 'MATCH (d:Doc) WHERE d.tenant = $tenant RETURN d.id AS id ORDER BY id', parameters := '{\"tenant\":\"alice\"}'::JSON)").fetchall()
        self.assertEqual(result, [(1,), (3,)])
        self.assertEqual(self.db.execute("SELECT * FROM orchid_query('pg_graph', 'MATCH (d:Doc) RETURN count(d) AS n')").fetchall(), [(3,)])
        self.assertEqual(self.db.execute('SELECT sum(amount) FROM pg.public.documents').fetchone()[0], 37.50)

    def test_parquet_projection_filter_and_loaded_functions(self):
        with tempfile.TemporaryDirectory() as folder:
            path = str(Path(folder) / 'documents.parquet')
            self.db.execute("COPY (SELECT * FROM (VALUES (1::BIGINT, 'alice'), (2::BIGINT,'bob'), (3::BIGINT, 'alice')) t(id,tenant)) TO " + quote(path) + ' (FORMAT PARQUET)')
            self.db.execute('CREATE VIEW lake_documents AS SELECT * FROM read_parquet(' + quote(path) + ')')
            self.register('lake_graph', 'lake_documents')
            self.assertEqual(self.db.execute("SELECT * FROM orchid_query('lake_graph', ?)", ["MATCH (d:Doc) WHERE d.tenant = 'alice' RETURN sum(d.id) AS total"]).fetchall(), [(4,)])
            self.db.execute('LOAD json; LOAD icu')
            self.db.execute('CREATE PROPERTY GRAPH lake VERTEX TABLES (lake_documents KEY(id) LABEL Doc PROPERTIES(id,tenant))')
            expected = self.db.execute("SELECT icu_sort_key('alice','en')").fetchone()[0]
            self.assertEqual(self.db.execute("CYPHER lake MATCH (d:Doc) WHERE d.id = 1 RETURN icu_sort_key(d.tenant,'en')").fetchall(), [(expected,)])
            self.assertEqual(self.db.execute("CYPHER lake RETURN json_extract_string('{\"label\":\"orchid\"}', '$.label')").fetchall(), [('orchid',)])
