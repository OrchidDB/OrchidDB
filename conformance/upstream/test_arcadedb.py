"""Local integration checks for ArcadeDB transports and upstream assertions.

Run after build-arcadedb.sh with CONFORMANCE_ARCADEDB_TESTS=1 and Java 21.
"""
import json
import os
import unittest
from pathlib import Path
from cypher import Cypher
from run import Gremlin, ROOT


@unittest.skipUnless(os.environ.get('CONFORMANCE_ARCADEDB_TESTS') == '1',
                     'requires locally built ArcadeDB adapters and Java 21')
class ArcadeDBIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.adapter = Cypher('arcadedb')
        self.adapter.rust.send({'op': 'reset'})

    def tearDown(self):
        self.adapter.close()

    def query(self, text, params=None):
        result = self.adapter.query(text, params)
        self.assertNotIn('error', result, result)
        self.assertNotIn('adapter_error', result, result)
        return result

    def test_nested_parameters_preserve_null_boolean_and_numeric_types(self):
        result = self.query('RETURN $payload AS value', {'payload': [None, True, 7, 2.5, {'name': 'orchid'}]})
        self.assertEqual(result['columns'], ['value'])
        self.assertEqual(result['rows'], [[[None, True, 7, 2.5, {'name': 'orchid'}]]])

    def test_empty_projection_uses_provider_ast_names(self):
        result = self.query('MATCH (n:Person) RETURN n.name AS name, 1 AS count')
        self.assertEqual(result['rows'], [])
        self.assertEqual(result['columns'], ['name', 'count'])
        self.assertNotIn('adapter_error', result)

    def test_node_relationship_and_path_transport(self):
        self.query("CREATE (:Person {name:'a'})-[:KNOWS {weight:2}]->(:Person {name:'b'})")
        result = self.query('MATCH p=(a)-[r]->(b) RETURN a,r,p')
        node, edge, path = result['rows'][0]
        self.assertEqual(node, {'$node': {'labels': ['Person'], 'properties': {'name': 'a'}}})
        self.assertEqual(edge, {'$relationship': {'type': 'KNOWS', 'properties': {'weight': 2}}})
        self.assertEqual(path['$path']['start'], node)
        self.assertEqual(path['$path']['segments'][0]['direction'], 'forward')
        self.assertEqual(path['$path']['segments'][0]['relationship'], edge)

    def test_reset_removes_records_and_close_removes_fixture_directory(self):
        self.query('CREATE (:Person)')
        self.adapter.rust.send({'op': 'reset'})
        self.assertEqual(self.query('MATCH (n) RETURN count(n) AS count')['rows'], [[0]])
        directory = Path(self.adapter.rust.fixture_directory.name)
        self.adapter.close()
        self.assertFalse(directory.exists())

    def test_native_runtime_phase_is_not_changed_to_match_error_assertion(self):
        result = self.adapter.run({'steps': [
            {'text': 'an empty graph'},
            {'text': 'executing query:', 'doc': 'CALL missing.procedure()'},
            {'text': 'a ProcedureError should be raised at compile time: ProcedureNotFound'},
        ]})
        self.assertEqual(result['status'], 'fail')
        self.assertEqual(result['actual']['classification']['phase'], 'runtime')
        self.assertIn('exception_class', result['actual'])

    def test_original_cypher_assertions_include_side_effects(self):
        cases = json.loads((ROOT / 'upstream/catalog.json').read_text())['cases']
        case = next(c for c in cases if c['id'].endswith('clauses/create/Create1.feature:33'))
        result = self.adapter.run(case)
        self.assertEqual(result['status'], 'pass', result)
        self.assertTrue(any('side effects' in a for a in result['assertions']))

    def test_empty_star_projection_tracks_with_scope(self):
        result = self.query('MATCH (a:Start) WITH a MATCH (a)-->(b) RETURN *')
        self.assertEqual(set(result['columns']), {'a', 'b'})
        self.assertEqual(result['rows'], [])
        result = self.query('MATCH (n) WITH n.age AS age WITH age, age+1 AS next RETURN *')
        self.assertEqual(set(result['columns']), {'age', 'next'})

    def test_temporal_values_retain_nanoseconds_offsets_and_duration(self):
        result = self.query("RETURN date('2020-01-02') AS d, time('12:34:56.123456700+01:02:03') AS t, duration('P1MT0.5S') AS duration")
        values = dict(zip(result['columns'], result['rows'][0]))
        self.assertEqual(values, {'d': '2020-01-02', 't': '12:34:56.1234567+01:02:03', 'duration': 'P1MT0.5S'})

    def test_original_given_procedure_registration(self):
        cases = json.loads((ROOT / 'upstream/catalog.json').read_text())['cases']
        case = next(c for c in cases if c['id'].endswith('clauses/call/Call1.feature:33'))
        result = self.adapter.run(case)
        self.assertEqual(result['status'], 'pass', result)
        self.adapter.rust.send({'op': 'reset'})
        self.assertIn('error', self.adapter.query('CALL test.my.proc()'))

    def test_original_gremlin_count_assertions_and_cleanup(self):
        cases = json.loads((ROOT / 'upstream/catalog.json').read_text())['cases']
        case = next(c for c in cases if c['id'].endswith('map/Count.feature:21'))
        adapter = Gremlin('arcadedb')
        directory = Path(adapter.fixture_directory.name)
        try:
            result = adapter.run(case)
            self.assertEqual(result['status'], 'pass', result)
            self.assertIn('unmodified', result['assertion_engine'])
        finally:
            adapter.close()
        self.assertFalse(directory.exists())
