import unittest
from types import SimpleNamespace
from query_cost import collect, summarize

class QueryCostTests(unittest.TestCase):
    def entry(self, phase='query', work=10):
        return {'query':'RETURN 1','phase':phase,'cost':{'metric_version':1,'work_units':work,'request_elapsed_micros':20}}
    def test_fixture_work_is_not_ranked_as_test_query_work(self):
        result=collect(SimpleNamespace(query_costs=[self.entry('fixture',10000),self.entry()]),{})
        self.assertEqual(result['work_units'],10)
        self.assertEqual(len(result['queries']),2)
    def test_errors_are_timed_without_inventing_zero_work(self):
        result=collect(SimpleNamespace(query_costs=[self.entry(work=None)]),{})
        self.assertEqual(result['coverage'],'elapsed_only')
        self.assertIsNone(result['work_units'])
        self.assertEqual(result['request_elapsed_micros'],20)
    def test_java_transport_and_comparison(self):
        c=collect(None,{'query_transports':[{'query':'RETURN 1','query_cost':self.entry()['cost']}]})
        row={'id':'case','case_sha256':'same','query_cost':c}
        report=summarize([row],5,{'results':[row]})
        self.assertEqual(len(report['expensive_queries']),1)
        self.assertEqual(report['baseline_comparisons'][0]['delta'],0)
        changed={**row,'case_sha256':'changed'}
        self.assertEqual(summarize([changed],5,{'results':[row]})['baseline_comparisons'],[])
    def test_no_execution_is_distinct_from_zero_measured_work(self):
        self.assertEqual(collect(None,{})['coverage'],'not_executed')
        self.assertEqual(collect(SimpleNamespace(query_costs=[self.entry(work=0)]),{})['work_units'],0)
if __name__=='__main__':unittest.main()
