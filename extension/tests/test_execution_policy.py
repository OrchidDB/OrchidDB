"""Execution placement and aggregate semantics are independent host settings."""
import unittest
from test_extension import connect, fixture


class ExecutionPolicyTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        fixture(self.db)

    def tearDown(self):
        self.db.close()

    def test_sql_required_mapped_join_and_aggregate(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        self.assertEqual(self.db.execute("""CYPHER social
            MATCH (a:Person)-[e:FOLLOWS]->(b)
            RETURN a.name, count(e) ORDER BY a.name""").fetchall(), [('Alice', 2), ('Bob', 1)])

    def test_aggregate_semantics_are_independent(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        query = "CYPHER social MATCH (p:Person) WHERE p.age < 0 RETURN sum(p.age)"
        self.assertEqual(self.db.execute(query).fetchone(), (0,))
        self.db.execute("SET orchid_cypher_aggregates='sql_native'")
        self.assertEqual(self.db.execute(query).fetchone(), (None,))

    def test_strict_rejects_managed_relational_fallback(self):
        self.db.execute('CREATE PROPERTY GRAPH managed_policy')
        self.db.execute('CYPHER managed_policy CREATE (:P {x:1})').fetchall()
        before = self.db.execute('CYPHER managed_policy MATCH (p:P) RETURN p.x').fetchall()
        self.assertEqual(len(before), 1)
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        with self.assertRaisesRegex(Exception, 'SQL-required'):
            self.db.execute('CYPHER managed_policy MATCH (p:P) SET p.x=2 RETURN p.x').fetchall()
        self.db.execute("SET orchid_lowering_policy='hybrid'")
        self.assertEqual(self.db.execute('CYPHER managed_policy MATCH (p:P) RETURN p.x').fetchall(), before)

    def test_prepared_query_observes_changed_aggregate_policy(self):
        self.db.execute("PREPARE total AS SELECT * FROM orchid_cypher('social', 'MATCH (p:Person) WHERE p.age < 0 RETURN sum(p.age)')")
        self.assertEqual(self.db.execute("EXECUTE total").fetchone(), (0,))
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        self.db.execute("SET orchid_cypher_aggregates='sql_native'")
        self.assertEqual(self.db.execute("EXECUTE total").fetchone(), (None,))

    def test_invalid_policy_fails_closed(self):
        self.db.execute("SET orchid_lowering_policy='typo'")
        with self.assertRaisesRegex(Exception, 'Invalid orchid_lowering_policy'):
            self.db.execute('CYPHER social RETURN 1').fetchall()
