"""Run identical payload/ack settings against the isolated Docker benchmark stack."""
import hashlib, json, os, pathlib, subprocess, sys, time
binary = str(pathlib.Path(sys.argv[1]).resolve())
with open(binary, "rb") as f:
    binary_sha256 = hashlib.file_digest(f, "sha256").hexdigest()
output = pathlib.Path(sys.argv[2])
n = int(sys.argv[3]) if len(sys.argv)>3 else 3000
modes = sys.argv[4:] or ['native-read','native-project','orchid-read','native-write','orchid-write']
topic = f'orchid-bench-{time.time_ns()}'
output.parent.mkdir(parents=True,exist_ok=True)
def run(mode, topic):
    env = dict(os.environ, BENCH_N=str(n), BENCH_MODE=mode, BENCH_TOPIC=topic)
    completed = subprocess.run([binary], env=env, text=True, capture_output=True, timeout=300)
    if completed.returncode:
        raise RuntimeError(f'{mode}: {completed.stderr[-4000:]}')
    result = json.loads(completed.stdout.strip().splitlines()[-1])
    result.update(topic=topic, binary=binary, binary_sha256=binary_sha256, unix_time=time.time(), batch_size=int(env.get('BENCH_BATCH', '256')), inflight=int(env.get('BENCH_INFLIGHT', '4096')), linger_ms=int(env.get('BENCH_LINGER_MS', '5')), entity_keys=int(env.get('BENCH_KEYS', str(n))))
    print(json.dumps(result),flush=True)
    with output.open('a') as f: f.write(json.dumps(result)+'\n')
if any('write' not in mode and not mode.endswith('live') for mode in modes):
    run('seed',topic)
for mode in modes:
    run(mode, topic+'-'+mode if 'write' in mode or mode.endswith('live') else topic)
