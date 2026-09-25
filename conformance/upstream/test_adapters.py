import unittest,tempfile
from pathlib import Path
from cypher import value,rows_equal,native_value,classified_error_matches
from sparql import rows_equal as rdf_rows,relocated_graph_name
from bridge import property_type
class AdapterTests(unittest.TestCase):
 def test_driver_temporal_notation_preserves_offset_and_precision(self):
  from datetime import timezone,timedelta
  from neo4j.time import Time,DateTime,Duration
  from cypher_driver import temporal
  self.assertEqual(temporal(Time(10,35)), '10:35')
  self.assertEqual(temporal(Time(12,30,14,645876123)), '12:30:14.645876123')
  self.assertEqual(temporal(DateTime(1980,12,11,12,31,14,tzinfo=timezone(timedelta(hours=-11,minutes=-59)))), '1980-12-11T12:31:14-11:59')
  self.assertEqual(temporal(Duration(months=13,seconds=3600)), 'P1Y1MT1H')
  from cypher import normalize
  self.assertEqual(normalize(Duration(seconds=79200)), 'PT22H')
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
 def test_named_graph_fixture_relocation(self):
  with tempfile.TemporaryDirectory() as folder:
   base=Path(folder)/'new checkout';relative='sparql/sparql10/graph/data-g1.ttl';current=base/relative
   current.parent.mkdir(parents=True);current.write_text('<#s> <https://example/p> <#o>.')
   old=(Path('/old/checkout')/relative).as_uri()
   self.assertEqual(relocated_graph_name(base,relative,old),current.absolute().as_uri())
   for explicit in ['https://example/graph','urn:graph:one','file:///unrelated/data-g1.ttl',old+'#graph',old+'?version=1']:
    self.assertEqual(relocated_graph_name(base,relative,explicit),explicit)
   self.assertIsNone(relocated_graph_name(base,relative,None))
   self.assertEqual(relocated_graph_name(base,'missing.ttl','file:///old/missing.ttl'),'file:///old/missing.ttl')
   from rdflib import Graph
   parsed=Graph().parse(current)
   self.assertIn(current.absolute().as_uri()+'#s',[str(s) for s in parsed.subjects()])
 def test_rdf_literal_identity(self):
  term=lambda v,d:{'type':'literal','value':v,'datatype':d,'lang':None}
  self.assertFalse(rdf_rows([[term('1','integer')]],[[term('1','string')]],False))
if __name__=='__main__':unittest.main()
