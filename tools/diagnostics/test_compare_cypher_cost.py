import unittest
from compare_cypher_cost import compare


def case(work=112, version=2):
    entry = {'query': 'RETURN 1', 'phase': 'query', 'cost': {
        'work_units': work, 'orchid_work_units': 102, 'engine_work_units': work-102,
        'measurements': {'sql_executions': 1, 'native_source_queries': 0,
                         'engine': {'peak_memory_bytes': 4096, 'max_plan_depth': 3}}}}
    return {'id': 'case', 'status': 'pass', 'case_sha256': 'same',
            'query_cost': {'metric_version': version, 'attribution_version': 2, 'queries': [entry]}}


class CostComparisonTests(unittest.TestCase):
    def test_components_and_peaks(self):
        a, b = case(), case(107)
        c = case(105)
        c['id'] = 'second'
        report = compare({'results': [a, c]}, {'results': [b, c]})
        self.assertEqual(report['improved'], 1)
        self.assertEqual(report['totals']['after']['peak_memory_bytes'], 4096)
        self.assertEqual(report['totals']['after']['total_work'], 212)
        self.assertEqual(report['totals']['after']['orchid_work'], 204)

    def test_incompatible_measurements_are_excluded(self):
        for field, value in [('case_sha256', 'changed'), ('status', 'fail')]:
            old, new = case(), case()
            new[field] = value
            self.assertEqual(compare({'results': [old]}, {'results': [new]})['compared'], 0)
        self.assertEqual(compare({'results': [case(version=1)]}, {'results': [case()]})['compared'], 0)

    def test_query_changes_are_excluded(self):
        old, new = case(), case()
        new['query_cost']['queries'][0]['query'] = 'RETURN 2'
        self.assertEqual(compare({'results': [old]}, {'results': [new]})['compared'], 0)


if __name__ == '__main__':
    unittest.main()
