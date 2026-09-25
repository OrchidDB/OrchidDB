#!/usr/bin/env python3
"""Disposable fixture adapters; no expectations or test cases live here."""
import base64,hashlib,json,os,select,subprocess,sys,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class Rust:
 def __init__(self):
  self.p=subprocess.Popen([os.environ.get('CONFORMANCE_ORCHIDDB_BINARY',str(ROOT/'target/debug/upstream'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=sys.stderr,text=True,bufsize=1)
 def send(self,req):
  if self.p.poll() is not None:
   raise RuntimeError('The single OrchidDB instance exited; this run cannot restart it')
  self.p.stdin.write(json.dumps(req)+'\n');self.p.stdin.flush()
  if not select.select([self.p.stdout],[],[],40)[0]:self.p.kill();raise TimeoutError('OrchidDB adapter deadline')
  line=self.p.stdout.readline()
  if not line:raise RuntimeError('OrchidDB adapter exited')
  return json.loads(line)
 def close(self):self.p.terminate();self.p.wait(timeout=5)
def property_type(v,declared=None):
 if declared=="Integer":return ("INTEGER","Int")
 if declared=="Float":raise ValueError("Fixture adapter cannot preserve 32-bit Float properties")
 if isinstance(v,bool):return ('BOOLEAN','Boolean')
 if isinstance(v,int):return ('BIGINT','Long')
 if isinstance(v,float):return ('DOUBLE PRECISION','Double')
 if isinstance(v,str):return ('TEXT','String')
 raise ValueError('Fixture adapter cannot preserve this property type: '+type(v).__name__)
class PuppyFixture:
 def __init__(self):
  import psycopg
  self.pg=psycopg.connect('postgresql://conformance:conformance-local-only@127.0.0.1:15433/conformance',autocommit=True,prepare_threshold=None)
  self.last=None;self.shape=None
 def setup(self,req):
  from psycopg import sql
  digest=hashlib.sha256(json.dumps(req,sort_keys=True).encode()).hexdigest()
  if self.last==digest:return {'ok':True,'cached':True}
  nodes=req['nodes'];edges=req['edges'];labels={n['id']:n['label'] for n in nodes}
  groups={}
  for n in nodes:groups.setdefault(('node',n['label']),[]).append(n)
  for r in edges:groups.setdefault(('edge',r['label'],labels[r['src']],labels[r['dst']]),[]).append(r)
  if not groups:groups[('node','EmptyFixture')]=[]
  specs=[]
  for key,items in sorted(groups.items()):
   props={}
   for item in items:
    for k,v in item['properties'].items():
     if v is None:continue
     t=property_type(v,item.get("property_types",{}).get(k))
     if k in props and props[k]!=t:
      if {props[k][1],t[1]}<={'Long','Double'}:t=('DOUBLE PRECISION','Double')
      else:raise ValueError('Fixture adapter cannot preserve heterogeneous property '+k)
     props[k]=t
   specs.append((key,items,props,'f_'+hashlib.sha256(repr((key,props)).encode()).hexdigest()[:12]))
  shape=[(k,props,table) for k,_,props,table in specs]
  self.pg.execute('CREATE SCHEMA IF NOT EXISTS upstream_fixture')
  if shape!=self.shape:
   # Only our disposable fixture schema is replaced.
   self.pg.execute('DROP SCHEMA upstream_fixture CASCADE');self.pg.execute('CREATE SCHEMA upstream_fixture')
  for key,items,props,table in specs:
   fields={'fixture_id':('BIGINT','Long')}
   if key[0]=='edge':fields.update(fixture_src=('BIGINT','Long'),fixture_dst=('BIGINT','Long'))
   if any(k in fields for k in props):raise ValueError('Fixture property collides with adapter storage key')
   fields.update(props)
   if shape!=self.shape:
    self.pg.execute(sql.SQL('CREATE TABLE upstream_fixture.{} ({})').format(sql.Identifier(table),sql.SQL(',').join(sql.SQL('{} {}').format(sql.Identifier(k),sql.SQL(t[0])) for k,t in fields.items())))
   else:self.pg.execute(sql.SQL('TRUNCATE upstream_fixture.{}').format(sql.Identifier(table)))
   query=sql.SQL('INSERT INTO upstream_fixture.{} ({}) VALUES ({})').format(sql.Identifier(table),sql.SQL(',').join(map(sql.Identifier,fields)),sql.SQL(',').join(sql.Placeholder() for _ in fields))
   with self.pg.cursor() as cursor:
    values=[]
    for item in items:
     value={'fixture_id':int(item['id']),**item['properties']}
     if key[0]=='edge':value.update(fixture_src=int(item['src']),fixture_dst=int(item['dst']))
     values.append([value.get(k) for k in fields])
    if values:cursor.executemany(query,values)
  if shape!=self.shape:
   schema={'catalog':[{'name':'upstream_fixture','type':'postgresql','jdbc':{'jdbcUri':os.environ.get('PUPPY_JDBC','jdbc:postgresql://host.docker.internal:15433/conformance'),'driverClass':'org.postgresql.Driver','username':'conformance','password':'conformance-local-only'}}],'node':[],'edge':[]}
   for key,items,props,table in specs:
    fields=['fixture_id',*(['fixture_src','fixture_dst'] if key[0]=='edge' else []),*props]
    entry={'label':key[1],'id':[{'name':'fixture_id','type':'Long'}],'attribute':[{'name':k,'type':v[1]} for k,v in props.items()],'dataSourceGroup':{'externalDataSource':{'enabled':True,'catalog':'upstream_fixture','schema':'upstream_fixture','table':table,'mappedField':[{'sourceFieldName':f,'targetFieldName':f} for f in fields]}}}
    if key[0]=='edge':entry.update(fromNodeLabel=key[2],toNodeLabel=key[3],fromKey=[{'name':'fixture_src','type':'Long'}],toKey=[{'name':'fixture_dst','type':'Long'}])
    schema[key[0]].append(entry)
   request=urllib.request.Request('http://127.0.0.1:18081/ui-api/uploadSchema',data=json.dumps(schema).encode(),headers={'Content-Type':'application/json','Authorization':'Basic '+base64.b64encode(b'puppygraph:conformance-local-only').decode()})
   try:
    with urllib.request.urlopen(request,timeout=60) as r:reply=json.load(r)
   except urllib.error.HTTPError as ex:
    self.shape=None;self.last=None
    raise RuntimeError(ex.read().decode()) from ex
   if not reply.get('ok'):raise RuntimeError(str(reply))
   self.shape=shape
  self.specs=specs
  self.last=digest;return {'ok':True}
 def snapshot(self):
  from psycopg import sql
  snap={k:set() for k in ['nodes','relationships','labels','node_properties','edge_properties']}
  for key,_,props,table in self.specs:
   rows=self.pg.execute(sql.SQL('SELECT * FROM upstream_fixture.{}').format(sql.Identifier(table))).fetchall()
   cols=['fixture_id',*(['fixture_src','fixture_dst'] if key[0]=='edge' else []),*props]
   for row in rows:
    data=dict(zip(cols,row));identity=[list(key),data['fixture_id']];kind='nodes' if key[0]=='node' else 'relationships';snap[kind].add(json.dumps(identity,sort_keys=True))
    if key[0]=='node':snap['labels'].add(json.dumps([key[1]]))
    for prop in props:
     if data[prop] is not None:snap['node_properties' if key[0]=='node' else 'edge_properties'].add(json.dumps([identity,prop,data[prop]],sort_keys=True))
  return snap
 def close(self):self.pg.close()
def main():
 backend=sys.argv[1];adapter=Rust() if backend=='orchiddb' else PuppyFixture()
 try:
  for line in sys.stdin:
   try:
    req=json.loads(line);result=adapter.send(req) if backend=='orchiddb' else adapter.setup(req)
   except Exception as e:result={'error':str(e),'adapter_error':True,'timeout':isinstance(e,TimeoutError)}
   print(json.dumps(result),flush=True)
 finally:adapter.close()
if __name__=='__main__':main()
