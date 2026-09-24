import unittest
from run import execution_profile, gremlin_capability

class JvmProfileTests(unittest.TestCase):
 def test_separate_execution_and_language_claims(self):
  self.assertEqual(execution_profile('crabgraph')['executor'],'native Rust planner')
  self.assertEqual(execution_profile('crabgraph')['traversal_language'],'gremlin-language')
  self.assertEqual(execution_profile('crabgraph-jvm')['execution'],'OLTP')
  self.assertEqual(execution_profile('crabgraph-jvm')['traversal_language'],'gremlin-groovy')
  self.assertEqual(execution_profile('crabgraph-computer')['execution'],'GraphComputer')
 def test_null_policy_exclusion_is_explicit(self):
  self.assertEqual(gremlin_capability({'status':'skipped','error':'Upstream execution profile excludes @DisallowNullPropertyValues'})['name'],'null-as-removal')

if __name__=='__main__':unittest.main()
