"""Real C++ Arrow bridge probes, available only in diagnostic extension builds.

Build with CXXFLAGS=-DORCHID_TEST_ARROW_STREAM and set ORCHID_STREAM_TESTS=1.
The production extension does not register these diagnostic functions.
"""
import json
import os
import unittest
from test_extension import connect


@unittest.skipUnless(os.environ.get("ORCHID_STREAM_TESTS") == "1", "requires Arrow diagnostic extension build")
class HostStreamingTests(unittest.TestCase):
    def test_lazy_input_bounded_output_replay_and_release(self):
        db = connect()
        try:
            report = json.loads(db.execute("SELECT __orchid_stream_self_test()").fetchone()[0])
            self.assertEqual(report["limit"]["pulls"], 1)
            self.assertEqual(report["limit"]["rows"], 1)
            self.assertEqual(report["projection"]["rows"], 10)
            self.assertEqual(report["replay"]["rows"], 1)
            # Five Arrow batches plus EOS, not two independent remote scans.
            self.assertEqual(report["replay"]["pulls"], 6)
            self.assertEqual(report["recursive_replay"]["pulls"], 6)
            self.assertEqual(report["recursive_replay"]["rows"], 1)
            self.assertEqual(report["stream_output"]["rows"], 2048)
            # Source has 100,000 rows. Early result drop must not drain it.
            self.assertLessEqual(report["stream_output"]["pulls"], 6)
            for scenario in report.values():
                self.assertEqual(scenario["releases"], 1)
            self.assertEqual(db.execute("SELECT 42").fetchone(), (42,))
        finally:
            db.close()

    def test_producer_error_fails_query_and_connection_recovers(self):
        db = connect()
        try:
            with self.assertRaisesRegex(Exception, "injected Arrow source failure"):
                db.execute("SELECT __orchid_stream_case('error')").fetchall()
            self.assertEqual(db.execute("SELECT 42").fetchone(), (42,))
        finally:
            db.close()
