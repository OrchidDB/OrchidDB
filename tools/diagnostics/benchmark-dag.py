#!/usr/bin/env python3
"""Matched repeated-query benchmark; not a replacement for upstream conformance."""
import argparse, hashlib, json, platform, statistics, subprocess, time
from pathlib import Path
QUERIES = [
 "g.V().count()",
 "g.V().has('age',gt(20)).count()",
 "g.V().out('knows').count()",
 "g.V().values('age').sum()",
 "g.V().has('age',gt(20)).values('age').order()",
 "g.V().local(__.out('knows').count()).sum()",
]
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',action='append',required=True,help='name=path; runs interleaved');p.add_argument('--output',type=Path,required=True);p.add_argument('--repeats',type=int,default=20);p.add_argument('--rounds',type=int,default=3);p.add_argument('--nodes',type=int,default=256);args=p.parse_args()
 binaries=dict(x.split('=',1) for x in args.binary); samples={n:{q:[] for q in QUERIES} for n in binaries}; expected={};instances={n:[] for n in binaries}
 fixture={'op':'fixture','nodes':[{'id':i,'label':'person','properties':{'age':i%50},'property_types':{'age':'Integer'}} for i in range(args.nodes)],'edges':[{'id':i+args.nodes,'src':i,'dst':(i+1)%args.nodes,'label':'knows','properties':{}} for i in range(args.nodes)]}
 for round in range(args.rounds):
  order=list(binaries.items());order=order if round%2==0 else order[::-1]
  for name,binary in order:
   proc=subprocess.Popen([binary],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=None,text=True)
   def send(obj):
    proc.stdin.write(json.dumps(obj)+'\n');proc.stdin.flush();result=json.loads(proc.stdout.readline());
    if 'error' in result:raise RuntimeError(result['error'])
    return result
   try:
    instances[name].append(send(fixture)['engine_instance'])
    for iteration in range(args.repeats+3):
     for query in QUERIES:
      started=time.perf_counter_ns();r=send({'op':'gremlin','query':query});elapsed=(time.perf_counter_ns()-started)/1e6
      value={k:v for k,v in r.items() if k not in ('engine_instance','backend')}
      if query in expected and value!=expected[query]:raise AssertionError(f'Result changed: {name}: {query}')
      expected[query]=value
      if iteration>=3:samples[name][query].append(elapsed)
   finally:
    proc.stdin.close();proc.wait(timeout=30)
 report={'profile':'dev, debug=0; same profile for both binaries','platform':platform.platform(),'nodes':args.nodes,'rounds':args.rounds,'repeats':args.repeats,'warmup_per_query':3,'measurement':'request/response including parse, plan, execution, serialization; fixture setup excluded','results_verified_every_iteration':True,'binaries':{n:{'sha256':hashlib.sha256(Path(b).read_bytes()).hexdigest(),'instances':instances[n],'total_ms':sum(sum(v) for v in samples[n].values()),'queries':[{'query':q,'median_ms':statistics.median(v),'samples_ms':v} for q,v in samples[n].items()]} for n,b in binaries.items()}}
 args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n')
 for name,data in report['binaries'].items():print(name,round_number(data['total_ms']),[(x['query'],round_number(x['median_ms'])) for x in data['queries']])
def round_number(n):return round(n,3)
if __name__=='__main__':main()
