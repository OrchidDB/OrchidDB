"""Local regression coverage for release gates and interrupted-build recovery."""
import datetime
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import pipeline as p


class PipelineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'state.json'
        self.state = {'version': '1.2.3', 'workspace': self.temp.name,
                      'pins': {kind: kind + '-commit' for kind in p.REPOS},
                      'workflow_revisions': {'native': 'audited'}, 'builds': {}, 'validation': {}}

    def dispatch(self, retry=False):
        p.dispatch(self.state, self.path, 'native', 'linux-x86_64', 'native.yml', {'tag': 'v1.2.3'}, retry)

    def test_no_cloud_before_local_tests(self):
        with patch.object(p, 'command') as command, self.assertRaisesRegex(ValueError, 'local validation'):
            p.build(self.state, self.path)
        command.assert_not_called()

    def test_missing_or_modified_test_evidence_blocks_build(self):
        self.state['validation'] = {'pins': self.state['pins'], 'core': {'exit_code': 0, 'log_sha256': 'bad'}, 'clients': {'exit_code': 0}}
        for content in [None, 'changed']:
            if content:
                (self.path.parent / 'core-tests.log').write_text(content)
            with self.assertRaisesRegex(ValueError, 'evidence'):
                p.build(self.state, self.path)

    def test_recover_dispatch_without_duplicate(self):
        run = {'headSha': 'audited', 'databaseId': 123, 'url': 'run-url'}
        with patch.object(p, 'find_run', return_value=run), patch.object(p.subprocess, 'run') as mutation:
            self.dispatch()
        mutation.assert_not_called()
        self.assertEqual(self.state['builds']['native/linux-x86_64']['run_id'], 123)

    def test_unknown_network_outcome_is_not_dispatched_twice(self):
        def interrupted(*args, **kwargs):
            saved = json.loads(self.path.read_text())
            self.assertTrue(saved['builds']['native/linux-x86_64']['dispatch_started'])
            raise OSError('connection lost after dispatch')
        with patch.object(p, 'find_run', return_value=None), patch.object(p.subprocess, 'run', side_effect=interrupted):
            with self.assertRaises(OSError):
                self.dispatch()
        with patch.object(p, 'find_run', return_value=None), patch.object(p.subprocess, 'run') as mutation:
            with self.assertRaisesRegex(ValueError, 'uncertain'):
                self.dispatch()
        mutation.assert_not_called()

    def test_retry_preserves_ready_running_and_successful_work(self):
        for entry, run in [({'artifacts': {'a': 'digest'}}, None),
                           ({'run_id': 1}, {'status': 'in_progress', 'conclusion': None}),
                           ({'run_id': 1}, {'status': 'completed', 'conclusion': 'success'})]:
            self.state['builds'] = {'native/linux-x86_64': entry.copy()}
            with patch.object(p, 'api', return_value=run), patch.object(p, 'find_run') as find:
                self.dispatch(retry=True)
            find.assert_not_called()
            self.assertEqual(self.state['builds']['native/linux-x86_64'], entry)

    def test_retry_failed_platform_keeps_history(self):
        self.state['builds']['native/linux-x86_64'] = {'run_id': 1, 'attempt': 0}
        with patch.object(p, 'api', return_value={'status': 'completed', 'conclusion': 'failure'}), patch.object(p, 'find_run', return_value={'headSha': 'audited', 'databaseId': 2, 'url': 'url'}):
            self.dispatch(retry=True)
        entry = self.state['builds']['native/linux-x86_64']
        self.assertEqual((entry['attempts'], entry['attempt'], entry['run_id']), ([1], 1, 2))

    def test_collector_preserves_artifact_from_failed_run(self):
        folder = self.path.parent / 'artifacts/elixir/sources'
        folder.mkdir(parents=True)
        (folder / 'SOURCE_COMMIT').write_text(self.state['pins']['elixir'])
        self.state['builds']['elixir/sources'] = {'run_id': 1}
        with patch.object(p, 'api', return_value={'artifacts': [{'name': 'hex-package', 'expired': False}]}), patch.object(p, 'validate_artifact') as validate, patch.object(p.subprocess, 'run') as mutation:
            p.collect(self.state, self.path)
        validate.assert_called_once()
        mutation.assert_not_called()
        self.assertIn('artifacts', self.state['builds']['elixir/sources'])
        (folder / 'SOURCE_COMMIT').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'Cached release artifact changed'):
            p.collect(self.state, self.path)

    def test_unaudited_recovered_workflow_rejected(self):
        with patch.object(p, 'find_run', return_value={'headSha': 'wrong'}), self.assertRaisesRegex(ValueError, 'Workflow revision changed'):
            self.dispatch()

    def test_source_mismatch_rejected_before_payload(self):
        (self.path.parent / 'SOURCE_COMMIT').write_text('wrong')
        with self.assertRaisesRegex(ValueError, 'source revision'):
            p.validate_artifact(self.path.parent, self.state, 'native', 'linux-x86_64')

    def test_audit_rejects_tests_tag_triggers_and_maven_tests(self):
        for text in ['run: cargo +1.93.1 test --locked', 'run: npm test', 'run: pytest -q', 'run: mix test', 'run: ctest', 'run: mvn package', '  tags: [v*]', 'run: bash scripts/build.sh']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                p.audit_text(text, 'fixture')
        p.audit_text('run: mvn -DskipTests package\nrun: cargo build --release', 'fixture')

    def test_quota_preflight_distinguishes_expired_block(self):
        report = {'warnings': ['Publishing is blocked until your usage resets on November 1, 2026.']}
        with self.assertRaisesRegex(ValueError, 'quota preflight'):
            p.reject_blocked_quota(report, datetime.date(2026, 10, 2))
        p.reject_blocked_quota(report, datetime.date(2026, 11, 1))

    def test_request_identity_changes_only_with_source_platform_or_attempt(self):
        key = p.request_key(self.state, 'native', 'linux-x86_64', 0)
        self.state['builds']['unrelated'] = {'run_id': 23}
        self.assertEqual(key, p.request_key(self.state, 'native', 'linux-x86_64', 0))
        self.assertNotEqual(key, p.request_key(self.state, 'native', 'macos-x86_64', 0))
        self.assertNotEqual(key, p.request_key(self.state, 'native', 'linux-x86_64', 1))


if __name__ == '__main__':
    unittest.main()
