#!/usr/bin/env python3
"""Run each measurement in a fresh process; macOS /usr/bin/time reports peak RSS."""
import json, os, pathlib, platform, random, statistics, subprocess, time, sys
root = pathlib.Path(__file__).resolve().parents[1]
out = root / 'results'
out.mkdir(exist_ok=True)
env = dict(os.environ, DYLD_LIBRARY_PATH=str(root / 'target/release/deps'))
cases = []
def add(name, rows, batch, latency=0, workers=0, threads=1, mode='join'):
    cases.append((name, [rows, batch, latency, workers, threads, mode]))
for batch in (4, 256, 2048, 8192, 65536):
    add(f'batch_{batch}', 1_000_000, batch)
for mode in ('scan', 'join', 'fixed', 'materialize'):
    add(f'10m_{mode}', 10_000_000, 8192, mode=mode)
for threads in (4, 8):
    add(f'10m_threads_{threads}', 10_000_000, 8192, threads=threads)
for threads in (4, 8):
    add(f'10m_fixed_threads_{threads}', 10_000_000, 8192, threads=threads, mode='fixed')
for threads in (1, 4, 8):
    add(f'10m_fixed_large_batch_threads_{threads}', 10_000_000, 65536, threads=threads, mode='fixed')
for workers in (1, 4, 8):
    add(f'10m_workers_{workers}', 10_000_000, 8192, workers=workers)
for rows in (30_000_000, 100_000_000):
    add(f'scale_{rows}', rows, 8192, mode='fixed')
for batch in (8192, 65536):
    add(f'100m_tuned_batch_{batch}', 100_000_000, batch, threads=8, mode='fixed')
add('1b_tuned', 1_000_000_000, 8192, threads=8, mode='fixed')
for workers in (0, 4, 16):
    add(f'latency_workers_{workers}', 200_000, 2000, latency=10, workers=workers)
add('latency_larger_batch', 200_000, 20000, latency=10)

def execute(args):
    start = time.perf_counter()
    result = subprocess.run(['/usr/bin/time', '-l', '/usr/bin/env', 'DYLD_LIBRARY_PATH='+str(root/'target/release/deps'), str(root/'target/release/duckdb-arrow-stream-demo'), '--bench', *map(str,args)],
        env=env, text=True, capture_output=True, timeout=120, check=True)
    measurement = json.loads(result.stdout)
    measurement['process_wall_seconds'] = time.perf_counter() - start
    measurement['peak_rss_bytes'] = int(next(line.split()[0] for line in result.stderr.splitlines() if 'maximum resident set size' in line))
    return measurement

# Untimed process warmup for library/filesystem caches.
execute([100000,8192,0,0,1,'join'])
jobs = [(name,args,rep) for name,args in cases for rep in range(3)]
random.Random(42).shuffle(jobs)
resume = '--resume' in sys.argv
measurements = [json.loads(line) for line in (out/'raw.jsonl').read_text().splitlines()] if resume else []
completed = {(m['case'],m['repetition']) for m in measurements}
with (out/'raw.jsonl').open('a' if resume else 'w') as f:
    for name,args,rep in jobs:
        if (name,rep) in completed: continue
        item = dict(case=name, repetition=rep, **execute(args))
        measurements.append(item)
        f.write(json.dumps(item)+'\n'); f.flush()
        print(f'{name} #{rep}: {item["seconds"]:.3f}s, {item["rows_per_second"]/1e6:.2f}M rows/s, {item["peak_rss_bytes"]/2**20:.1f} MiB', flush=True)
summary=[]
for name,args in cases:
    subset=[m for m in measurements if m['case']==name]
    summary.append(dict(case=name, args=args,
        median_seconds=statistics.median(m['seconds'] for m in subset),
        min_seconds=min(m['seconds'] for m in subset), max_seconds=max(m['seconds'] for m in subset),
        median_rows_per_second=statistics.median(m['rows_per_second'] for m in subset),
        median_peak_rss_mib=statistics.median(m['peak_rss_bytes']/2**20 for m in subset)))
report=dict(platform=platform.platform(), cpu=subprocess.check_output(['sysctl','-n','machdep.cpu.brand_string'],text=True).strip(),
    memory_bytes=int(subprocess.check_output(['sysctl','-n','hw.memsize'],text=True)),
    duckdb='1.5.5', build='release', repetitions=3, seed=42, results=summary)
(out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
