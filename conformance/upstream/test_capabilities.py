import unittest
from run import gremlin_capability, capability_summary


class CapabilityTests(unittest.TestCase):
 def test_observed_exclusions_have_separate_capabilities(self):
  reasons=[
   ('S01','Upstream execution profile excludes @GraphComputerOnly'),
   ('S02','This test uses a lambda as a parameter which is not supported by gremlin-language'),
   ('S03','adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve'),
   ('S04',"This test is not supported by Gherkin because: We don't have a nice way to assert the remotely written file with this framework"),
   ('S05','Upstream execution profile excludes @AllowNullPropertyValues'),
   ('S06','This test uses a Edge as a parameter which is not supported by gremlin-language'),
   ('S07',"This test is not supported by Gherkin because: GLV suite doesn't support property identifiers and related assertions"),
   ('S08','This test uses a empty Set as a parameter which is not supported by gremlin-language'),
   ('S09','This test is not supported by Gherkin because: GLV Suite does not support BigInteger assignments at this time.'),
  ]
  for work_item,reason in reasons:
   with self.subTest(work_item=work_item):
    result={'status':'skipped','error':reason}
    self.assertEqual(gremlin_capability(result)['work_item'],work_item)
    self.assertEqual(result,{'status':'skipped','error':reason})

 def test_lambda_capability_is_not_a_pass_or_skip(self):
  result={'status':'unsupported','error':'unsupported-feature: remote-lambda: profile cannot compile inline Lambda expressions'}
  self.assertEqual(gremlin_capability(result)['work_item'],'F18')
  self.assertEqual(result['status'],'unsupported')
  for status in ['pass','fail','adapter-error','timeout']:
   self.assertIsNone(gremlin_capability({**result,'status':status}))

 def test_unknown_capabilities_stay_visible(self):
  capability=gremlin_capability({'status':'skipped','reason':'new upstream exclusion'})
  self.assertEqual(capability['name'],'unclassified')
  self.assertEqual(capability_summary([{'status':'skipped','reason':'new upstream exclusion'},{'status':'pass'}]),{'unclassified':1})
  # Resumed journals from before capability metadata still count exclusions.
  self.assertEqual(capability_summary([{'status':'skipped','error':'Upstream execution profile excludes @GraphComputerOnly'}]),{'graph-computer':1})


if __name__=='__main__':unittest.main()
