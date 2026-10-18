import copy
import unittest
from compact_report import compact


class CompactReportTests(unittest.TestCase):
    def test_execution_plans_and_exact_sql_survive_publication(self):
        plan = {'format_version': 1, 'capture': 'executed', 'lowering': 'full_sql',
                'fully_lowered_to_sql': True, 'logical_plan': 'Projection\n  TableScan',
                'physical_plan': 'DuckDbExec', 'sql_queries': ['SELECT "name"\nFROM "people"'],
                'schema_sql': ['CREATE TABLE people (node_id BIGINT PRIMARY KEY, name VARCHAR);'],
                'write_queries': [{'sql': 'UPDATE people SET name = input.name FROM input',
                                   'inputs': [['input', 2]]}],
                'native_source_queries': [], 'residual_operators': 0}
        query = {'query': 'MATCH (p:Person) RETURN p.name', 'phase': 'query',
                 'cost': {'execution_plan': plan, 'work_units': 10}, 'returned_rows': ['private']}
        report = {'results': [{'id': 'case', 'status': 'pass',
                              'query_cost': {'work_units': 10, 'queries': [query]},
                              'actual': ['private'], 'query_transports': ['private']}]}
        original = copy.deepcopy(report)
        published = compact(report)
        row = published['results'][0]
        self.assertEqual(row['query_cost']['queries'][0]['cost']['execution_plan'], plan)
        self.assertNotIn('returned_rows', row['query_cost']['queries'][0])
        self.assertNotIn('actual', row)
        self.assertNotIn('query_transports', row)
        self.assertEqual(report, original)
        self.assertEqual(compact(published), published)

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
