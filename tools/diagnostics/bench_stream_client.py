#!/usr/bin/env python3
"""Foreign-client streaming throughput, including native bridge and Arrow decode.
Use a release native library. Local authorization is a benchmark-only policy;
production SpiceDB latency/cache behavior must be measured separately.
"""
import argparse,base64,hashlib,json,os,pathlib,subprocess,time
import pyarrow as pa
from orchiddb import StreamConnection
root=pathlib.Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('--n',type=int,default=10000);p.add_argument('--topic');p.add_argument('--library',required=True);p.add_argument('--batch-sizes',default='1,256');p.add_argument('--decode',choices=['row','columns'],default='columns');a=p.parse_args()
library_sha256=hashlib.sha256(pathlib.Path(a.library).read_bytes()).hexdigest()
topic=a.topic or 'orchid-foreign-bench-'+str(time.time_ns())
if not a.topic:
    env=os.environ|dict(BENCH_MODE='seed',BENCH_N=str(a.n),BENCH_TOPIC=topic,BENCH_INFLIGHT='4096',BENCH_KEYS=str(a.n),KAFKA_BOOTSTRAP='127.0.0.1:19092')
    subprocess.run([str(root/'target/release/examples/kafka_bench')],env=env,stdout=subprocess.DEVNULL,check=True)
schema={'tables':{'events':{'label':'Event','resource_type':'event','read_permission':'view','write_permission':'edit','properties':{'type':'object','required':['name','score','body'],'properties':{'name':{'type':'string'},'score':{'type':'integer'},'body':{'type':'string'}}}}}}
for size in map(int,a.batch_sizes.split(',')):
    auth_calls=[0]
    def auth(subject,checks):auth_calls[0]+=1;return [True]*len(checks)
    with StreamConnection(schema,kafka={'bootstrap.servers':'127.0.0.1:19092'},topic=topic,source='bench',authorizer=auth,library=a.library) as c:
        c.subscribe('MATCH (e:Event) RETURN e.name AS name, e.score AS score',subject={'subject_type':'user','subject_id':'bench'},offsets={str(p):0 for p in range(6)})
        start=time.perf_counter();steady=start;received=checksum=wire_bytes=ipc_streams=0;warmup=min(1000,a.n//10);steady_count=0
        while received<a.n:
            b=c.next_batch(size,10)
            ipc_streams+=len(b.get("batches", []))+sum("ipc" in f["result"] for f in b["frames"] if f["result"])
            wire_bytes+=sum(len(r["ipc"]) for r in b.get("batches", []))
            wire_bytes+=sum(len(f["result"]["ipc"]) for f in b["frames"] if f["result"] and "ipc" in f["result"])
            batches=[pa.ipc.open_stream(base64.b64decode(r['ipc'])).read_all() for r in b.get('batches', [])]
            scores=[table.column('score').to_pylist() for table in batches] if a.decode=='columns' else None
            for frame in b['frames']:
                ref=frame['result']
                if 'ipc' in ref:
                    table=pa.ipc.open_stream(base64.b64decode(ref['ipc'])).read_all()
                    value=table.column('score')[0].as_py()
                elif scores is not None:
                    value=scores[ref['batch']][ref['offset']]
                else:
                    value=batches[ref['batch']].slice(ref['offset'],ref['length']).column('score')[0].as_py()
                checksum+=value;received+=1
            if steady_count == 0 and received>=warmup:
                steady=time.perf_counter();steady_count=received
        elapsed=time.perf_counter()-start;steady_elapsed=time.perf_counter()-steady
        assert checksum==a.n*(a.n-1)//2,(checksum,a.n)
        print(json.dumps(dict(n=received,decode=a.decode,batch_size=size,seconds=elapsed,records_s=received/elapsed,steady_records_s=(received-steady_count)/steady_elapsed,warmup_records=steady_count,checksum=checksum,auth_callbacks=auth_calls[0],ipc_base64_bytes=wire_bytes,ipc_streams=ipc_streams,topic=topic,library=a.library,library_sha256=library_sha256)),flush=True)
