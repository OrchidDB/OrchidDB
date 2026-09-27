"""Snapshot RPC unit tests; no engine process or conformance suite runs."""
import json
import unittest
from cypher import Cypher

KINDS=('nodes','relationships','labels','node_properties','edge_properties')
def tag(kind,value):return {'type':kind,'value':value}
class FakeProcess:
 def __init__(self):self.requests=[];self.p=self;self.error=False
 def poll(self):return None
 def send(self,request,**kwargs):
  self.requests.append(request)
  if request['op']=='cypher-snapshot':
   if self.error:return {'error':'storage unavailable'}
   result={kind:[] for kind in KINDS};result['nodes']=[[tag('int',7)]]
   result['node_properties']=[[tag('int',7),tag('string','when'),tag('cypher_temporal',{'kind':'date','text':'2020-02-03'})]]
   return {'native_snapshot':result}
  if request['op']=='cypher':return {'columns':['n'],'rows':[[1]]}
  return {'ok':True}
class SnapshotTests(unittest.TestCase):
 def adapter(self):
  adapter=Cypher('orchiddb');adapter.rust=FakeProcess();return adapter
 def test_rpc_uses_existing_native_normalization_and_records_helper_time(self):
  adapter=self.adapter();adapter.query_phase='query';snapshot=adapter.snapshot()
  self.assertEqual(snapshot['nodes'],{json.dumps([7])})
  self.assertEqual(snapshot['node_properties'],{json.dumps([7,'when','2020-02-03'])})
  self.assertEqual(adapter.query_phase,'query');self.assertEqual(len(adapter.rust.requests),1)
  cost=adapter.query_costs[0]
  self.assertEqual((cost['phase'],cost['operation']),('observation','cypher-snapshot'))
  self.assertEqual(cost['cost']['coverage'],'elapsed_only');self.assertIsNone(cost['cost']['work_units'])
 def test_failure_cannot_be_mistaken_for_empty_graph(self):
  adapter=self.adapter();adapter.rust.error=True;adapter.query_phase='control'
  with self.assertRaisesRegex(ValueError,'Cannot observe'):adapter.snapshot()
  self.assertEqual(adapter.query_phase,'control')
 def test_scalar_identity_transport_is_lossless(self):
  from cypher import native_value
  key=[0,255,3,128]
  self.assertEqual(native_value(tag('scalar',{'scalar_key':key})),{'$scalar':key})
  self.assertNotEqual(native_value(tag('scalar',{'scalar_key':key})),native_value(tag('scalar',{'scalar_key':key+[0]})))
 def test_incomplete_snapshot_is_an_adapter_error(self):
  adapter=self.adapter()
  adapter.rust.send=lambda *args,**kwargs:{'native_snapshot':{'nodes':[]}}
  with self.assertRaisesRegex(ValueError,'Malformed'):adapter.snapshot()
 def test_only_side_effect_assertions_request_snapshots(self):
  base=[{'text':'an empty graph'},{'text':'executing query:','doc':'RETURN 1 AS n'},{'text':'the result should be:','table':[['n'],['1']]}]
  for effects,count in [(False,0),(True,2)]:
   adapter=self.adapter();steps=base+([{'text':'no side effects'}] if effects else [])
   result=adapter.run({'steps':steps});self.assertEqual(result['status'],'pass',result)
   self.assertEqual(sum(request['op']=='cypher-snapshot' for request in adapter.rust.requests),count)
   self.assertEqual(sum(request['op']=='cypher' for request in adapter.rust.requests),1)
if __name__=='__main__':unittest.main()
