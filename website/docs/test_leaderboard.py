import unittest
from leaderboard import passed_runtime

class PassedRuntimeTests(unittest.TestCase):
    def test_only_successful_cases_contribute(self):
        rows=[{'status':s,'elapsed_ms':9999} for s in ('fail','timeout','skipped','unsupported','adapter-error')]
        rows += [{'status':'pass','elapsed_ms':125.5},{'status':'pass','elapsed_ms':0}]
        self.assertEqual(passed_runtime(rows),{'elapsed_ms':125.5,'timed_passes':2,'passed':2})

    def test_missing_or_invalid_timing_is_not_zero(self):
        for value in (None, float('nan'),float('inf'),-1,True,'10'):
            with self.subTest(value=value):
                self.assertIsNone(passed_runtime([{'status':'pass','elapsed_ms':value}])['elapsed_ms'])

    def test_no_passes_has_zero_total(self):
        self.assertEqual(passed_runtime([{'status':'fail'}])['elapsed_ms'],0)

if __name__=='__main__':unittest.main()
