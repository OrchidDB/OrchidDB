"""Scalar-only extension contract, exercised against an actual DuckDB host."""
import json
import os
from pathlib import Path
import unittest
import duckdb

class ScalarExtensionTests(unittest.TestCase):
    def setUp(self):
        extension = Path(os.environ.get('ORCHID_EXTENSION', Path(__file__).resolve().parents[1] / 'build/orchid.duckdb_extension'))
        self.db = duckdb.connect(config={'allow_unsigned_extensions': 'true'})
        self.db.execute("LOAD '" + str(extension).replace("'", "''") + "'")

    def tearDown(self):
        self.db.close()

    def test_no_query_or_catalog_interface(self):
        functions = self.db.execute("SELECT function_name, function_type FROM duckdb_functions() WHERE starts_with(function_name, 'orchid_') OR starts_with(function_name, '__orchiddb_')").fetchall()
        self.assertTrue(functions)
        self.assertTrue(all(kind == 'scalar' for _, kind in functions), functions)
        self.assertFalse({'orchid_query', 'orchid_cypher', 'orchid_register_schema', 'orchid_register_catalog', 'orchid_sparql_update'} & {name for name, _ in functions})

    def test_typed_arrow_batches(self):
        rows = self.db.execute("SELECT __orchiddb_value_json(orchid_cypher_add(orchid_cypher_pack(i), orchid_cypher_pack(3))) FROM range(10000) t(i) ORDER BY i").fetchall()
        self.assertEqual([json.loads(row[0])['value'] for row in rows], list(range(3, 10003)))
        self.assertIsNone(self.db.execute("SELECT orchid_cypher_add(orchid_cypher_pack(NULL), orchid_cypher_pack(3))").fetchone()[0])

    def test_nested_nulls_and_error_recovery(self):
        value = json.loads(self.db.execute(
            "SELECT __orchiddb_value_json(orchid_cypher_pack([[1,NULL],[],NULL]))"
        ).fetchone()[0])
        self.assertEqual(value, {'type': 'list', 'value': [
            {'type': 'list', 'value': [{'type': 'int', 'value': 1}, {'type': 'null', 'value': None}]},
            {'type': 'list', 'value': []}, {'type': 'null', 'value': None}]})
        with self.assertRaisesRegex(duckdb.InvalidInputException, 'Divide by zero'):
            self.db.execute("SELECT orchid_cypher_divide(orchid_cypher_pack(1), orchid_cypher_pack(0))").fetchall()
        value = json.loads(self.db.execute(
            "SELECT __orchiddb_value_json(orchid_cypher_add(orchid_cypher_pack(2), orchid_cypher_pack(3)))"
        ).fetchone()[0])
        self.assertEqual(value, {'type': 'int', 'value': 5})

    def test_float_batch_nulls_and_boundaries(self):
        values = [None, '', 'foo', '2.9', '__1__2__', '\u00a01.5\u3000', 'NaN', 'Infinity', '1e999', '1e-999']
        self.db.execute('CREATE TABLE input(id INTEGER, raw VARCHAR)')
        self.db.executemany('INSERT INTO input VALUES (?, ?)', enumerate(values))
        actual = self.db.execute('SELECT __orchiddb_cypher_to_float_v1(raw) FROM input ORDER BY id').fetchall()
        self.assertEqual(actual, [(v,) for v in [None, None, None, 2.9, 12.0, 1.5, None, None, None, 0.0]])
        self.assertEqual(self.db.execute("SELECT sum(__orchiddb_cypher_to_float_v1(i::VARCHAR)) FROM range(10000) t(i)").fetchone(), (49995000.0,))

if __name__ == '__main__':
    unittest.main()
