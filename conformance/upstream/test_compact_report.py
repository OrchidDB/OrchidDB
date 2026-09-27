import copy
import unittest
from compact_report import compact


class CompactReportTests(unittest.TestCase):
    def test_comparisons_are_bounded_without_losing_aggregate_totals(self):
        comparisons = [{'id': str(i), 'before': 200, 'after': i, 'delta': i-200}
                       for i in range(401)]
        raw = {'query_cost_summary': {'baseline_comparisons': comparisons},
               'results': [{'id': 'one', 'status': 'pass', 'query_transports': ['large'],
                            'query_cost': {'work_units': 7, 'queries': ['large']}}]}
        original = copy.deepcopy(raw)
        published = compact(raw)
        summary = published['query_cost_summary']
        self.assertEqual(len(summary['baseline_comparisons']), 100)
        totals = summary['baseline_comparison_summary']
        self.assertEqual((totals['case_count'], totals['improved'], totals['regressed'], totals['unchanged']),
                         (401, 200, 200, 1))
        self.assertEqual(totals['after_work_units'], sum(range(401)))
        self.assertNotIn('query_transports', published['results'][0])
        self.assertEqual(published['results'][0]['query_cost'], {'work_units': 7})
        self.assertEqual(raw, original)
        self.assertEqual(compact(published), published)


if __name__ == '__main__':
    unittest.main()
