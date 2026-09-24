#!/usr/bin/env python3
"""Compile original Gherkin with Cucumber and enumerate original W3C manifests."""
import hashlib,json
from pathlib import Path
from urllib.parse import unquote,urlparse
from gherkin.parser import Parser
from gherkin.pickles.compiler import Compiler
from rdflib import Graph,Namespace,RDF,RDFS,BNode
from fetch import ROOT,CACHE
MF=Namespace('http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#')
QT=Namespace('http://www.w3.org/2001/sw/DataAccess/tests/test-query#')
SOURCES=json.loads((ROOT/'sources.json').read_text())
def local(uri):return Path(unquote(urlparse(str(uri)).path))
def gherkin(name):
 source=SOURCES[name];base=CACHE/name
 for path in sorted((base/source['directory']).rglob('*.feature')):
  text=path.read_text();doc=Parser().parse(text);doc['uri']=str(path.relative_to(base))
  nodes={}
  def walk(x):
   if isinstance(x,dict):
    if 'id' in x:nodes[x['id']]=x
    for v in x.values():walk(v)
   elif isinstance(x,list):
    for v in x:walk(v)
  walk(doc)
  for pickle in Compiler().compile(doc):
   line=nodes[pickle['astNodeIds'][-1]]['location']['line'];relative=str(path.relative_to(base))
   steps=[]
   for step in pickle['steps']:
    s={'text':step['text']};arg=step.get('argument',{})
    if 'docString' in arg:s['doc']=arg['docString']['content']
    if 'dataTable' in arg:s['table']=[[c['value'] for c in r['cells']] for r in arg['dataTable']['rows']]
    steps.append(s)
   yield {'id':f'{name}:{relative}:{line}','suite':name,'name':pickle['name'],'feature':doc['feature']['name'],'path':relative,'line':line,'source':source['url'].removesuffix('.git')+'/blob/'+source['revision']+'/'+relative+'#L'+str(line),'source_sha256':hashlib.sha256(text.encode()).hexdigest(),'tags':[t['name'] for t in pickle['tags']],'steps':steps}
def rdf():
 base=CACHE/'rdf';source=SOURCES['rdf']
 seen=set()
 for folder in ['sparql10','sparql11']:
  for path in sorted((base/'sparql'/folder).rglob('manifest*.ttl')):
   graph=Graph().parse(path,format='turtle')
   for head in graph.objects(None,MF.entries):
    for test in graph.items(head):
     if test in seen:continue
     seen.add(test);relative=str(path.relative_to(base));test_id=str(test).replace(base.resolve().as_uri()+'/', '').replace(base.as_uri()+'/', '')
     # rdflib resolves symlinks differently across versions. Keep portable IDs.
     test_id=relative+'#'+str(test).split('#')[-1] if '#' in str(test) else relative+':'+str(test).rsplit('/',1)[-1]
     action=graph.value(test,MF.action);result=graph.value(test,MF.result)
     def rel(x):
      if x is None:return None
      if isinstance(x,BNode):return None
      try:return str(local(x).relative_to(base))
      except ValueError:
       try:return str(local(x).relative_to(base.resolve()))
       except ValueError:return str(x)
     query=graph.value(action,QT.query) if action else None
     if query is None and action and not isinstance(action,BNode):query=action
     data=[rel(x) for x in graph.objects(action,QT.data)] if action else []
     named=[]
     for item in graph.objects(action,QT.graphData):
      if isinstance(item,BNode):named.append({'file':rel(graph.value(item,QT.graph)),'name':str(graph.value(item,RDFS.label) or graph.value(item,QT.graph))})
      else:named.append({'file':rel(item),'name':str(item)})
     yield {'id':'rdf:'+test_id,'suite':'rdf','name':str(graph.value(test,MF.name) or test_id),'feature':folder+'/'+path.parent.name,'path':relative,'source':source['url'].removesuffix('.git')+'/blob/'+source['revision']+'/'+relative,'source_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'types':[str(t).rsplit('#',1)[-1] for t in graph.objects(test,RDF.type)],'query_file':rel(query),'result_file':rel(result),'data':data,'named':named,'requires':[str(x) for x in graph.objects(test,MF.requires)],'manifest_test':str(test),'action_complex':isinstance(action,BNode)}
def main():
 cases=[*gherkin('opencypher'),*gherkin('tinkerpop'),*rdf()]
 assert len({c['id'] for c in cases})==len(cases)
 out=ROOT/'catalog.json';out.write_text(json.dumps({'sources':SOURCES,'cases':cases},indent=2)+'\n')
 from collections import Counter
 print(Counter(c['suite'] for c in cases))
if __name__=='__main__':main()
