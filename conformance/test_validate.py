import copy
import unittest

from validate import compact_evidence, validate_costs, validate_single_instance
from upstream.query_cost import summarize


class PublishedEvidenceValidationTest(unittest.TestCase):
    def profile(self):
        return {'execution_profile': {'single_instance_verified': True,
                                      'engine_instances': ['engine-1']},
                'results': [{'id': 'case'}]}

    def test_compact_requires_verified_single_instance_manifest(self):
        report = {**self.profile(), 'evidence_format': 'compact-v1'}
        validate_single_instance(report, 'test')
        for value in (False, None):
            changed = copy.deepcopy(report)
            changed['execution_profile']['single_instance_verified'] = value
            with self.assertRaises(AssertionError):
                validate_single_instance(changed, 'test')
        for instances in ([], ['one', 'two'], [None]):
            changed = copy.deepcopy(report)
            changed['execution_profile']['engine_instances'] = instances
            with self.assertRaises(AssertionError):
                validate_single_instance(changed, 'test')

    def test_raw_still_requires_matching_instance_in_every_row(self):
        report = self.profile()
        with self.assertRaises(AssertionError):
            validate_single_instance(report, 'test')
        report['results'][0]['engine_instance'] = 'engine-1'
        validate_single_instance(report, 'test')
        report['results'].append({'engine_instance': 'another-engine'})
        with self.assertRaises(AssertionError):
            validate_single_instance(report, 'test')

    def test_compact_cost_totals_and_rankings_are_checked_without_queries(self):
        report = {'evidence_format': 'compact-v1', 'results': [{
            'id': 'case', 'query_cost': {'metric_version': 1, 'query_count': 1,
            'measured_queries': 1, 'request_elapsed_micros': 25,
            'coverage': 'boundary_work', 'work_units': 100}}]}
        report['query_cost_summary'] = summarize(report['results'], 50)
        validate_costs(report, 'test')
        changed = copy.deepcopy(report)
        changed['query_cost_summary']['highest_work'][0]['work_units'] += 1
        with self.assertRaises(AssertionError):
            validate_costs(changed, 'test')
        del report['results'][0]['query_cost']
        with self.assertRaises(AssertionError):
            validate_costs(report, 'test')

    def test_unknown_format_cannot_bypass_raw_validation(self):
        with self.assertRaises(AssertionError):
            compact_evidence({'evidence_format': 'compact-v2'}, 'test')


if __name__ == '__main__':
    unittest.main()
