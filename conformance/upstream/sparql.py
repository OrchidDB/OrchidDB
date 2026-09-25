"""W3C manifest adapter. Uses original data/query/result files, preserving RDF terms."""
import io,json,re,time,os
from pathlib import Path
from urllib.parse import urlsplit,unquote
import rdflib
from rdflib import Graph,URIRef,BNode,Literal
from rdflib.query import Result
from rdflib.compare import isomorphic
from fetch import CACHE
from run import Process,ROOT,REPO,crabgraph_binary
rdflib.NORMALIZE_LITERALS=False
XSD='http://www.w3.org/2001/XMLSchema#'
RS=rdflib.Namespace('http://www.w3.org/2001/sw/DataAccess/tests/result-set#')
def relocated_graph_name(base,filename,name):
 """Rebase catalog file-derived graph names when the upstream checkout moves.

 Only a file URI matching the complete relative fixture path is eligible.
 Keep its lexical checkout path, matching RDFLib's base for fixture/result files.
 Resolving symlinks here would give a different graph IRI from those files.
 """
 if not name or not filename:return name
 uri=urlsplit(name);relative=Path(filename)
 if uri.scheme!='file' or uri.netloc not in ('','localhost') or uri.query or uri.fragment:return name
 if relative.is_absolute() or '..' in relative.parts:return name
 if not unquote(uri.path).endswith('/'+relative.as_posix()):return name
 current=Path(base)/relative
 return current.absolute().as_uri() if current.is_file() else name
def term(v):
 if v is None:return None
 if isinstance(v,URIRef):return {'type':'uri','value':str(v)}
 if isinstance(v,BNode):return {'type':'bnode','value':str(v)}
 return {'type':'literal','value':str(v),'datatype':str(v.datatype or (rdflib.RDF.langString if v.language else rdflib.XSD.string)),'lang':v.language.lower() if v.language else None}
def fromterm(t):
 if t['type']=='uri':return URIRef(t['value'])
 if t['type']=='bnode':return BNode(t['value'])
 return Literal(t['value'],lang=t.get('lang'),datatype=None if t.get('lang') else t.get('datatype'),normalize=False)
def matchterm(a,b,mapping):
 if a is None or b is None:return a is b
 if a.get('type')=='bnode' and b.get('type')=='bnode':
  left=a['value'];right=b['value']
  if left in mapping:return mapping[left]==right
  if right in mapping.values():return False
  mapping[left]=right;return True
 return a==b
def rows_equal(actual,expected,ordered):
 if len(actual)!=len(expected):return False
 def visit(index,remaining,mapping):
  if index==len(expected):return True
  candidates=[index] if ordered else remaining
  for i in candidates:
   if i not in remaining:continue
   a=actual[i];b=expected[index];m=dict(mapping)
   if len(a)==len(b) and all(matchterm(x,y,m) for x,y in zip(a,b)):
    if visit(index+1,remaining-{i},m):return True
  return False
 return visit(0,set(range(len(actual))),{})
def expected(path):
 ext=path.suffix.lower()
 if ext in ['.srx','.srj']:
  with path.open('rb') as f:r=Result.parse(f,format='xml' if ext=='.srx' else 'json')
  if r.type=='ASK':return {'boolean':bool(r.askAnswer)}
  return {'variables':[str(v) for v in r.vars],'rows':[[term(row.get(v)) for v in r.vars] for row in r.bindings]}
 graph=Graph().parse(path,format='turtle' if ext=='.ttl' else None)
 roots=list(graph.subjects(rdflib.RDF.type,RS.ResultSet))
 if roots:
  root=roots[0];boolean=graph.value(root,RS.boolean)
  if boolean is not None:return {'boolean':bool(boolean.toPython())}
  variables=[str(x) for x in graph.objects(root,RS.resultVariable)];solutions=list(graph.objects(root,RS.solution));solutions.sort(key=lambda s:int(graph.value(s,RS.index) or 0));rows=[]
  for s in solutions:
   values={str(graph.value(b,RS.variable)):term(graph.value(b,RS.value)) for b in graph.objects(s,RS.binding)};rows.append([values.get(v) for v in variables])
  return {'variables':variables,'rows':rows}
 return {'graph':[[term(s),term(p),term(o)] for s,p,o in graph]}
class Sparql:
 def __init__(self,engine='crabgraph'):self.process=None;self.engine=engine
 def send(self,request,timeout=25):
  if self.process is None or self.process.p.poll() is not None:
   if self.engine=='jena':
    root=ROOT/'adapters/jena'
    classpath=str(root/'target/classes')+os.pathsep+(root/'target/classpath.txt').read_text().strip()
    command=[os.environ.get('CONFORMANCE_JAVA','java'),'-Dorg.slf4j.simpleLogger.defaultLogLevel=error','-cp',classpath,'JenaAdapter']
   else:command=[str(crabgraph_binary())]
   self.process=Process(command,ROOT/f'upstream-{self.engine}-rdf.log')
  return self.process.send(request,timeout=timeout)
 def run(self,case):
  types=case['types'];base=CACHE/'rdf'
  if any('Update' in t for t in types):
   if not any('Syntax' in t for t in types):
    if self.engine=='crabgraph':return {'status':'unsupported','reason':'Crabgraph RDF adapter exposes read queries; SPARQL Update interface unavailable'}
    from sparql_updates import run_update
    return run_update(self,case)
  if any('Protocol' in t or 'ServiceDescription' in t or 'CSV' in t for t in types):return {'status':'not-applicable','reason':'This case tests an HTTP protocol or wire serializer; the compared Crabgraph API is embedded'}
  if '/entailment/' in case['path']:return {'status':'skipped','reason':'Upstream entailment profile requires a separately configured reasoning dataset'}
  if case.get('result_file','') and case['result_file'].endswith(('.tsv','.csv')):return {'status':'not-applicable','reason':'Upstream case asserts TSV/CSV wire serialization; embedded adapter exposes RDF terms'}
  path=base/case['query_file'];query=path.read_text()
  if re.search(r'\bSERVICE\b',query,re.I):return {'status':'skipped','reason':'Upstream federated SERVICE fixture endpoint is not installed locally','query':query}
  negative=any('Negative' in t for t in types);syntax=any('Syntax' in t for t in types)
  if syntax:
   before=time.monotonic();actual=self.send({'op':'sparql-syntax','query':query,'base':path.absolute().as_uri(),'update':any('Update' in t for t in types)})
   passed=('error' in actual)==negative
   return {'status':'pass' if passed else 'fail','query':query,'expected':{'parses':not negative},'actual':actual,'query_ms':round((time.monotonic()-before)*1000,3),'assertion':'W3C positive/negative syntax; no query or update evaluation'}
  quads=[]
  for filename,name in [(f,None) for f in case['data']]+[(r['file'],r['name']) for r in case['named']]:
   if not filename:return {'status':'adapter-error','reason':'Manifest graph fixture has no file'}
   name=relocated_graph_name(base,filename,name)
   data=Graph().parse(base/filename)
   for triple in data:
    row=[name]
    for v in triple:
     t=term(v);row.extend([t['value'],{'uri':'IRI','bnode':'BLANK','literal':'LITERAL'}[t['type']],t.get('datatype'),t.get('lang')])
    quads.append(row)
  if not case['result_file']:return {'status':'adapter-error','reason':'No supported result artifact in manifest'}
  want=expected(base/case['result_file'])
  effective='BASE <'+path.absolute().as_uri()+'>\n'+query
  before=time.monotonic();actual=self.send({'op':'rdf','query':effective,'quads':quads},timeout=25);duration=round((time.monotonic()-before)*1000,3)
  if 'error' in actual:passed=False
  elif 'boolean' in want:passed=actual.get('boolean')==want['boolean']
  elif 'graph' in want:
   if 'graph' not in actual:passed=False
   else:
    a=Graph();b=Graph()
    for triple in actual['graph']:a.add(tuple(fromterm(t) for t in triple))
    for triple in want['graph']:b.add(tuple(fromterm(t) for t in triple))
    passed=isomorphic(a,b)
  else:
   if set(actual.get('variables',[]))!=set(want['variables']):passed=False
   else:
    rows=[[r[actual['variables'].index(v)] for v in want['variables']] for r in actual['rows']]
    passed=rows_equal(rows,want['rows'],bool(re.search(r'\bORDER\s+BY\b',query,re.I)))
  return {'status':'pass' if passed else 'fail','query':query,'effective_base':path.absolute().as_uri(),'fixture_quads':len(quads),'expected':want,'actual':actual,'query_ms':duration,'assertion':'W3C expected result; RDF term identity and global blank-node bijection / graph isomorphism'}
 def close(self):
  if self.process:
   if self.engine=='jena' and self.process.p.poll() is None:
    import subprocess
    self.process.p.stdin.close()
    try:self.process.p.wait(timeout=5)
    except subprocess.TimeoutExpired:pass
   self.process.close()
