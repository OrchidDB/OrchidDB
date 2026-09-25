import unittest,json,tempfile
from pathlib import Path
from unittest.mock import patch
import run
from run import execution_profile, gremlin_capability

class JvmProfileTests(unittest.TestCase):
 def test_separate_execution_and_language_claims(self):
  self.assertEqual(execution_profile('orchiddb')['executor'],'SQL IR DAG executed by DuckDB and DataFusion, including JVM compute operators')
  self.assertEqual(execution_profile('orchiddb')['traversal_language'],'gremlin-language')
  self.assertEqual(execution_profile('orchiddb-jvm')['execution'],'OLTP')
  self.assertEqual(execution_profile('orchiddb-jvm')['traversal_language'],'gremlin-groovy')
  self.assertEqual(execution_profile('orchiddb-computer')['execution'],'GraphComputer')
 def test_build_identity_is_captured_before_any_scenario_runs(self):
  events=[]
  class Adapter:
   classpath='frozen-classes:frozen-orchiddb-jvm.jar'
   def __init__(self,engine):pass
   def run(self,case):events.append('scenario');return {'status':'pass'}
   def close(self):events.append('close')
  def capture(classpath):
   events.append('build')
   return {'revision':'source-at-launch','classpath':classpath}
  with tempfile.TemporaryDirectory() as folder:
   root=Path(folder);(root/'upstream').mkdir();output=root/'result.json'
   (root/'upstream/catalog.json').write_text(json.dumps({'sources':{'tinkerpop':{'revision':'pinned'}},'cases':[{'id':'case','suite':'tinkerpop','tags':[],'steps':[]}]}))
   with patch.object(run,'ROOT',root),patch.object(run,'Gremlin',Adapter),patch.object(run,'jvm_build',side_effect=capture),patch('sys.argv',['run.py','--engine','orchiddb-jvm','--suite','tinkerpop','--output',str(output)]):
    run.main()
   recorded=json.loads(output.read_text())
   self.assertEqual(events,['build','scenario','close'])
   self.assertEqual(recorded['build']['revision'],'source-at-launch')
   self.assertEqual(recorded['build']['capture_phase'],'before-scenarios')
   self.assertIn('captured_at',recorded['build'])
 def test_null_policy_exclusion_is_explicit(self):
  self.assertEqual(gremlin_capability({'status':'skipped','error':'Upstream execution profile excludes @DisallowNullPropertyValues'})['name'],'null-as-removal')
 def test_optional_codec_jar_does_not_ambiguate_executor_identity(self):
  with tempfile.TemporaryDirectory() as folder:
   root=Path(folder);(root/'UpstreamGremlin.class').write_bytes(b'compiled')
   executor=root/'orchiddb-jvm-0.1.0.jar';codec=root/'orchiddb-jvm-codecs-0.1.0.jar'
   with patch.object(run,'file_identity',side_effect=lambda p:{'path':str(p)}),patch.object(run.subprocess,'check_output',return_value=''):
    build=run.jvm_build(':'.join(map(str,[root,codec,executor])))
   self.assertEqual(build['jvm_bridge']['path'],str(executor))

if __name__=='__main__':unittest.main()
