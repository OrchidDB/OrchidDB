import copy
import hashlib
import json
import unittest
from parallel import merge

class ParallelTests(unittest.TestCase):
    def setUp(self):
        self.cases = [{'id': str(i), 'suite': 'opencypher', 'steps': []} for i in range(4)]
        self.reports = []
        for index in range(2):
            rows = [{'id': c['id'], 'status': 'pass' if c['id'] != '3' else 'fail',
                     'case_sha256': hashlib.sha256(json.dumps(c, sort_keys=True).encode()).hexdigest(),
                     'query_cost': {'queries': [{'sql': c['id']}]}} for c in self.cases[index::2]]
            self.reports.append({'engine': 'orchiddb', 'suite': 'opencypher', 'source': {'revision': 'pinned'},
                'started_at': f'2026-01-01T00:00:0{index}', 'finished_at': f'2026-01-01T00:01:0{index}',
                'build': {'binary_sha256': 'same'}, 'results': rows})

    def test_preserves_order_failures_and_execution_evidence(self):
        result = merge(self.reports, self.cases, 4, [])
        self.assertEqual([r['id'] for r in result['results']], ['0','1','2','3'])
        self.assertEqual(result['results'][-1]['status'], 'fail')
        self.assertEqual(result['results'][-1]['query_cost'], self.reports[1]['results'][-1]['query_cost'])
        self.assertFalse(result['coverage']['filtered'])
        self.assertTrue(merge(self.reports, self.cases, 10, [])['coverage']['filtered'])

    def test_rejects_missing_duplicate_or_unexpected_cases(self):
        for replacement in [[], [self.reports[0]['results'][0]], [{'id': 'other'}]]:
            reports = copy.deepcopy(self.reports)
            reports[0]['results'] = replacement
            with self.assertRaisesRegex(ValueError, 'exactly once'):
                merge(reports, self.cases, 4, [])

    def test_rejects_changed_binary_catalog_and_source(self):
        for change in ['binary', 'catalog', 'source']:
            reports = copy.deepcopy(self.reports)
            if change == 'binary': reports[1]['build']['binary_sha256'] = 'changed'
            elif change == 'catalog': reports[1]['results'][0]['case_sha256'] = 'changed'
            else: reports[1]['source']['revision'] = 'changed'
            with self.assertRaises(ValueError): merge(reports, self.cases, 4, [])


class WorkerCliTests(unittest.TestCase):
    def test_rejects_unsafe_or_empty_selections_without_starting_workers(self):
        import subprocess, sys, tempfile
        from pathlib import Path
        script = str(Path(__file__).with_name('run.py'))
        with tempfile.TemporaryDirectory() as directory:
            empty = Path(directory)/'empty.json'
            empty.write_text('[]')
            for arguments in [
                ['--suite','tinkerpop','--jobs','4'],
                ['--suite','opencypher','--jobs','0'],
                ['--suite','opencypher','--jobs','4','--resume'],
                ['--suite','opencypher','--case-file',str(empty)],
            ]:
                result = subprocess.run([sys.executable,script,'--engine','orchiddb',*arguments],capture_output=True,text=True)
                self.assertEqual(result.returncode,2,result.stderr)

if __name__ == '__main__': unittest.main()
