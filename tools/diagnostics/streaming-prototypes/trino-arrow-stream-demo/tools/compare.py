#!/usr/bin/env python3
"""Paired, correctness-checked DuckDB/Trino streaming benchmark on this host."""
import json, os, pathlib, random, statistics, subprocess, time, urllib.request
from demo import ROOT, COMPOSE, BASE
OUT = ROOT/'results/comparison'
DUCK = ROOT.parent/'duckdb-arrow-stream-demo'
OUT.mkdir(exist_ok=True)

def trino(sql):
    req=urllib.request.Request(BASE+'/v1/statement',data=sql.encode(),headers={'X-Trino-User':'arrow-benchmark'})
    rows=[]; start=time.perf_counter()
    while True:
        with urllib.request.urlopen(req,timeout=120) as response: payload=json.load(response)
        if 'error' in payload: raise RuntimeError(json.dumps(payload['error']))
        rows.extend(payload.get('data',[]))
        if 'nextUri' not in payload: break
        req=urllib.request.Request(payload['nextUri'],headers={'X-Trino-User':'arrow-benchmark'})
    return rows, {'client_seconds':time.perf_counter()-start, 'query_id':payload['id'], 'stats':payload.get('stats',{})}

def expected(rows):
    full,tail=divmod(rows,20); tail=min(tail,10)
    cycle=sum(i for i in range(100000) if i%20<10)
    docs=(rows//100000)*cycle+sum(i for i in range(rows%100000) if i%20<10)
    return [[full*10+tail,10*full*(full-1)//2+tail*full,docs]]

def sql_for(rows,batch,parts,latency,mode):
    join='' if mode=='scan' else 'JOIN memory.default.documents d ON d.id=m.document_id'
    return f'SELECT count(*), sum(m.source_id), sum(m.document_id) FROM remote.default.bench_{rows}_{batch}_{parts}_{latency} m {join} WHERE m.score>=0.8'

def measure(engine,args):
    rows,batch,parts,latency,mode=args
    if engine=='trino_http':
        values,metrics=trino(sql_for(*args)); assert values==expected(rows),(values,expected(rows))
    else:
        prefix=['/usr/bin/time','-l','/usr/bin/env','DYLD_LIBRARY_PATH='+str(DUCK/'target/release/deps')]
        if engine=='duckdb_http': prefix+=['ARROW_BENCH_HTTP=http://127.0.0.1:18881']
        # Native generator concurrency models the same count of concurrent requests.
        workers=parts if parts>1 else 0
        cmd=prefix+[str(DUCK/'target/release/duckdb-arrow-stream-demo'),'--bench',str(rows),str(batch),str(latency),str(workers),'8','scan' if mode=='scan' else 'fixed']
        start=time.perf_counter()
        result=subprocess.run(cmd,text=True,capture_output=True,timeout=120)
        if result.returncode: raise RuntimeError(result.stderr)
        metrics=json.loads(result.stdout)
        metrics['client_seconds']=time.perf_counter()-start
        metrics['peak_rss_mib']=int(next(line.split()[0] for line in result.stderr.splitlines() if 'maximum resident set size' in line))/2**20
    return {**metrics, 'engine':engine, 'rows':rows, 'batch':batch, 'parts':parts, 'latency_ms':latency, 'mode':mode, 'validated':True}

def main():
    cases=[
        ('1m_join_p1',[1_000_000,8192,1,0,'join']),
        ('10m_join_p1',[10_000_000,8192,1,0,'join']),
        ('10m_join_p4',[10_000_000,8192,4,0,'join']),
        ('10m_scan_p4',[10_000_000,8192,4,0,'scan']),
        ('100m_join_p4',[100_000_000,8192,4,0,'join']),
        ('10m_large_batch_p4',[10_000_000,65536,4,0,'join']),
        ('latency_p1',[200_000,2000,1,10,'join']),
        ('latency_p4',[200_000,2000,4,10,'join']),
        ('latency_p16',[200_000,2000,16,10,'join']),
    ]
    raw=[]
    with (OUT/'server.log').open('w') as log:
        server=subprocess.Popen([str(ROOT/'server/target/release/trino-arrow-fixture')],stdout=log,stderr=log)
        try:
            for _ in range(100):
                if server.poll() is not None: raise RuntimeError('Rust fixture failed to start')
                try: urllib.request.urlopen('http://127.0.0.1:18881/health',timeout=1).close(); break
                except OSError: time.sleep(.1)
            subprocess.run(COMPOSE+['up','-d'],check=True)
            deadline=time.monotonic()+120
            while True:
                try: trino('SELECT 1'); break
                except Exception:
                    if time.monotonic()>deadline: raise
                    time.sleep(1)
            trino('CREATE SCHEMA IF NOT EXISTS memory.default')
            trino('CREATE TABLE memory.default.documents AS SELECT CAST(x*100+y AS BIGINT) id FROM UNNEST(sequence(0,999)) t(x) CROSS JOIN UNNEST(sequence(0,99)) u(y)')
            for _ in range(3): measure('trino_http',[1_000_000,8192,4,0,'join'])
            for engine in ['duckdb_native','duckdb_http','trino_http']:
                measure(engine,[10_000_000,8192,4,0,'join'])
            plan,_=trino('EXPLAIN '+sql_for(10_000_000,8192,4,0,'join'))
            (OUT/'trino-plan.txt').write_text(plan[0][0])
            jobs=[(name,args,engine,rep) for name,args in cases for engine in ['duckdb_native','duckdb_http','trino_http'] for rep in range(3)]
            random.Random(42).shuffle(jobs)
            with (OUT/'raw.jsonl').open('w') as f:
                for name,args,engine,rep in jobs:
                    item=dict(case=name,repetition=rep,**measure(engine,args));raw.append(item)
                    f.write(json.dumps(item)+'\n');f.flush()
                    print(f'{name} {engine} #{rep}: {item["client_seconds"]:.4f}s',flush=True)
            summary=[]
            for name,args in cases:
                for engine in ['duckdb_native','duckdb_http','trino_http']:
                    values=[x['client_seconds'] for x in raw if x['case']==name and x['engine']==engine]
                    summary.append(dict(case=name,engine=engine,rows=args[0],batch=args[1],parts=args[2],latency_ms=args[3],mode=args[4],median_seconds=statistics.median(values),min_seconds=min(values),max_seconds=max(values),million_rows_per_second=args[0]/statistics.median(values)/1e6))
            report=dict(host=subprocess.check_output(['sysctl','-n','machdep.cpu.brand_string'],text=True).strip(),
                docker=subprocess.check_output(['docker','info','--format','CPUs={{.NCPU}} RAM={{.MemTotal}}'],text=True).strip(),
                trino_version=trino('SELECT version()')[0][0][0],duckdb_version='1.5.5',repetitions=3,results=summary,
                timing='Client wall time; DuckDB includes process/database setup; Trino uses a warmed persistent JVM. All return three aggregate values.',
                validated_runs=len(raw))
            (OUT/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
            print(json.dumps(report,indent=2))
        finally:
            logs=subprocess.run(COMPOSE+['logs','--no-color'],capture_output=True,text=True)
            (OUT/'trino.log').write_text(logs.stdout+logs.stderr)
            subprocess.run(COMPOSE+['down'],check=False)
            server.terminate()
            try:server.wait(timeout=10)
            except subprocess.TimeoutExpired:server.kill();server.wait()

if __name__=='__main__': main()
