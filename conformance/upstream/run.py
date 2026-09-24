#!/usr/bin/env python3
"""Run pinned upstream scenarios locally; publish only these recorded artifacts."""
import argparse,datetime,hashlib,json,os,platform,select,signal,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];REPO=ROOT.parent
def gremlin_capability(result):
 """Describe observed exclusions without predicting outcomes or changing status."""
 if result.get('status') not in ('skipped','unsupported'):return None
 reason=result.get('error') or result.get('reason','')
 categories=[
  ('unsupported-feature: remote-lambda:', 'F18', 'remote-lambda', 'language-profile'),
  ('excludes @GraphComputerOnly', 'S01', 'graph-computer', 'execution-profile'),
  ('lambda as a parameter', 'S02', 'lambda-parameter', 'upstream-language'),
  ('fixture has multi/meta-properties', 'S03', 'multi-meta-property-fixture', 'adapter'),
  ('SQLg does not support the upstream CREW', 'S03', 'multi-meta-property-fixture', 'provider'),
  ('assert the remotely written file', 'S04', 'remote-file-assertion', 'upstream-assertion'),
  ('excludes @AllowNullPropertyValues', 'S05', 'null-property-values', 'execution-profile'),
  ('excludes @DisallowNullPropertyValues', None, 'null-as-removal', 'execution-profile'),
  ('Edge as a parameter', 'S06', 'edge-parameter', 'upstream-language'),
  ('property identifiers and related assertions', 'S07', 'property-identifier-assertion', 'upstream-assertion'),
  ('empty Set as a parameter', 'S08', 'empty-set-parameter', 'upstream-language'),
  ('not supported by Gherkin because:', 'S09', 'gherkin-assertion', 'upstream-assertion'),
  ('excludes @RemoteOnly', None, 'remote-only', 'execution-profile'),
  ('Path as a parameter', None, 'path-parameter', 'upstream-language'),
  ('Set as a parameter', None, 'set-parameter', 'upstream-language'),
 ]
 for text,work_item,capability,origin in categories:
  if text in reason:
   return {'name':capability,'origin':origin,**({'work_item':work_item} if work_item else {})}
 return {'name':'unclassified','origin':'unknown'}

def capability_summary(results):
 from collections import Counter
 return dict(Counter(capability['name'] for r in results if (capability:=gremlin_capability(r))))

JVM_ENGINES=('crabgraph-jvm','crabgraph-computer')
def file_identity(path):
 path=Path(path).resolve()
 return {'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
def execution_profile(engine):
 return {'traversal_language':'gremlin-groovy' if engine in JVM_ENGINES else 'gremlin-language',
         'assertions':'Apache gremlin-test 3.7.4 StepDefinition (unmodified)',
         'execution':'GraphComputer' if engine=='crabgraph-computer' else 'OLTP',
         'null_properties':'stored null' if engine in JVM_ENGINES else 'per-scenario @AllowNullPropertyValues opt-in' if engine=='crabgraph' else 'provider default',
         'executor':'Crabgraph JVM provider over native store' if engine in JVM_ENGINES else 'native Rust planner' if engine=='crabgraph' else engine,
         'remote': 'inline Lambda bytecode submissions only' if engine in JVM_ENGINES else engine not in ('reference','sqlg')}
def jvm_build(classpath):
 binary=Path(os.environ.get('CRABGRAPH_JVM_STORE',str(REPO/'target/debug/crabgraph-jvm-store')))
 artifacts=[Path(entry) for entry in classpath.split(os.pathsep) if 'crabgraph-jvm' in Path(entry).name and entry.endswith('.jar')]
 if len(artifacts)!=1:raise RuntimeError('Expected one pinned crabgraph-jvm jar in conformance classpath')
 return {'native_store':file_identity(binary),'jvm_bridge':file_identity(artifacts[0]),
         'adapter_source':file_identity(ROOT/'adapters/sqlg/src/main/java/UpstreamGremlin.java'),
         'adapter_class':file_identity(next(Path(entry)/'UpstreamGremlin.class' for entry in classpath.split(os.pathsep) if (Path(entry)/'UpstreamGremlin.class').is_file())),
         'tinkerpop_version':'3.7.4',
         'java':subprocess.check_output([os.environ.get('CONFORMANCE_JAVA','java'),'-version'],stderr=subprocess.STDOUT,text=True).strip(),
         'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),
         'working_tree_modified':bool(subprocess.check_output(['git','status','--porcelain'],cwd=REPO,text=True).strip())}

def crabgraph_binary():
 return Path(os.environ.get('CONFORMANCE_CRABGRAPH_BINARY',str(REPO/'target/debug/upstream'))).resolve()
class Process:
 def __init__(self,command,log,ready=False):
  self.log=open(log,'a');self.p=subprocess.Popen(command,cwd=REPO,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,bufsize=1,start_new_session=True,env={**os.environ,'CONFORMANCE_PYTHON':os.environ.get('CONFORMANCE_PYTHON',os.sys.executable)})
  if ready:self.read(90)
 def read(self,timeout=40):
  if not select.select([self.p.stdout],[],[],timeout)[0]:self.close();raise TimeoutError('scenario deadline; adapter process group terminated')
  line=self.p.stdout.readline()
  if not line:raise RuntimeError('adapter process exited')
  return json.loads(line)
 def send(self,obj,timeout=40):
  self.p.stdin.write(json.dumps(obj)+'\n');self.p.stdin.flush();return self.read(timeout)
 def close(self):
  if self.p.poll() is None:
   os.killpg(self.p.pid,signal.SIGKILL);self.p.wait()
  self.log.close()
class Gremlin:
 def __init__(self,engine):
  d=ROOT/'adapters/sqlg';classpath=os.environ.get('CONFORMANCE_GREMLIN_CLASSPATH')
  if not classpath:classpath=str(d/'target/classes')+os.pathsep+(d/'classpath.txt').read_text().strip()
  self.classpath=classpath
  self.engine=engine;self.command=[os.environ.get('CONFORMANCE_JAVA','java'),'--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED','--add-opens=java.base/java.lang=ALL-UNNAMED','--add-opens=java.base/java.util=ALL-UNNAMED','--add-opens=java.base/java.lang.invoke=ALL-UNNAMED','-Dorg.slf4j.simpleLogger.defaultLogLevel=error','-cp',classpath,'UpstreamGremlin',engine];self.process=None
 def run(self,case):
  if self.process is None or self.process.p.poll() is not None:self.process=Process(self.command,ROOT/f'upstream-{self.engine}-gremlin.log',True)
  return self.process.send(case,timeout=45 if 'grateful' not in str(case['steps']) else 90)
 def close(self):
  if self.process:self.process.close()
def main():
 p=argparse.ArgumentParser();p.add_argument('--engine',choices=['crabgraph','crabgraph-jvm','crabgraph-computer','sqlg','puppygraph','reference'],required=True);p.add_argument('--suite',choices=['opencypher','tinkerpop','rdf'],required=True);p.add_argument('--limit',type=int);p.add_argument('--filter',default='');p.add_argument('--case',action='append',default=[],help='Exact upstream case ID, repeatable');p.add_argument('--resume',action='store_true');p.add_argument('--output',type=Path);args=p.parse_args()
 catalog=json.loads((ROOT/'upstream/catalog.json').read_text());cases=[c for c in catalog['cases'] if c['suite']==args.suite and args.filter in c['id'] and (not args.case or c['id'] in args.case)];cases=cases[:args.limit] if args.limit else cases
 if args.case:
  missing=set(args.case)-{c['id'] for c in cases}
  if missing:p.error('Unknown or filtered case IDs: '+', '.join(sorted(missing)))
 if (args.limit or args.filter or args.case) and not args.output:p.error('Subset runs require --output')
 output=args.output or ROOT/'upstream-results'/f'{args.engine}-{args.suite}.json';output.parent.mkdir(parents=True,exist_ok=True);journal=output.with_suffix('.jsonl')
 results=[]
 if args.resume and journal.exists():
  for line in journal.read_text().splitlines():
   try:results.append(json.loads(line))
   except json.JSONDecodeError:break
 else:journal.write_text('')
 done={r['id'] for r in results};started=datetime.datetime.now(datetime.timezone.utc).isoformat()
 applicable=args.suite=='tinkerpop' or args.suite=='opencypher' and args.engine in ['crabgraph','puppygraph'] or args.suite=='rdf' and args.engine=='crabgraph'
 adapter=None
 if applicable:
  if args.suite=='tinkerpop':adapter=Gremlin(args.engine)
  elif args.suite=='rdf':
   from sparql import Sparql
   adapter=Sparql()
  else:
   from cypher import Cypher
   adapter=Cypher(args.engine)
 try:
  with journal.open('a') as f:
   for i,case in enumerate(cases):
    if case['id'] in done:continue
    before=time.monotonic()
    try:result=adapter.run(case) if adapter else {'status':'not-applicable','reason':'No native interface for this suite in the compared product'}
    except TimeoutError as e:result={'status':'timeout','reason':str(e)}
    except Exception as e:result={'status':'adapter-error','reason':str(e)}
    if args.suite=='tinkerpop':
     capability=gremlin_capability(result)
     if capability:result={**result,'capability':capability}
    result={'id':case['id'],'case_sha256':hashlib.sha256(json.dumps(case,sort_keys=True).encode()).hexdigest(),'elapsed_ms':round((time.monotonic()-before)*1000,3),**result}
    results.append(result);f.write(json.dumps(result)+'\n');f.flush()
    if (i+1)%50==0:print(args.engine,args.suite,i+1,'/',len(cases),flush=True)
 finally:
  if adapter:adapter.close()
 content={'schema_version':3,'engine':args.engine,'suite':args.suite,'source':catalog['sources'][args.suite],'started_at':started,'finished_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'environment':{'system':platform.system(),'architecture':platform.machine(),'python':platform.python_version(),'logical_cpus':os.cpu_count()},'coverage':{'catalog_cases':len([c for c in catalog['cases'] if c['suite']==args.suite]),'recorded_cases':len(results),'filtered':bool(args.limit or args.filter or args.case)},'results':results}
 if args.suite=='tinkerpop':
  content['execution_profile']=execution_profile(args.engine)
  content['coverage']['excluded_capabilities']=capability_summary(results)
 if args.engine in JVM_ENGINES:
  content['build']=jvm_build(adapter.classpath if adapter else os.environ.get('CONFORMANCE_GREMLIN_CLASSPATH',''))
 elif args.engine=='crabgraph':
  binary=crabgraph_binary();content['build']={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'working_tree_modified':bool(subprocess.check_output(['git','status','--porcelain'],cwd=REPO,text=True).strip())}
 else:content['build']={'version':{'sqlg':'3.1.6','puppygraph':'1.11.1','reference':'3.7.4'}[args.engine]}
 output.write_text(json.dumps(content,indent=2)+'\n')
 from collections import Counter
 print(args.engine,args.suite,dict(Counter(r['status'] for r in results)),flush=True)
if __name__=='__main__':main()
