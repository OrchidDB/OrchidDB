import copy
import json
from pathlib import Path
import unittest
from java_assertions import counterpart, result_for, ROOT


class JavaAssertionTests(unittest.TestCase):
    def setUp(self):
        self.selection = json.loads((ROOT / 'adapters/jvm-provider-tests/placeholders.json').read_text())
        catalog = json.loads((ROOT / 'upstream/catalog.json').read_text())['cases']
        self.cases = [c for c in catalog if c['suite'] == 'tinkerpop' and counterpart(c, self.selection)]
        self.case = self.cases[0]
        self.mapping = counterpart(self.case, self.selection)
        self.report = {'upstream_revision': self.selection['upstream_revision'], 'run_complete': True,
            'inventory_only': False, 'upstream_assertions_modified': False, 'profile': 'jvm-provider',
            'process_exit_code': 0, 'cases': [{'id': self.mapping['java_class'] + '#' + self.mapping['java_method'],
                'source_sha256': self.mapping['source_sha256'], 'placeholder': self.mapping,
                'elapsed_ms': 12.5, 'status': 'pass'}]}

    def result(self, report):
        return result_for(self.case, self.mapping, report, self.selection['upstream_revision'])

    def test_exactly_fifteen_pinned_placeholders(self):
        self.assertEqual(len(self.cases), 15)
        self.assertEqual(len({c['id'] for c in self.cases}), 15)

    def test_mapping_rejects_changed_source_name_or_executable_steps(self):
        for key, value in [('source_sha256', 'changed'), ('name', 'other'), ('steps', [{'text': 'the modern graph'}])]:
            case = {**self.case, key: value}
            with self.assertRaises(ValueError): counterpart(case, self.selection)

    def test_preserves_failures_and_assumptions(self):
        for status in ('pass', 'fail', 'skipped'):
            self.report['cases'][0]['status'] = status
            result = self.result(self.report)
            self.assertEqual(result['status'], status)
            self.assertEqual(result['elapsed_ms'], 12.5)
            self.assertEqual(result['assertion_source']['kind'], 'java-counterpart')
            self.assertEqual(result['assertion_source']['gherkin_case_id'], self.case['id'])

    def test_rejects_incomplete_inventory_wrong_revision_or_modified_assertions(self):
        for key, value in [('run_complete', False), ('inventory_only', True), ('upstream_assertions_modified', True),
                           ('upstream_revision', 'other'), ('process_exit_code', 124), ('profile', 'jvm-graphcomputer')]:
            with self.subTest(key=key), self.assertRaises(ValueError): self.result({**self.report, key: value})

    def test_rejects_missing_duplicate_unfinished_or_stale_results(self):
        row = self.report['cases'][0]
        for rows in ([], [row, row], [{**row, 'status': 'running'}], [{**row, 'source_sha256': 'changed'}],
                     [{**row, 'placeholder': {}}], [{**row, 'elapsed_ms': -1}]):
            with self.subTest(rows=rows), self.assertRaises(ValueError): self.result({**self.report, 'cases': rows})

if __name__ == '__main__': unittest.main()
