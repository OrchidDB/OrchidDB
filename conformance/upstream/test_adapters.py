import unittest
from cypher import value,rows_equal,native_value,classified_error_matches
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
 def test_cypher_paths_and_multiline_strings(self):
  p=value("<(:A)-[:T]->(:B)<-[:U]-(:C)>")['$path']
  self.assertEqual([s['direction'] for s in p['segments']],['forward','backward'])
  self.assertEqual(p['segments'][1]['relationship']['$relationship']['type'],'U')
  self.assertEqual(value("'first\nsecond'"),'first\nsecond')
  self.assertEqual(value(r"'quote\' slash\\ tab\t'"),"quote' slash\\ tab\t")
 def test_row_multiplicity_and_order(self):
  self.assertFalse(rows_equal([[1],[1]],[[1],[2]],False))
  self.assertTrue(rows_equal([[2],[1]],[[1],[2]],False))
  self.assertFalse(rows_equal([[2],[1]],[[1],[2]],True))
  self.assertFalse(rows_equal([[True]],[[1]],False))
 def test_native_values_are_typed_not_display_guesses(self):
  tag=lambda kind,value:{'type':kind,'value':value}
  self.assertEqual(native_value(tag('string','v[marko]')),'v[marko]')
  self.assertEqual(native_value(tag('list',[tag('int',1),tag('string','1'),tag('null',None)])),[1,'1',None])
  node={'type':'vertex','id':tag('int',1),'label':'Person','properties':{'name':tag('string','Alice')}}
  self.assertEqual(native_value(node),value("(:Person {name:'Alice'})"))
  with self.assertRaises(ValueError):native_value(tag('unknown',1))
 def test_large_integer_comparison_is_exact(self):
  self.assertFalse(rows_equal([[4611686018427387904]],[[4611686018427387905]],False))
  self.assertTrue(rows_equal([[4611686018427387905]],[[4611686018427387905]],False))
 def test_native_internal_ids_preserve_table_and_offset(self):
  a=native_value({'type':'internal_id','value':{'table':1,'offset':3}})
  b=native_value({'type':'internal_id','value':{'table':2,'offset':3}})
  self.assertNotEqual(a,b)
  self.assertFalse(rows_equal([[a]],[[3]],False))
 def test_native_nonfinite_float_tags(self):
  for literal,expected in [('inf','Infinity'),('-inf','-Infinity'),('NaN','NaN')]:
   self.assertEqual(native_value({'type':'double','value':literal}),{'$float':expected})
   self.assertEqual(native_value({'type':'string','value':literal}),literal)
 def test_classified_errors_require_type_detail_and_phase(self):
  actual={'type':'SyntaxError','detail':'UndefinedVariable','phase':'compile time'}
  self.assertTrue(classified_error_matches('a SyntaxError should be raised at compile time: UndefinedVariable',actual))
  self.assertFalse(classified_error_matches('a SyntaxError should be raised at runtime: UndefinedVariable',actual))
  self.assertFalse(classified_error_matches('a SyntaxError should be raised at compile time: VariableAlreadyBound',actual))
  self.assertFalse(classified_error_matches('a TypeError should be raised at any time: *',actual))
  self.assertTrue(classified_error_matches('a SyntaxError should be raised at any time: *',actual))
  self.assertIsNone(classified_error_matches('a SyntaxError should be raised at compile time: UndefinedVariable',None))
  self.assertIsNone(classified_error_matches('a SyntaxError should be raised at compile time: UndefinedVariable',{'type':'SyntaxError'}))
 def test_blank_identity_across_rows(self):
  b=lambda x:{'type':'bnode','value':x}
  self.assertTrue(rdf_rows([[b('a')],[b('a')]],[[b('z')],[b('z')]],False))
  self.assertFalse(rdf_rows([[b('a')],[b('b')]],[[b('z')],[b('z')]],False))
 def test_rdf_literal_identity(self):
  term=lambda v,d:{'type':'literal','value':v,'datatype':d,'lang':None}
  self.assertFalse(rdf_rows([[term('1','integer')]],[[term('1','string')]],False))
if __name__=='__main__':unittest.main()
