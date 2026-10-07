"""Customer API: schema is connection-local and query text is always separate."""
import json
import unittest
from test_extension import connect


class SchemaConnectionTests(unittest.TestCase):
    def test_reuse_parameters_and_isolation(self):
        with connect() as db:
            db.execute('CALL orchid_register_schema(?, ?)', ['empty', '{"tables":[]}'])
            self.assertEqual(db.execute("SELECT * FROM orchid_query('empty', 'RETURN $value AS n', parameters := {value: 7})").fetchall(), [(7,)])
            self.assertEqual(db.execute("SELECT * FROM orchid_query('empty', 'RETURN 9 AS n')").fetchall(), [(9,)])
            with connect() as other:
                with self.assertRaisesRegex(Exception, 'Unknown Orchid schema'):
                    other.execute("SELECT * FROM orchid_query('empty', 'RETURN 1')")
            with self.assertRaises(Exception):
                db.execute("SELECT * FROM orchid_compile('{}')")
            with self.assertRaises(Exception):
                db.execute("SELECT * FROM orchid_query('{}')")

    def test_schema_rejects_query_fields_and_explain_does_not_register(self):
        with connect() as db:
            for field in ('query', 'parameters', 'language', 'bindings', 'authorization'):
                with self.subTest(field=field), self.assertRaisesRegex(Exception, 'belongs'):
                    db.execute('CALL orchid_register_schema(?, ?)', ['invalid', json.dumps({'tables': [], field: 'x'})])
            db.execute("EXPLAIN CALL orchid_register_schema('future', '{\"tables\":[]}')").fetchall()
            with self.assertRaisesRegex(Exception, 'Unknown Orchid schema'):
                db.execute("SELECT * FROM orchid_query('future', 'RETURN 1')")
