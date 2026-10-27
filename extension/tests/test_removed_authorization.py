"""Removed built-in policies must fail closed; ordinary graphs still work."""
import json
import unittest

from test_extension import connect, quote


class RemovedAuthorizationTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        self.db.execute('CREATE TABLE users(id BIGINT)')
        self.db.execute('INSERT INTO users VALUES (1)')

    def tearDown(self):
        self.db.close()

    def test_removed_functions_and_secrets(self):
        names = self.db.execute("SELECT function_name FROM duckdb_functions() WHERE function_name IN ('orchid_set_authorization', 'orchid_graph_authorization', '__orchid_spicedb_check')").fetchall()
        self.assertEqual(names, [])
        with self.assertRaises(Exception):
            self.db.execute("CREATE SECRET auth (TYPE SPICEDB, ENDPOINT 'http://localhost:1', TOKEN 'unused')")

    def test_removed_syntax(self):
        for sql in [
            "SET GRAPH AUTHORIZATION (subject_type 'user', subject_id 'alice')",
            'RESET GRAPH AUTHORIZATION',
            'ALTER PROPERTY GRAPH g SET AUTHORIZATION (PROVIDER auth, DEFAULT DENY)',
            'CREATE PROPERTY GRAPH g VERTEX TABLES (users KEY (id)) AUTHORIZATION (PROVIDER auth, DEFAULT DENY)',
        ]:
            # DuckDB reports its parser error when an override rejects syntax.
            with self.subTest(sql=sql), self.assertRaises(Exception):
                self.db.execute(sql)

    def test_persisted_policy_rejected_and_unprotected_graph_works(self):
        self.db.execute('CREATE PROPERTY GRAPH g VERTEX TABLES (users KEY (id) LABEL User)')
        self.assertEqual(self.db.execute("SELECT * FROM orchid_cypher('g', 'MATCH (u:User) RETURN u.id')").fetchall(), [(1,)])
        definition = json.loads(self.db.execute('SELECT * FROM __orchid_graph_g').fetchone()[0])
        definition['authorization'] = {'provider': 'auth', 'vertices': []}
        with self.assertRaisesRegex(Exception, 'authorization was removed'):
            self.db.execute('CREATE OR REPLACE VIEW __orchid_graph_g AS SELECT * FROM orchid_graph_definition(' + quote(json.dumps(definition)) + ')')
        self.assertEqual(self.db.execute("SELECT * FROM orchid_cypher('g', 'MATCH (u:User) RETURN u.id')").fetchall(), [(1,)])


if __name__ == '__main__':
    unittest.main()
