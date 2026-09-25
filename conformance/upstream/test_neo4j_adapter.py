"""Transport and diagnostic regressions independent of TCK expectations."""
import unittest
from datetime import date
from unittest.mock import Mock

from cypher import Cypher, normalize
from cypher_driver import bolt_date, install_lossless_temporal_hydration
from neo4j_errors import classify


class Neo4jAdapterTests(unittest.TestCase):
    def test_epoch_days_include_wide_and_negative_years(self):
        for value in (date(1, 1, 1), date(1600, 2, 29), date(1970, 1, 1), date(9999, 12, 31)):
            self.assertEqual(bolt_date((value - date(1970, 1, 1)).days), value.isoformat())
        # Gregorian years repeat after 146097 days (400 years).
        epoch_year_zero = (date(400, 1, 1) - date(1970, 1, 1)).days - 146097
        self.assertEqual(bolt_date(epoch_year_zero), '0000-01-01')
        self.assertEqual(bolt_date(epoch_year_zero - 1), '-0001-12-31')
        self.assertEqual(bolt_date(epoch_year_zero + 2500 * 146097), '+1000000-01-01')

    def test_bolt_timezone_seconds_and_nanoseconds(self):
        install_lossless_temporal_hydration()
        from neo4j._codec.hydration.v1.temporal import hydrate_time
        from neo4j._codec.hydration.v2.temporal import hydrate_datetime
        seconds = 12 * 3600 + 34 * 60 + 56
        self.assertEqual(normalize(hydrate_time(seconds * 10**9 + 123, 7559)), '12:34:56.000000123+02:05:59')
        self.assertEqual(normalize(hydrate_time(seconds * 10**9, -7507)), '12:34:56-02:05:07')
        self.assertEqual(normalize(hydrate_datetime(0, 123, 7559)), '1970-01-01T02:05:59.000000123+02:05:59')
        self.assertEqual(normalize(hydrate_datetime(0, 0, None)), '1970-01-01T00:00')
        self.assertEqual(normalize(hydrate_datetime(-2208988800, 0, 'Europe/Stockholm')),
                         '1900-01-01T01:00+01:00[Europe/Stockholm]')
        self.assertEqual(normalize(hydrate_datetime(0, 0, 'UTC')), '1970-01-01T00:00Z')

    def test_classification_has_no_expected_assertion_input(self):
        actual = classify('Neo.ClientError.Statement.SyntaxError', 'Variable `x` not defined',
                          'compile time', [{'status': '42N62'}])
        self.assertEqual(actual, {'type': 'SyntaxError', 'phase': 'compile time', 'detail': 'UndefinedVariable'})
        self.assertIsNone(classify('Neo.ClientError.Statement.SyntaxError', 'New unknown error', 'compile time'))
        self.assertEqual(classify('Neo.ClientError.Statement.TypeError',
                                 'Invalid input for function labels(): Expected a Node, got: Long(1)',
                                 'runtime')['type'], 'TypeError')

    def test_phase_uses_explain_without_executing_again(self):
        from neo4j.exceptions import Neo4jError
        for compile_failure in (False, True):
            adapter = Cypher.__new__(Cypher)
            adapter.engine = 'neo4j'
            driver = Mock()
            adapter.driver = driver
            execution, explanation = Mock(), Mock()
            driver.session.return_value.__enter__ = Mock(side_effect=[execution, explanation])
            driver.session.return_value.__exit__ = Mock(return_value=False)
            error = Neo4jError._hydrate_neo4j(code='Neo.ClientError.Statement.TypeError',
                                            message='Property values can only be of primitive types or arrays thereof')
            execution.run.side_effect = error
            if compile_failure:
                compilation = Neo4jError._hydrate_neo4j(code='Neo.ClientError.Statement.SyntaxError', message='Invalid input')
                explanation.run.side_effect = compilation
            result = adapter.query('CREATE (n {x: $x})', {'x': {'nested': 1}})
            self.assertEqual(result['classification']['phase'], 'compile time' if compile_failure else 'runtime')
            self.assertEqual(str(explanation.run.call_args.args[0]), 'CYPHER 5 EXPLAIN CREATE (n {x: $x})')
            self.assertEqual(execution.run.call_count, 1)


if __name__ == '__main__':
    unittest.main()
