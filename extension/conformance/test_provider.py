"""Provider evidence checks, independent of query execution or expected values."""
import copy
import json
from pathlib import Path
import unittest

from provider import original_provider_cases, verify_provider_result

ROOT = Path(__file__).resolve().parents[2]


class ProviderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        catalog = json.loads((ROOT / 'conformance/upstream/catalog.json').read_text())
        cls.mappings = original_provider_cases(ROOT, catalog, ROOT / 'conformance/upstream/cache/tinkerpop')

    def evidence(self):
        entry = next(iter(self.mappings.values()))
        result = {'status': 'pass', 'engine_instance': 'test-instance',
                  'java_assertion': dict(zip(
                      ['class', 'method', 'source', 'source_sha256', 'feature_source_sha256'],
                      [entry[key] for key in ['java_class', 'java_method', 'source', 'source_sha256', 'feature_source_sha256']]
                  ), run_count=1, failure_count=0, ignored_count=0, assumption_count=0),
                  'query_transports': [{'backend': 'duckdb-extension', 'engine_instance': 'test-instance',
                                        'extension_artifact_sha256': 'test-artifact'}]}
        return entry, result

    def test_every_placeholder_maps_to_unchanged_original_source(self):
        self.assertEqual(len(self.mappings), 15)

    def test_valid_evidence_is_preserved(self):
        entry, result = self.evidence()
        self.assertIs(verify_provider_result(result, entry, 'test-artifact'), result)

    def test_pass_requires_original_assertion_and_same_extension_instance(self):
        entry, valid = self.evidence()
        for mutation in (
            lambda r: r['java_assertion'].update(assumption_count=1),
            lambda r: r['java_assertion'].update(run_count=0),
            lambda r: r['java_assertion'].update(method='replacement'),
            lambda r: r.update(query_transports=[]),
            lambda r: r['query_transports'][0].update(backend='graphengine'),
            lambda r: r['query_transports'][0].update(engine_instance='another-instance'),
            lambda r: r['query_transports'][0].update(extension_artifact_sha256='different-artifact'),
        ):
            result = copy.deepcopy(valid)
            mutation(result)
            self.assertEqual(verify_provider_result(result, entry, 'test-artifact')['status'], 'adapter-error')

    def test_failure_is_not_reclassified_as_a_pass(self):
        entry, result = self.evidence()
        result.update(status='fail', error='original assertion failed')
        self.assertIs(verify_provider_result(result, entry, 'test-artifact'), result)


if __name__ == '__main__':
    unittest.main()
