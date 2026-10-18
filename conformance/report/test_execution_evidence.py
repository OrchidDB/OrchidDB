import csv
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import conformance_page
from build import build
from execution_evidence import badge, lowering


def query(state, phase='query'):
    return {'query': 'RETURN 1 AS answer', 'phase': phase, 'cost': {'execution_plan': {
        'format_version': 1, 'capture': 'executed', 'lowering': state,
        'fully_lowered_to_sql': state == 'full_sql',
        'logical_plan': 'Projection\n  EmptyRelation', 'physical_plan': 'DuckDbExec',
        'sql_queries': ['SELECT 1 AS "answer"\n-- exact emitted SQL'],
        'native_source_queries': [], 'residual_operators': 0 if state == 'full_sql' else 1,
    }}}


class ExecutionEvidenceTests(unittest.TestCase):
    def test_run_metadata_is_readable_and_escaped(self):
        html = conformance_page.run_summary({'finished_at': '2026-10-18',
            'build': {'version': '<local>', 'revision': 'abc123'},
            'coverage': {'recorded_cases': 12, 'catalog_cases': 100, 'filtered': True}})
        self.assertIn('<dt>Recorded scenarios</dt><dd>12</dd>', html)
        self.assertIn('<dt>Run selection</dt><dd>Filtered</dd>', html)
        self.assertIn('&lt;local&gt;', html)
        self.assertNotIn('<pre>', html)

    def test_query_lowering_is_separate_from_conformance_outcome_and_fixture(self):
        row = {'status': 'fail', 'query_cost': {'queries': [query('runtime', 'fixture'), query('full_sql')]}}
        self.assertEqual(lowering(row), 'full_sql')
        row['query_cost']['queries'].append(query('runtime', 'observation'))
        self.assertEqual(lowering(row), 'full_sql')
        row['query_cost']['queries'].append(query('mixed', 'control'))
        self.assertEqual(lowering(row), 'mixed')

    def test_missing_or_partial_evidence_never_claims_sql_lowering(self):
        for row in ({'status': 'pass'}, {'status': 'fail'}, {'query_cost': {'queries': [query('full_sql'), {'phase': 'query'}]}},
                    {'query_cost': {'queries': [query('full_sql', 'fixture')]}}):
            with self.subTest(row=row):
                self.assertEqual(lowering(row), 'unknown')
        item = query('full_sql')
        item['cost']['execution_plan']['fully_lowered_to_sql'] = False
        self.assertEqual(lowering({'query_cost': {'queries': [item]}}), 'unknown')

    def test_runtime_and_mixed_are_distinct(self):
        self.assertEqual(lowering({'query_cost': {'queries': [query('runtime')]}}), 'runtime')
        self.assertEqual(lowering({'query_cost': {'queries': [query('runtime'), query('full_sql')]}}), 'mixed')

    def test_badges_have_both_icons_and_text(self):
        for state, icon, label in [('full_sql', '✓', 'Fully lowered to SQL'),
                                   ('mixed', '◐', 'SQL + OrchidDB runtime'),
                                   ('runtime', '○', 'OrchidDB runtime'),
                                   ('unknown', '?', 'Plan not captured')]:
            self.assertIn(icon, badge(state))
            self.assertIn(label, badge(state))

    def test_published_page_and_downloads_keep_the_same_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'upstream').mkdir()
            (root / 'data').mkdir()
            runs = root / 'runs'; runs.mkdir()
            cases = [{'id': f'case-{i}', 'suite': 'opencypher', 'feature': 'SQL lowering',
                      'name': f'Case {i} <script>', 'source': 'https://example.com/pinned', 'steps': []}
                     for i in range(4)]
            (root / 'upstream/catalog.json').write_text(json.dumps({'cases': cases,
                'sources': {s: {'version': 'pinned'} for s in conformance_page.SUITES}}))
            (root / 'upstream/sources.json').write_text('{}')
            (root / 'data/capabilities.json').write_text('[]')
            rows = [{'id': c['id'], 'status': 'pass',
                     **({'query_cost': {'queries': [query(state)]}} if state != 'unknown' else {})}
                    for c, state in zip(cases, ['full_sql', 'mixed', 'runtime', 'unknown'])]
            (runs / 'orchiddb-opencypher.json').write_text(json.dumps({
                'results': rows, 'finished_at': '2026-10-18', 'build': {}, 'coverage': {'filtered': True}}))
            with patch.object(conformance_page, 'ROOT', root):
                page = build(root / 'output', runs)
            html = page.read_text()
            self.assertIn('Partial run evidence', html)
            self.assertIn('&lt;script&gt;', html)
            for state in ('full_sql', 'mixed', 'runtime', 'unknown'):
                self.assertIn(f'data-lowering="{state}"', html)
            evidence = json.loads(next((page.parent / 'downloads/conformance/features').glob('*.json')).read_text())
            self.assertEqual(evidence['results']['orchiddb']['case-0']['query_cost'], rows[0]['query_cost'])
            with (page.parent / 'downloads/conformance/upstream-comparison.csv').open() as stream:
                csv_rows = list(csv.DictReader(stream))
            self.assertEqual([r['orchiddb_sql_lowering'] for r in csv_rows if r['product'] == 'orchiddb'],
                             ['full_sql', 'mixed', 'runtime', 'unknown'])


if __name__ == '__main__':
    unittest.main()
