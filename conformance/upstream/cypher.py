"""Execute original TCK steps, with independent parsing of upstream value notation."""
import ast,json,math,re,time,logging
logging.getLogger("neo4j.notifications").setLevel(logging.ERROR)
from collections import Counter
from lark import Lark,Transformer
from neo4j import GraphDatabase
from neo4j.graph import Node,Relationship,Path
from fetch import CACHE
from bridge import PuppyFixture
from run import Process,ROOT,REPO,crabgraph_binary
GRAMMAR=r'''
?start: value
?value: STRING -> string
 | SIGNED_NUMBER -> number
 | "true" -> true
 | "false" -> false
 | "null" -> null
 | "NaN" -> nan
 | "Inf" -> inf
 | "-Inf" -> ninf
 | list | map | node | relationship | path
list: "[" [value ("," value)*] "]"
map: "{" [pair ("," pair)*] "}"
pair: key ":" value
?key: NAME | STRING -> string | BACKTICK -> tick
node: "(" label* [map] ")"
label: ":" key
relationship: "[" label [map] "]"
path: "<" node path_segment* ">"
path_segment: "-" relationship "->" node -> forward
 | "<-" relationship "-" node -> backward
STRING: /'(?:\\.|[^'\\])*'/ | /"(?:\\.|[^"\\])*"/
BACKTICK: /`(?:``|[^`])*`/
NAME: /[A-Za-z_][A-Za-z_0-9]*/
%import common.SIGNED_NUMBER
%import common.WS
%ignore WS
'''
class Values(Transformer):
 def string(self,x):
  # TCK strings may contain literal newlines, which Python source literals reject.
  text=str(x[0])[1:-1]
  escapes={'b':'\b','f':'\f','n':'\n','r':'\r','t':'\t',"'":"'",'"':'"','\\':'\\'}
  def decode(m):
   token=m.group(1)
   if token.startswith(('u','U')):return chr(int(token[1:],16))
   if token not in escapes:raise ValueError('Unknown TCK string escape: '+token)
   return escapes[token]
  return re.sub(r'\\(u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8}|.)',decode,text)
 def tick(self,x):return str(x[0])[1:-1].replace('``','`')
 def number(self,x):
  s=str(x[0]);return float(s) if any(c in s.lower() for c in '.e') else int(s)
 def true(self,x):return True
 def false(self,x):return False
 def null(self,x):return None
 def nan(self,x):return {'$float':'NaN'}
 def inf(self,x):return {'$float':'Infinity'}
 def ninf(self,x):return {'$float':'-Infinity'}
 def list(self,x):return list(x)
 def pair(self,x):return (str(x[0]),x[1])
 def map(self,x):return dict(x)
 def label(self,x):return str(x[0])
 def node(self,x):return {'$node':{'labels':sorted(v for v in x if isinstance(v,str)),'properties':next((v for v in x if isinstance(v,dict)),{})}}
 def forward(self,x):return {'direction':'forward','relationship':x[0],'node':x[1]}
 def backward(self,x):return {'direction':'backward','relationship':x[0],'node':x[1]}
 def path(self,x):return {'$path':{'start':x[0],'segments':x[1:]}}
 def relationship(self,x):return {'$relationship':{'type':x[0],'properties':x[1] if len(x)>1 else {}}}
PARSE=Lark(GRAMMAR,parser='lalr',transformer=Values(),maybe_placeholders=False)
def value(text):return PARSE.parse(text)
def normalize(v):
 if isinstance(v,Node):return {'$node':{'labels':sorted(v.labels),'properties':{k:normalize(x) for k,x in dict(v).items() if x is not None}}}
 if isinstance(v,Relationship):return {'$relationship':{'type':v.type,'properties':{k:normalize(x) for k,x in dict(v).items() if x is not None}}}
 if isinstance(v,Path):
  return {'$path':{'start':normalize(v.start_node),'segments':[{'direction':'forward' if rel.start_node.element_id==node.element_id else 'backward','relationship':normalize(rel),'node':normalize(v.nodes[i+1])} for i,(node,rel) in enumerate(zip(v.nodes,v.relationships))]}}
 if isinstance(v,(tuple,list)):return [normalize(x) for x in v]
 if isinstance(v,dict):return {k:normalize(x) for k,x in v.items()}
 if isinstance(v,float) and not math.isfinite(v):return {'$float':'NaN' if math.isnan(v) else 'Infinity' if v>0 else '-Infinity'}
 if v is None or isinstance(v,(str,int,float,bool)):return v
 raise ValueError('Unsupported driver result type '+type(v).__name__)
def classified_error_matches(assertion,classification):
 """Match original TCK constraints; unknown diagnostics never establish a pass."""
 expected=re.fullmatch(r'a (\w+) should be raised at (compile time|runtime|any time): (\w+|\*)',assertion)
 if not expected:raise ValueError('Unmapped TCK error assertion: '+assertion)
 if not isinstance(classification,dict) or not all(classification.get(k) for k in ('type','detail','phase')):return None
 kind,phase,detail=expected.groups()
 return classification['type']==kind and (phase=='any time' or classification['phase']==phase) and (detail=='*' or classification['detail']==detail)
def native_value(v):
 kind=v['type'];x=v.get('value')
 if kind=='null':return None
 if kind=='internal_id':return {'$internal_id':{'table':x['table'],'offset':x['offset']}}
 if kind in ('boolean','string'):return x
 if kind in ('byte','uint8','short','uint16','int','uint32','long','uint64','uint128','bigint'):return int(x)
 if kind in ('float','double'):return normalize(float(x))
 if kind in ('list','set'):return [native_value(i) for i in x]
 if kind=='map':return {native_value(k):native_value(item) for k,item in x}
 if kind=='vertex':return {'$node':{'labels':sorted(v.get('labels',[v['label']])),'properties':{k:native_value(item) for k,item in v['properties'].items()}}}
 if kind=='edge':return {'$relationship':{'type':v['label'],'properties':{k:native_value(item) for k,item in v['properties'].items()}}}
 if kind=='path':
  if not x or len(x)%2!=1 or any(n['type']!='vertex' for n in x[::2]) or any(e['type']!='edge' for e in x[1::2]):raise ValueError('Native path must contain alternating vertices and relationships')
  segments=[]
  for i in range(1,len(x),2):
   edge=x[i];node=x[i-1]
   forward=edge['outVLabel']==node['label'] and edge['outV']==node['id']
   segments.append({'direction':'forward' if forward else 'backward','relationship':native_value(edge),'node':native_value(x[i+1])})
  return {'$path':{'start':native_value(x[0]),'segments':segments}}
 raise ValueError('Unmapped native Cypher result type '+kind)
def equal(a,b,unordered_lists=False):
 if a is None or b is None:return a is b
 if isinstance(a,bool) or isinstance(b,bool):return type(a)==type(b) and a==b
 if isinstance(a,int) and isinstance(b,int):return a==b
 if isinstance(a,(int,float)) and isinstance(b,(int,float)):return math.isclose(a,b,rel_tol=1e-9,abs_tol=1e-9)
 if isinstance(a,list) and isinstance(b,list):return rows_equal(a,b,not unordered_lists,unordered_lists)
 if isinstance(a,dict) and isinstance(b,dict):return a.keys()==b.keys() and all(equal(a[k],b[k],unordered_lists) for k in a)
 return type(a)==type(b) and a==b
def rows_equal(actual,expected,ordered,unordered_lists=False):
 if len(actual)!=len(expected):return False
 if ordered:return all(equal(a,b,unordered_lists) for a,b in zip(actual,expected))
 remaining=list(actual)
 for row in expected:
  hit=next((i for i,x in enumerate(remaining) if equal(x,row,unordered_lists)),None)
  if hit is None:return False
  remaining.pop(hit)
 return True
class Cypher:
 def __init__(self,engine):
  self.engine=engine;self.rust=None;self.driver=None;self.fixture=None;self.seed=None
  if engine=='puppygraph':
   self.driver=GraphDatabase.driver('bolt://127.0.0.1:17688',auth=('puppygraph','conformance-local-only'))
   # Neo4j materializes upstream GIVEN fixtures only. Expected results always come from TCK.
   self.seed=GraphDatabase.driver('bolt://127.0.0.1:17687',auth=('neo4j','conformance-local-only'))
   self.fixture=PuppyFixture()
 def query(self,q,params=None):
  if self.engine=='crabgraph':
   result=self.rust.send({'op':'cypher','query':q,'params':params or {}},timeout=20)
   if result.get('native_rows') is not None:
    result['rows']=[[native_value(v) for v in row] for row in result['native_rows']]
    if result.get('native_columns') is not None:result['columns']=result['native_columns']
   return result
  from neo4j.exceptions import ServiceUnavailable,SessionExpired
  try:
   with self.driver.session() as s:
    r=s.run(q,params or {});columns=list(r.keys());rows=[[normalize(v) for v in row] for row in r.values()]
    return {'columns':columns,'rows':rows}
  except (ServiceUnavailable,SessionExpired,OSError,TimeoutError):raise
  except ValueError as e:return {'adapter_error':str(e)}
  except Exception as e:return {'error':str(e),'code':getattr(e,'code',None)}
 def snapshot(self):
  if self.engine=="puppygraph":return self.fixture.snapshot()
  queries={'nodes':'MATCH (n) RETURN id(n)','relationships':'MATCH ()-[r]->() RETURN id(r)','labels':'MATCH (n) UNWIND labels(n) AS l RETURN DISTINCT l','node_properties':'MATCH (n) UNWIND keys(n) AS k RETURN id(n),k,n[k]','edge_properties':'MATCH ()-[r]->() UNWIND keys(r) AS k RETURN id(r),k,r[k]'}
  result={}
  for k,q in queries.items():
   v=self.query(q)
   if 'error' in v or 'adapter_error' in v:raise ValueError('Cannot observe TCK side effects: '+str(v))
   result[k]=set(json.dumps(r,sort_keys=True) for r in v['rows'])
  return result
 def run(self,case):
  steps=case['steps'];setup=[];params={};original=next(s['doc'] for s in steps if s['text']=='executing query:')
  if any(s['text'].startswith('there exists a procedure') for s in steps):return {'status':'skipped','reason':'Upstream GIVEN procedure registration requires a provider-specific procedure adapter'}
  for s in steps:
   if s['text']=='having executed:':setup.append(s['doc'])
   if s['text']=='parameters are:':params={r[0]:value(r[1]) for r in s['table']}
   if re.fullmatch(r'the binary-tree-[12] graph',s['text']):
    name=s['text'].split()[1];setup.append((CACHE/'opencypher/tck/graphs'/name/(name+'.cypher')).read_text())
  if self.engine=='crabgraph':
   if self.rust is None or self.rust.p.poll() is not None:self.rust=Process([str(crabgraph_binary())],ROOT/'upstream-crabgraph-cypher.log')
   self.rust.send({'op':'reset'})
   for q in setup:
    result=self.query(q,params)
    if 'error' in result:return {'status':'fail','stage':'fixture-query','query':q,'actual':result,'reason':'Engine rejected an upstream GIVEN query'}
  else:
   with self.seed.session() as session:
    session.run('MATCH(n) DETACH DELETE n').consume()
    for q in setup:session.run('CYPHER 5 '+q,params).consume()
    nodes=[]
    for node in session.run('MATCH(n) RETURN id(n),labels(n),properties(n)').values():
     if len(node[1])!=1:return {'status':'skipped','reason':'PuppyGraph fixture mapper cannot preserve unlabelled or multi-label upstream nodes','query':original}
     nodes.append({'id':node[0],'label':node[1][0],'properties':node[2]})
    edges=[{'id':e[0],'label':e[1],'src':e[2],'dst':e[3],'properties':e[4]} for e in session.run('MATCH(a)-[r]->(b) RETURN id(r),type(r),id(a),id(b),properties(r)').values()]
   try:self.fixture.setup({'nodes':nodes,'edges':edges})
   except Exception as e:return {'status':'adapter-error','stage':'fixture-mapping','reason':str(e)}
  assertions=[];actual=None;before=None;query_ms=[];wanted=None
  try:
   for s in steps:
    text=s['text']
    if text in ['any graph','an empty graph','having executed:','parameters are:'] or re.fullmatch(r'the binary-tree-[12] graph',text):continue
    if text in ['executing query:','executing control query:']:
     if text=='executing query:':before=self.snapshot()
     start=time.monotonic();actual=self.query(s['doc'],params);query_ms.append(round((time.monotonic()-start)*1000,3));continue
    if 'should be raised' in text:
     if 'error' not in actual:return {'status':'fail','query':original,'expected_error':text,'actual':actual,'query_ms':query_ms}
     matched=classified_error_matches(text,actual.get('classification'))
     if matched is not None:return {'status':'pass' if matched else 'fail','query':original,'expected_error':text,'actual':actual,'query_ms':query_ms,'assertions':[text] if matched else []}
     return {'status':'adapter-error','reason':'Error observed, but TCK error type/detail/phase cannot be authoritatively classified by this adapter','query':original,'expected_error':text,'actual':actual,'query_ms':query_ms}
    if text.startswith('the result should be'):
     if actual is None:raise ValueError('Result assertion has no query')
     if 'adapter_error' in actual:raise ValueError(actual['adapter_error'])
     if 'error' in actual:return {'status':'fail','query':original,'actual':actual,'expected':s,'query_ms':query_ms}
     table=s.get('table',[]);columns=table[0] if table else [];wanted=[[value(x) for x in r] for r in table[1:]] if table else []
     if table and set(columns)!=set(actual['columns']):return {'status':'fail','query':original,'expected':{'columns':columns,'rows':wanted},'actual':actual,'query_ms':query_ms}
     got=[[row[actual['columns'].index(k)] for k in columns] for row in actual['rows']] if table else actual['rows']
     if not rows_equal(got,wanted,text=='the result should be, in order:', 'ignoring element order' in text):return {'status':'fail','query':original,'expected':{'columns':columns,'rows':wanted},'actual':actual,'query_ms':query_ms}
     assertions.append(text);continue
    if text in ['no side effects','the side effects should be:']:
     after=self.snapshot();effect={}
     for kind in ['nodes','relationships','labels']:
      effect['+'+kind]=len(after[kind]-before[kind]);effect['-'+kind]=len(before[kind]-after[kind])
     for sign,left,right in [('+',after,before),('-',before,after)]:effect[sign+'properties']=sum(len(left[k]-right[k]) for k in ['node_properties','edge_properties'])
     wanted_effect={k:0 for k in effect}
     if 'table' in s:
      for key,amount in s['table']:wanted_effect[key]=int(amount)
     if effect!=wanted_effect:return {'status':'fail','stage':'side-effects','query':original,'expected':wanted_effect,'actual':effect,'query_ms':query_ms}
     assertions.append(text);continue
    raise ValueError('Unmapped TCK step: '+text)
  except Exception as e:return {'status':'adapter-error','reason':str(e),'query':original,'actual':actual,'query_ms':query_ms}
  return {'status':'pass','query':original,'expected':wanted,'actual':actual,'assertions':assertions,'query_ms':query_ms}
 def close(self):
  if self.rust:self.rust.close()
  if self.driver:self.driver.close()
  if self.seed:self.seed.close()
  if self.fixture:self.fixture.close()
