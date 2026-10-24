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

    def test_sql_resumes_after_managed_mutation(self):
        self.db.execute('CREATE PROPERTY GRAPH managed_policy')
        self.db.execute('CYPHER managed_policy CREATE (:P {x:1})').fetchall()
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        self.db.execute('CYPHER managed_policy MATCH (p:P) SET p.x=2 RETURN p.x').fetchall()
        result = self.db.execute("SELECT __orchiddb_value_json(x) FROM orchid_cypher('managed_policy','MATCH (p:P) RETURN p.x AS x')").fetchone()
        import json
        self.assertEqual(json.loads(result[0])['value'], 2)

    def test_managed_projection_preserves_properties(self):
        self.db.execute('CREATE PROPERTY GRAPH temporal_policy')
        self.db.execute('CYPHER temporal_policy CREATE (:P {x:42})').fetchall()
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        result = self.db.execute("SELECT __orchiddb_value_json(n) FROM orchid_cypher('temporal_policy', 'MATCH (p:P) WITH p WHERE p.x=42 RETURN count(*) AS n')").fetchone()
        import json
        self.assertEqual(json.loads(result[0])['value'], 1)

    def test_limit_zero_preserves_effects_and_empty_results(self):
        self.db.execute('CREATE PROPERTY GRAPH empty_policy')
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        self.assertEqual(self.db.execute('CYPHER empty_policy CREATE (p:P {x:42}) RETURN p LIMIT 0').fetchall(), [])
        self.assertEqual(self.db.execute('CYPHER empty_policy MATCH (p:P) RETURN p LIMIT 0').fetchall(), [])
        result = self.db.execute("SELECT __orchiddb_value_json(n) FROM orchid_cypher('empty_policy', 'MATCH (p:P) RETURN count(*) AS n')").fetchone()
        import json
        self.assertEqual(json.loads(result[0])['value'], 1)

    def test_strict_scalar_types_survive_sql_boundary(self):
        self.db.execute('CREATE PROPERTY GRAPH value_policy')
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        import json
        for query, expected in [("RETURN 1.0 AS x", 'double'), ("UNWIND [localtime('12:30')] AS t WITH t ORDER BY t RETURN t AS x", 'cypher_temporal')]:
            value = self.db.execute("SELECT __orchiddb_value_json(x) FROM orchid_cypher('value_policy', ?)", [query]).fetchone()[0]
            self.assertEqual(json.loads(value)['type'], expected)

    def test_strict_variable_path_and_distinct_map(self):
        self.db.execute('CREATE PROPERTY GRAPH path_policy')
        self.db.execute('CYPHER path_policy CREATE (:P {x:1})-[:T]->(:P {x:2})').fetchall()
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        import json
        result = self.db.execute("SELECT __orchiddb_value_json(n) FROM orchid_cypher('path_policy', 'MATCH ()-[r*0..1]-() RETURN count(*) AS n')").fetchone()
        self.assertEqual(json.loads(result[0])['value'], 4)
        result = self.db.execute("SELECT __orchiddb_value_json(n) FROM orchid_cypher('path_policy', 'MATCH (p:P) RETURN count(DISTINCT {x:p.x}) AS n')").fetchone()
        self.assertEqual(json.loads(result[0])['value'], 2)

    def test_zero_column_effect_handoff_preserves_multiplicity(self):
        self.db.execute('CREATE PROPERTY GRAPH cardinality_policy')
        self.db.execute('CYPHER cardinality_policy CREATE (), ()').fetchall()
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        self.db.execute('CYPHER cardinality_policy MATCH () CREATE () WITH * MATCH () CREATE ()').fetchall()
        import json
        result = self.db.execute("SELECT __orchiddb_value_json(n) FROM orchid_cypher('cardinality_policy', 'MATCH () RETURN count(*) AS n')").fetchone()
        self.assertEqual(json.loads(result[0])['value'], 12)

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
