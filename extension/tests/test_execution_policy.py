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

    def test_strict_repeat_preserves_path_and_loop_counter(self):
        self.db.execute('CREATE PROPERTY GRAPH repeat_policy')
        self.db.execute("CYPHER repeat_policy CREATE (:P {name:'a'})-[:R]->(:P {name:'b'})-[:R]->(:P {name:'c'})").fetchall()
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        import json
        query = 'g.V().has("name","a").repeat(__.out()).until(__.loops().is(2)).path().by("name")'
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('repeat_policy', ?)", [query]).fetchall()
        self.assertEqual(len(rows), 1)
        value = json.loads(rows[0][0])
        self.assertEqual([item['value'] for item in value['value']], ['a', 'b', 'c'])

    def test_strict_repeat_keeps_bulk_compact_before_dedup(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        # Parallel edges produce 4**20 logical traversers, represented by one
        # compact traverser. Dedup must reset bulk without expanding it.
        query = 'g.V(1).repeat(__.out().in().barrier()).times(20).dedup().values("name")'
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('social', ?)", [query]).fetchall()
        import json
        self.assertEqual([json.loads(row[0])['value'] for row in rows], ['Alice'])

    def test_strict_nested_repeat_preserves_named_outer_counter(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        query = 'g.V(1).repeat("outer", __.out().repeat("inner", __.out().filter(__.loops("outer").is(0))).emit()).emit().values("name")'
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('social', ?)", [query]).fetchall()
        import json
        self.assertEqual([json.loads(row[0])['value'] for row in rows], ['Cara', 'Cara'])

    def test_strict_branch_routes_predicates(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('social', 'g.V().branch(__.label()).option(\"Person\", __.values(\"name\"))')").fetchall()
        import json
        self.assertCountEqual([json.loads(row[0])['value'] for row in rows], ['Alice', 'Bob', 'Cara'])

    def test_strict_gremlin_mutation_seed(self):
        self.db.execute('CREATE PROPERTY GRAPH mutation_policy')
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        self.db.execute('GREMLIN mutation_policy g.addV("person").property("name","Ada")').fetchall()
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('mutation_policy', 'g.V().values(\"name\")')").fetchall()
        import json
        self.assertEqual([json.loads(row[0])['value'] for row in rows], ['Ada'])

    def test_strict_choose_fold_keeps_correlation(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        import json
        query = 'g.inject(1).choose(__.is(1), __.constant(10).fold(), __.fold())'
        value = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('social', ?)", [query]).fetchone()[0]
        self.assertEqual(json.loads(value), {'type': 'list', 'value': [{'type': 'int', 'value': 10}]})

    def test_strict_match_preserves_absent_labels(self):
        self.db.execute('CREATE PROPERTY GRAPH match_policy')
        self.db.execute("CYPHER match_policy CREATE (:Person {name:'Ada'})-[:CREATED]->(:Project {name:'Orchid'})").fetchall()
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        query = 'g.V().local(__.match(__.as("project").in("CREATED").as("person"), __.as("person").values("name").as("name"))).select("name")'
        import json
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('match_policy', ?)", [query]).fetchall()
        self.assertEqual([json.loads(row[0])['value'] for row in rows], ['Ada'])

    def test_strict_union_preserves_branch_paths(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        query = 'g.union(__.V(1).out().barrier(), __.V(3).barrier()).path().by("name")'
        import json
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('social', ?)", [query]).fetchall()
        paths = [[item['value'] for item in json.loads(row[0])['value']] for row in rows]
        self.assertCountEqual(paths, [['Alice', 'Bob'], ['Alice', 'Bob'], ['Cara']])

    def test_strict_quantifiers_require_lists(self):
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        import json
        for operation in ('all', 'any'):
            with self.subTest(operation=operation):
                for query in (f'g.inject(40).{operation}(P.gt(32))', f'g.inject(null).{operation}(P.gt(32))'):
                    self.assertEqual(self.db.execute("SELECT * FROM orchid_gremlin('social', ?)", [query]).fetchall(), [])
                query = f'g.inject([40,50]).{operation}(P.gt(32))'
                rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('social', ?)", [query]).fetchall()
                self.assertEqual(len(rows), 1)
                self.assertEqual([item['value'] for item in json.loads(rows[0][0])['value']], [40, 50])

    def test_strict_union_executes_effectful_inputs_in_order(self):
        self.db.execute('CREATE PROPERTY GRAPH union_effects')
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        query = 'g.union(__.addV("Marker").barrier().constant("first"), __.V().hasLabel("Marker").barrier().count())'
        import json
        rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('union_effects', ?)", [query]).fetchall()
        self.assertEqual([json.loads(row[0])['value'] for row in rows], ['first', 1])

    def test_transport_names_are_valid_cypher_result_aliases(self):
        self.db.execute('CREATE PROPERTY GRAPH alias_policy')
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        import json
        for prefix in ('', 'CREATE () '):
            query = prefix + 'RETURN [] AS __orchid_absent_state, {} AS __orchid_extra_bindings, 17 AS __orchid_traverser_bulk'
            row = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_cypher('alias_policy', ?)", [query]).fetchone()
            self.assertEqual([json.loads(value)['type'] for value in row], ['list', 'map', 'long'])
            self.assertEqual(json.loads(row[2])['value'], 17)

    def test_shared_sql_input_keeps_partition_traversal_state_serial(self):
        self.db.execute('CREATE PROPERTY GRAPH partition_policy')
        self.db.execute("""CYPHER partition_policy
            CREATE (a:Person {name:'a', _partition:'a'})
                -[:R {_partition:'a', weight:1}]->
                (b:Person {name:'b', _partition:'b'})
                -[:R {_partition:'b', weight:2}]->(a)""").fetchall()
        self.db.execute("SET orchid_lowering_policy='sql_required'")
        self.db.execute('SET threads=4')
        query = 'g.withStrategies(new PartitionStrategy(partitionKey:"_partition", writePartition:"a", readPartitions:["a","b"])).V().bothE().dedup().values("weight")'
        import json
        for _ in range(3):
            rows = self.db.execute("SELECT __orchiddb_value_json(COLUMNS(*)) FROM orchid_gremlin('partition_policy', ?)", [query]).fetchall()
            self.assertCountEqual([json.loads(row[0])['value'] for row in rows], [1, 2])

    def test_invalid_policy_fails_closed(self):
        self.db.execute("SET orchid_lowering_policy='typo'")
        with self.assertRaisesRegex(Exception, 'Invalid orchid_lowering_policy'):
            self.db.execute('CYPHER social RETURN 1').fetchall()
