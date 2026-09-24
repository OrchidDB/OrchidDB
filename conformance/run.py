#!/usr/bin/env python3
"""Run an identical versioned probe set against configured, disposable engines."""
import argparse,datetime,hashlib,json,math,os,select,subprocess,time,sys
from pathlib import Path
from catalog import FIXTURE,TESTS
ROOT=Path(__file__).resolve().parent

from compare import equivalent,equal_rows
import statistics,platform

class Process:
 def __init__(self,command,log,env=None):
  self.log=open(log,'w');self.p=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,bufsize=1,env=env)
 def read(self,timeout=30):
  if not select.select([self.p.stdout],[],[],timeout)[0]:
   self.p.kill(); self.p.wait(); raise TimeoutError('adapter killed after deadline; subsequent probes require a fresh run')
  line=self.p.stdout.readline()
  if not line: raise RuntimeError('adapter exited; inspect its log')
  return json.loads(line)
 def query(self,test):
  self.p.stdin.write(json.dumps(test)+'\n');self.p.stdin.flush();return self.read()
 def close(self):
  self.p.terminate()
  try:self.p.wait(timeout=5)
  except subprocess.TimeoutExpired:self.p.kill();self.p.wait()
  self.log.close()

class Crabgraph:
 languages={'cypher','gremlin','sparql'}
 def __init__(self):
  binary=Path(os.environ.get('CRABGRAPH_CONFORMANCE_BIN',ROOT.parent/'target/debug/crabgraph-conformance-runner')).resolve()
  self.version={'version':'0.1.0','edition':'repository build','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'read_mode':os.environ.get('CONFORMANCE_READ_MODE','hybrid')}
  self.version.update(git_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),working_tree_modified=bool(subprocess.check_output(['git','status','--porcelain','--','src','Cargo.toml','Cargo.lock'],cwd=ROOT,text=True).strip()))
  self.proc=Process([str(binary)],ROOT/'crabgraph.log')
  for q in FIXTURE:
   result=self.proc.query({'query':q})
   if 'error' in result:raise RuntimeError(result['error'])
 def query(self,test):return self.proc.query(test)
 def close(self):self.proc.close()

class Ladybug:
 languages={'cypher'}
 def __init__(self):
  import ladybug
  self.version={'version':ladybug.__version__,'edition':'open source','storage':'in-memory','default_path_mode':'walk'}
  self.db=ladybug.Database(':memory:',buffer_pool_size=128*1024*1024,max_num_threads=2);self.conn=ladybug.Connection(self.db)
  self.conn.set_query_timeout(15000)
  for q in ['CREATE NODE TABLE Person(uid INT64 PRIMARY KEY,name STRING,age INT64,score INT64)','CREATE NODE TABLE Probe(name STRING PRIMARY KEY)','CREATE REL TABLE KNOWS(FROM Person TO Person,weight INT64)']+FIXTURE:self.conn.execute(q)
 def query(self,test):
  try:
   if test.get('mutates'):self.conn.execute('BEGIN TRANSACTION')
   r=self.conn.execute(test['query']);rows=[]
   while r.has_next():rows.append(r.get_next())
   return {'rows':rows}
  except Exception as e:return {'error':str(e)}
  finally:
   if test.get('mutates'):
    try:self.conn.execute('ROLLBACK')
    except Exception:pass
 def close(self):self.conn.close();self.db.close()

class Neo4j:
 languages={'cypher'}
 def __init__(self,puppy=False):
  from neo4j import GraphDatabase
  self.puppy=puppy
  self.driver=GraphDatabase.driver(os.environ.get('PUPPY_BOLT' if puppy else 'NEO4J_BOLT','bolt://127.0.0.1:'+('17688' if puppy else '17687')),auth=('puppygraph' if puppy else 'neo4j','conformance-local-only'),connection_timeout=15)
  self.version={'version':'1.11.1' if puppy else '2026.09.0','edition':'Developer (no enterprise license)' if puppy else 'Community','language':'openCypher 9' if puppy else 'Cypher 5 explicitly selected'}
  if not puppy:
   with self.driver.session() as s:
    s.run('MATCH (n) DETACH DELETE n').consume()
    for q in FIXTURE:s.run(q).consume()
  else:
   deadline=time.monotonic()+90
   while True:
    try:
     with self.driver.session() as session:
      counts=session.run('MATCH (n:Person) RETURN count(n)').values()
      edges=session.run('MATCH ()-[r:KNOWS]->() RETURN count(r)').values()
      if counts!=[[4]] or edges!=[[4]]:raise RuntimeError('PuppyGraph fixture does not match four nodes and four edges')
     break
    except Exception:
     if time.monotonic()>deadline:raise
     time.sleep(2)
   from gremlin_python.driver.client import Client
   self.gremlin=Client(os.environ.get('PUPPY_GREMLIN','ws://127.0.0.1:18182/gremlin'),'g',username='puppygraph',password='conformance-local-only')
 def query(self,test):
  try:
   if test['language']=='gremlin':
    results=self.gremlin.submit(test['query']).all().result(timeout=20)
    return {'rows':[[x] for x in results]}
   with self.driver.session() as s:
    if self.puppy:return {'rows':s.run(test['query']).values()}
    with s.begin_transaction(timeout=15) as tx:
     r=tx.run('CYPHER 5 '+test['query']).values();tx.rollback();return {'rows':r}
  except Exception as e:return {'error':str(e)}
 def close(self):
  self.driver.close()
  if self.puppy:self.gremlin.close()
class Puppygraph(Neo4j):
 languages={'cypher','gremlin'}
 def __init__(self):super().__init__(puppy=True)
class Sqlg:
 main_class='Conformance'
 languages={'gremlin'}
 def __init__(self):
  d=ROOT/'adapters/sqlg';cp=str(d/'target/classes')+os.pathsep+(d/'classpath.txt').read_text().strip()
  java=os.environ.get('CONFORMANCE_JAVA','java')
  self.proc=Process([java,'-Dorg.slf4j.simpleLogger.defaultLogLevel=error','-cp',cp,self.main_class],ROOT/'sqlg.log')
  self.proc.read(timeout=90)
  self.version={'version':'3.1.6','edition':'MIT open source','backend':'PostgreSQL 15','tinkerpop':'3.7.4'}
 def query(self,test):return self.proc.query(test)
 def close(self):self.proc.close()

class Reference(Sqlg):
 main_class='Reference'
 def __init__(self):
  super().__init__();self.version={'version':'3.7.4','edition':'TinkerGraph reference'}

ENGINES={'reference':Reference,'crabgraph':Crabgraph,'ladybug':Ladybug,'neo4j':Neo4j,'puppygraph':Puppygraph,'sqlg':Sqlg}
def main():
 p=argparse.ArgumentParser();p.add_argument('--engine',choices=ENGINES,required=True);p.add_argument('--output',type=Path);p.add_argument('--filter',default='');p.add_argument('--repetitions',type=int,default=3);args=p.parse_args()
 if args.repetitions<1:p.error('--repetitions must be positive')
 if args.filter and not args.output:p.error('Filtered runs require --output')
 started=datetime.datetime.now(datetime.timezone.utc).isoformat();engine=ENGINES[args.engine]()
 selected=[t for t in TESTS if t['language'] in engine.languages and args.filter in t['id']]
 results=[]
 try:
  for test in selected:
   trials=[]
   for iteration in range(args.repetitions):
    t=time.monotonic()
    try:
     actual=engine.query(test)
     status='query-error' if 'error' in actual else 'pass' if equal_rows(actual['rows'],test['expected'],test['ordered']) else 'mismatch'
    except TimeoutError as ex:actual={'error':str(ex)};status='timeout'
    except Exception as ex:actual={'error':str(ex)};status='harness-error'
    trials.append({'status':status,'elapsed_ms':round((time.monotonic()-t)*1000,2),**actual})
    if status in {'timeout','harness-error'}:break
   first=trials[0]
   unstable=any(x['status']!=first['status'] or ('rows' in x and not equal_rows(x['rows'],first.get('rows',[]),test['ordered'])) for x in trials[1:])
   status='mismatch' if unstable else first['status']
   samples=[x['elapsed_ms'] for x in trials]
   result={'id':test['id'],'probe_sha256':hashlib.sha256(json.dumps(test,sort_keys=True).encode()).hexdigest(),**first,'status':status,'unstable':unstable,'timing':{'samples_ms':samples,'median_ms':round(statistics.median(samples),2),'min_ms':min(samples),'max_ms':max(samples),'scope':'client wall time; three consecutive executions by default; first included; no warmup'},'trials':trials}
   results.append(result)
   print(args.engine,test['id'],status,flush=True)
 finally:engine.close()
 content={'schema_version':2,'execution_environment':{'system':platform.system(),'release':platform.release(),'architecture':platform.machine(),'logical_cpus':os.cpu_count(),'python':platform.python_version()},'engine':args.engine,'build':engine.version,'started_at':started,'finished_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'suite_sha256':hashlib.sha256((ROOT/'probes.json').read_bytes()).hexdigest(),'results':results}
 out=args.output or ROOT/'results'/f'{args.engine}.json';
 if args.filter and not args.output:raise ValueError('Filtered runs require --output to preserve the complete snapshot')
 out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(content,indent=2,default=str)+'\n')
 print(f'{sum(r["status"]=="pass" for r in results)}/{len(results)} probes passed; saved {out}')
if __name__=='__main__':main()
