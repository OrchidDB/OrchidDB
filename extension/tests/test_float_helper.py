"""Typed compatibility function and caller-catalog selection, on the actual extension."""
import unittest

from test_extension import connect


class FloatHelperTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        self.db.execute('CREATE TABLE float_input(id BIGINT, raw VARCHAR)')
        self.inputs = [None, '', 'foo', '2.9', '__1__2__', '\u00a01.5\u3000',
                       'NaN', 'Infinity', '1e999', '1e-999']
        self.expected = [None, None, None, 2.9, 12.0, 1.5, None, None, None, 0.0]
        self.db.executemany('INSERT INTO float_input VALUES (?, ?)', enumerate(self.inputs))
        self.db.execute('CREATE PROPERTY GRAPH float_graph VERTEX TABLES '
                        '(float_input KEY (id) LABEL Input PROPERTIES (id, raw))')

    def tearDown(self):
        self.db.close()

    def test_typed_batch_null_and_boundary_contract(self):
        rows = self.db.execute('SELECT __orchiddb_cypher_to_float_v1(raw) '
                               'FROM float_input ORDER BY id').fetchall()
        self.assertEqual([row[0] for row in rows], self.expected)
        signature = self.db.execute("SELECT parameter_types, return_type, stability, has_side_effects "
                                    "FROM duckdb_functions() WHERE function_name = '__orchiddb_cypher_to_float_v1'").fetchone()
        self.assertEqual(signature, (['VARCHAR'], 'DOUBLE', 'CONSISTENT', False))

    def test_mapped_compiler_selects_registered_contract(self):
        rows = self.db.execute('CYPHER float_graph MATCH (n:Input) '
                               'RETURN toFloat(n.raw) AS x ORDER BY n.id').fetchall()
        self.assertEqual([row[0] for row in rows], self.expected)
        explain = ("EXPLAIN (FORMAT JSON) SELECT * FROM orchid_cypher('float_graph', "
                   "'MATCH (n:Input) WHERE toFloat(n.raw) > 5 RETURN n.id')")
        plan = self.db.execute(explain).fetchone()[1]
        self.assertIn('__orchiddb_cypher_to_float_v1', plan)
        self.assertNotIn('isnan', plan.lower())
        self.assertEqual(plan.count('__orchiddb_cypher_to_float_v1'), 1)
        conditional = ("EXPLAIN (FORMAT JSON) SELECT * FROM orchid_cypher('float_graph', "
                       "'MATCH (n:Input) WHERE (CASE WHEN n.id = 1 THEN 0.0 "
                       "ELSE toFloat(n.raw) END) > 5 RETURN n.id')")
        plan = self.db.execute(conditional).fetchone()[1]
        self.assertNotIn('isnan', plan.lower())
        self.assertEqual(plan.count('__orchiddb_cypher_to_float_v1'), 1)
        # An arbitrary caller macro must never count as a compatible helper.
        self.db.execute('CREATE SCHEMA shadow')
        self.db.execute('CREATE MACRO shadow.__orchiddb_cypher_to_float_v1(x) AS 9999.0')
        self.db.execute("SET search_path = 'shadow,main'")
        rows = self.db.execute('CYPHER main.float_graph MATCH (n:Input) '
                               'RETURN toFloat(n.raw) AS x ORDER BY n.id').fetchall()
        self.assertEqual([row[0] for row in rows], self.expected)
        plan = self.db.execute(explain).fetchone()[1]
        self.assertNotIn('__orchiddb_cypher_to_float_v1', plan)
        self.assertNotIn('isnan', plan.lower())
        rows = self.db.execute('CYPHER main.float_graph MATCH (n:Input) '
                               'WHERE toFloat(n.raw) > 5 RETURN n.id ORDER BY n.id').fetchall()
        self.assertEqual(rows, [(4,)])
