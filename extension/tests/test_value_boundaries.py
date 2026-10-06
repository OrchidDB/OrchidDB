"""Language productivity, equality and ordering at DuckDB value boundaries."""
import json
import unittest

from test_extension import connect, fixture


class ValueBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        fixture(self.db)

    def tearDown(self):
        self.db.close()

    def gremlin(self, query):
        rows = self.db.execute(
            "SELECT __orchiddb_value_json(q.current) FROM orchid_gremlin('social', ?) q", [query]
        ).fetchall()
        return [json.loads(row[0]) for row in rows]

    def cypher(self, query):
        rows = self.db.execute(
            "SELECT __orchiddb_value_json(q) FROM orchid_cypher('social', ?) q", [query]
        ).fetchall()
        return [dict((k['value'], v) for k, v in json.loads(row[0])['value']) for row in rows]

    def test_map_lookup_null_is_productive_and_case_sensitive(self):
        self.assertEqual(self.gremlin('g.inject(["Name":"Ada"]).values("name")'), [])
        self.assertEqual(self.gremlin('g.inject(["Name":"Ada"]).as("m").select("m").by("name")'),
                         [dict(type='null', value=None)])
        self.assertEqual(self.gremlin('g.inject(["name":null]).values("name")'),
                         [dict(type='null', value=None)])
        self.assertEqual(self.gremlin('g.V().values("absent")'), [])

    def test_current_projection_retains_named_labels(self):
        rows = self.gremlin('g.V().as("person").values("name").select("person").values("age")')
        self.assertEqual(sorted(value['value'] for value in rows), [30, 40])

    def test_order_compares_native_values_instead_of_their_encoding(self):
        rows = self.gremlin('g.inject(10,2.0d,-1,100).order()')
        self.assertEqual([float(value['value']) for value in rows], [-1, 2, 10, 100])

    def test_distinct_and_grouping_share_cypher_numeric_equivalence(self):
        rows = self.cypher('UNWIND [1,1.0,2] AS x RETURN DISTINCT x ORDER BY x')
        self.assertEqual([float(row['x']['value']) for row in rows], [1, 2])
        rows = self.cypher('UNWIND [1,1.0,2] AS x RETURN x,count(*) AS n ORDER BY x')
        self.assertEqual([row['n']['value'] for row in rows], [2, 1])


if __name__ == '__main__':
    unittest.main(verbosity=2)
