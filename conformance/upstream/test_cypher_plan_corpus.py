import json
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch
from cypher_plan_corpus import corpus, inspect, plan_metrics

class PlanCorpusTests(unittest.TestCase):
    def test_generated_requests_are_unique_and_cover_fixtures(self):
        cases = list(corpus())
        self.assertGreaterEqual(len(cases), 14000)
        self.assertEqual(len(cases), len({c['id'] for c in cases}))
        self.assertEqual({c['fixture'] for c in cases}, {'empty', 'nullable', 'cycle'})
        self.assertEqual(len({c['family'] for c in cases}), 8)
        self.assertTrue(all(c['request']['scenario_queries'][-1] == c['request']['query'] for c in cases))

    def test_estimates_include_nested_join_children(self):
        plan = [{'plan': [{'name':' HASH_JOIN ', 'extra_info':{'Estimated Cardinality':'12'},
                 'children':[{'name':'SEQ_SCAN', 'extra_info':{'Estimated Cardinality':'200'}, 'children':[]}]}]}]
        self.assertEqual(plan_metrics(plan), {'operators':{'HASH_JOIN':1, 'SEQ_SCAN':1}, 'max_estimated_rows':200})

    def test_plan_only_flags_and_explain_failure_are_preserved(self):
        case = next(corpus())
        response = dict(sql='SELECT missing', fully_lowered=True,
                        sql_plans=[{'error':'Binder error: missing'}], residual_operators=0)
        with patch('cypher_plan_corpus.subprocess.run', return_value=subprocess.CompletedProcess([],0,json.dumps(response),'')) as run:
            row = inspect(case, Path('/probe'), 'nofold', 2)
        self.assertEqual(run.call_args.args[0], ['/probe','--conformance-sql','--explain-sql','--no-constant-folding'])
        self.assertEqual(json.loads(run.call_args.kwargs['input']), case['request'])
        self.assertIn('explain_error', row['findings'])
        self.assertNotIn('preparation_error', row['findings'])

    def test_timeout_is_not_a_pass(self):
        with patch('cypher_plan_corpus.subprocess.run', side_effect=subprocess.TimeoutExpired('/probe', 1)):
            row = inspect(next(corpus()), Path('/probe'), 'normal', 1)
        self.assertTrue(row['result']['timeout'])
        self.assertIn('preparation_error', row['findings'])
        self.assertIn('no_sql', row['findings'])

if __name__ == '__main__': unittest.main()
