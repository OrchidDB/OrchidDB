"""Complete managed elements survive SQL and executable boundaries."""
import unittest
from test_extension import connect  # Adds the shared transport to sys.path.
from adapter import Extension


class ManagedValueTransportTests(unittest.TestCase):
    def test_multi_properties_meta_properties_and_public_identity(self):
        for policy in ('hybrid', 'sql_required'):
            with self.subTest(policy=policy):
                adapter = Extension()
                try:
                    adapter.fixture({'nodes': [{
                        'id': 'person-a', 'label': 'Person', 'property_records': [
                            {'id': 100, 'key': 'location', 'value': 'London', 'meta': {'since': 2000}},
                            {'id': 101, 'key': 'location', 'value': 'Paris', 'meta': {'since': 2010}},
                        ],
                    }], 'edges': []})
                    adapter.db.execute("SET orchid_lowering_policy='" + policy + "'")
                    cases = [
                        ('id()', ['person-a']),
                        ('properties("location").value()', ['London', 'Paris']),
                        ('properties("location").id()', [100, 101]),
                        ('properties("location").values("since")', [2000, 2010]),
                    ]
                    for suffix, expected in cases:
                        result = adapter.query({'op': 'gremlin', 'query': 'g.V().barrier().' + suffix})
                        self.assertCountEqual([row[0]['value'] for row in result['native_rows']], expected)
                    adapter.query({'op': 'gremlin', 'query': 'g.V().properties("location").property("since",2024)'})
                    result = adapter.query({'op': 'gremlin', 'query': 'g.V().properties("location").values("since")'})
                    self.assertEqual([row[0]['value'] for row in result['native_rows']], [2024, 2024])
                    adapter.query({'op': 'gremlin', 'query': 'g.V().properties("location").properties("since").drop()'})
                    result = adapter.query({'op': 'gremlin', 'query': 'g.V().properties("location").properties("since")'})
                    self.assertEqual(result['native_rows'], [])
                    adapter.query({'op': 'gremlin', 'query': 'g.V().properties("location").drop()'})
                    result = adapter.query({'op': 'gremlin', 'query': 'g.V().properties()'})
                    self.assertEqual(result['native_rows'], [])
                finally:
                    adapter.db.close()
