import unittest
from cypher import value,rows_equal
from sparql import rows_equal as rdf_rows
from bridge import property_type
class AdapterTests(unittest.TestCase):
 def test_fixture_numeric_width(self):
  self.assertEqual(property_type(29,'Integer'),('INTEGER','Int'))
  self.assertEqual(property_type(29,'Long'),('BIGINT','Long'))
  self.assertEqual(property_type(29),('BIGINT','Long'))
  with self.assertRaises(ValueError):property_type(0.5,'Float')
 def test_cypher_values(self):
  self.assertEqual(value("{a:[1,null,true], b:'x'}"),{'a':[1,None,True],'b':'x'})
  self.assertEqual(value("(:B:A {x:1})"),{'$node':{'labels':['A','B'],'properties':{'x':1}}})
 def test_row_multiplicity_and_order(self):
  self.assertFalse(rows_equal([[1],[1]],[[1],[2]],False))
  self.assertTrue(rows_equal([[2],[1]],[[1],[2]],False))
  self.assertFalse(rows_equal([[2],[1]],[[1],[2]],True))
  self.assertFalse(rows_equal([[True]],[[1]],False))
 def test_blank_identity_across_rows(self):
  b=lambda x:{'type':'bnode','value':x}
  self.assertTrue(rdf_rows([[b('a')],[b('a')]],[[b('z')],[b('z')]],False))
  self.assertFalse(rdf_rows([[b('a')],[b('b')]],[[b('z')],[b('z')]],False))
 def test_rdf_literal_identity(self):
  term=lambda v,d:{'type':'literal','value':v,'datatype':d,'lang':None}
  self.assertFalse(rdf_rows([[term('1','integer')]],[[term('1','string')]],False))
if __name__=='__main__':unittest.main()
