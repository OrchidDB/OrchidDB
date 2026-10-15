#!/usr/bin/env python3
"""Real Flink -> Kafka -> Orchid streaming/hybrid integration; run with the extension test venv."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
COMPOSE = ['docker', 'compose', '-p', 'orchid-flink-e2e', '-f', str(HERE / 'compose.yml')]

def run(args, **kwargs):
    result = subprocess.run([str(x) for x in args], cwd=ROOT, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600, **kwargs)
    if result.returncode:
        raise RuntimeError(result.stdout[-12000:])
    return result.stdout

def get(path):
    with urllib.request.urlopen('http://127.0.0.1:18083' + path, timeout=10) as response:
        return json.load(response)

def wait(predicate, label, seconds=180):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = predicate()
            if value:
                return value
        except (OSError, ValueError, RuntimeError):
            pass
        time.sleep(1)
    raise RuntimeError('Timed out: ' + label)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--test-binary', type=Path,
                        help='fresh executable from cargo test --features kafka,duckdb --test flink_e2e --no-run')
    parser.add_argument('--keep', action='store_true')
    args = parser.parse_args()
    if args.test_binary:
        binary = args.test_binary.resolve()
    else:
        output = run(['cargo', 'test', '--locked', '--features', 'kafka,duckdb',
                      '--test', 'flink_e2e', '--no-run', '--message-format=json'])
        executables = []
        for line in output.splitlines():
            if line.startswith('{'):
                artifact = json.loads(line)
                if artifact.get('target', {}).get('name') == 'flink_e2e' and artifact.get('executable'):
                    executables.append(artifact['executable'])
        if len(executables) != 1:
            raise RuntimeError('Expected one Flink integration test executable')
        binary = Path(executables[0])
    if not (HERE / 'target/flink-e2e-1.0.jar').is_file():
        raise RuntimeError('Build the fixture first: mvn -f tools/diagnostics/flink-e2e/pom.xml package')
    work = ROOT / 'target' / 'flink-e2e' / str(time.time_ns())
    work.mkdir(parents=True)
    evidence = ROOT / 'artifacts' / 'e2e' / 'flink.json'
    evidence.parent.mkdir(parents=True, exist_ok=True)
    topic = 'orchid-flink-' + str(time.time_ns())
    env = os.environ | dict(ORCHID_FLINK_TOPIC=topic, ORCHID_FLINK_SNAPSHOT=str(work/'snapshot.json'))
    library = os.environ.get("DUCKDB_LIB_DIR")
    if library:
        for variable in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
            env[variable] = library + (os.pathsep + env[variable] if env.get(variable) else "")
    def probe(phase):
        output = run([binary, '--ignored', '--nocapture'], env=env | dict(ORCHID_FLINK_PHASE=phase))
        (work / (phase+'.log')).write_text(output)
        print('PASS Orchid phase:', phase, flush=True)
    def submit(savepoint=None):
        command = COMPOSE + ['exec','-T','jobmanager','flink','run','-d']
        if savepoint:
            command += ['-s',savepoint]
        output = run(command + ['-c','io.orchiddb.diagnostics.Materialize','/jobs/flink-e2e-1.0.jar',topic+'-input',topic+'-output'])
        job = re.search(r'JobID ([0-9a-f]{32})',output).group(1)
        wait(lambda: get('/jobs/'+job).get('state') == 'RUNNING', 'job running')
        return job
    def checkpoints(job):
        return get('/jobs/'+job+'/checkpoints')
    try:
        run(COMPOSE + ['up','-d'])
        run(COMPOSE + ['exec','-T','-u','root','jobmanager','chown','-R','flink:flink','/checkpoints'])
        wait(lambda: get('/overview').get('taskmanagers') == 1, 'task manager')
        wait(lambda: run(COMPOSE + ['exec','-T','kafka','/opt/kafka/bin/kafka-topics.sh','--bootstrap-server','kafka:29092','--list']) is not None, 'Kafka')
        probe('setup')
        job = submit()
        wait(lambda: checkpoints(job)['counts']['completed'] >= 3, 'initial checkpoints')
        stop = run(COMPOSE + ['exec','-T','jobmanager','flink','stop','--savepointPath','/checkpoints/savepoints',job])
        savepoint = re.search(r'(file:/+\S*savepoint-[^\s]+)',stop).group(1).rstrip('.')
        wait(lambda: get('/jobs/'+job)['state'] == 'FINISHED', 'stop with savepoint')
        probe('phase1')
        # The stream is quiescent after the successful stop/savepoint. Publish all
        # versioned state and next output offsets in ONE Iceberg snapshot.
        import pyarrow as pa
        from pyiceberg.catalog.sql import SqlCatalog
        warehouse = work/'warehouse'; warehouse.mkdir()
        catalog = SqlCatalog('checkpoint',uri='sqlite:///'+str(work/'catalog.db'),warehouse=warehouse.as_uri())
        catalog.create_namespace('checkpoint')
        table = catalog.create_table('checkpoint.manifests',schema=pa.schema([('manifest_json',pa.string())]))
        table.append(pa.table({'manifest_json':[(work/'snapshot.json').read_text()]}))
        env['ORCHID_ICEBERG_METADATA'] = table.metadata_location
        restored = submit(savepoint)
        wait(lambda: checkpoints(restored)['counts']['completed'] >= 2, 'restored checkpoint')
        run(COMPOSE + ['kill','-s','SIGKILL','taskmanager'])
        run(COMPOSE + ['up','-d','taskmanager'])
        recovery = wait(lambda: checkpoints(restored) if checkpoints(restored)['counts']['restored'] >= 2 else None,
                        'worker failure restored checkpoint')
        wait(lambda: get('/jobs/'+restored)['state'] == 'RUNNING', 'recovered job running')
        probe('phase2')
        result = {'status':'passed','flink_version':get('/overview')['flink-version'],
                  'job':job,'restored_job':restored,'restored_checkpoints':recovery['counts']['restored'],
                  'source_revision':run(['git','rev-parse','HEAD']).strip(),
                  'checks':['Kafka insertion authorization','stateful sums','duplicate input suppression',
                            'versioned deletes','single-frame filtered replacements','read authorization',
                            'savepoint stop and restore','SIGKILL taskmanager recovery',
                            'real Iceberg manifest','hybrid latest-version merge and tombstone suppression'],
                  'scope':'single broker, single Flink worker; Iceberg manifest published by PyIceberg after quiescent savepoint, not a Flink Iceberg connector sink; no production failover claim'}
        evidence.write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result,indent=2),flush=True)
    except Exception:
        (work/'compose.log').write_text(run(COMPOSE+['logs','--no-color','--tail','300']))
        print('Failure logs:',work,flush=True)
        raise
    finally:
        if not args.keep:
            run(COMPOSE + ['down','-v'])

if __name__ == '__main__':
    main()
