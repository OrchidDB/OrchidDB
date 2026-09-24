import unittest
from compare import equal_rows,equivalent
class ComparisonTests(unittest.TestCase):
 def test_multiplicity(self):
  self.assertFalse(equal_rows([[1],[2]],[[1],[1]],False))
  self.assertTrue(equal_rows([[1],[2],[1]],[[2],[1],[1]],False))
 def test_types_and_null(self):
  for a,b in [(True,1),(False,0),(None,0),('1',1)]:self.assertFalse(equivalent(a,b))
 def test_order(self):
  self.assertFalse(equal_rows([[2],[1]],[[1],[2]],True))
  self.assertFalse(equal_rows([[[1,2]]],[[[2,1]]],False))
 def test_numeric_tolerance(self):
  self.assertTrue(equivalent(1,1.0))
  self.assertTrue(equivalent(1,1.0000000001))
  self.assertFalse(equivalent(1,1.0001))
if __name__=='__main__':unittest.main()
