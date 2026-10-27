#!/usr/bin/env python3
"""Local, required live gates. No publication, SDK removal, or production endpoints."""
import argparse
import json
import os
import re
import hashlib
import shutil
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / 'tools/trino'))
from query import query

COMPOSE = ['docker', 'compose', '-p', 'orchid-execution-integration', '-f', str(HERE / 'compose.yml')]
ENDPOINT = 'http://127.0.0.1:18089'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--extension', type=Path, required=True, help='Freshly built Orchid extension')
    parser.add_argument('--python', type=Path, default=ROOT / 'extension/vendor/test-env/bin/python')
    args = parser.parse_args()
    extension = args.extension.resolve()
    if not extension.is_file():
        parser.error('extension artifact must exist')
    output = ROOT / 'target/diagnostics/execution-integration'
    output.mkdir(parents=True, exist_ok=True)
    report = {'status': 'RUNNING', 'suites': {}, 'extension': str(extension), 'endpoint': ENDPOINT, 'extension_sha256': hashlib.sha256(extension.read_bytes()).hexdigest(),
              'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'working_tree_modified': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT))}
    def save():
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    def run(name, command, env=None, cwd=ROOT):
        print(f'Running {name}', flush=True)
        log = output / (name + '.log')
        with log.open('w') as stream:
            result = subprocess.run(list(map(str, command)), cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT)
        text = log.read_text()
        skipped = bool(re.search(r'skipped=[1-9]|[1-9][0-9]* ignored;', text))
        failed = result.returncode != 0 or skipped
        report['suites'][name] = {'status': 'FAIL' if failed else 'PASS', 'log': str(log), 'skips_detected': skipped}
        match = re.search(r'Ran (\d+) tests', text)
        if match:
            report['suites'][name]['tests'] = int(match[1])
        save()
        if failed:
            raise RuntimeError(f'{name} failed; see {log}')
    save()
    try:
        run('services', COMPOSE + ['up', '-d'])
        deadline = time.monotonic() + 180
        while True:
            try:
                query('SELECT 1', ENDPOINT, user='admin')
                break
            except Exception:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(1)
        subprocess.run(COMPOSE + ['exec', '-T', 'postgres', 'psql', '-U', 'postgres', '-d', 'orchid', '-v', 'ON_ERROR_STOP=1'], input=(ROOT / 'tools/trino/iceberg-jdbc.sql').read_text(), text=True, check=True, stdout=subprocess.DEVNULL)
        subprocess.run(COMPOSE + ['exec', '-T', '--user', 'root', 'trino', 'chmod', '0777', '/tmp/orchid-integration-warehouse'], check=True)
        query('CREATE SCHEMA IF NOT EXISTS iceberg.orchid', ENDPOINT, user='admin')
        query('DROP TABLE IF EXISTS iceberg.orchid.documents', ENDPOINT, user='admin')
        query('CREATE TABLE iceberg.orchid.documents AS SELECT * FROM postgresql.public.documents', ENDPOINT, user='admin')
        query('CREATE SCHEMA IF NOT EXISTS memory.orchid', ENDPOINT, user='admin')
        query('DROP TABLE IF EXISTS memory.orchid.labels', ENDPOINT, user='admin')
        query('CREATE TABLE memory.orchid.labels AS SELECT id, title FROM postgresql.public.labels', ENDPOINT, user='admin')
        env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target'), CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
                   ORCHIDDB_TEST_OPA_URL=ENDPOINT, ORCHID_EXTERNAL_TESTS='1', ORCHID_EXTENSION=str(extension), ORCHID_EXECUTION_INTEGRATION='1',
                   ORCHID_TEST_PG_DSN='host=127.0.0.1 port=15439 dbname=orchid user=postgres password=orchid-fixture')
        run('trino-opa', ['cargo','test','--locked','--no-default-features','--features','trino','--test','trino_opa','--','--ignored','--nocapture'], env)
        run('duckdb-extensions', [args.python, '-m', 'unittest', '-v',
            'test_execution_extensions', 'test_duckdb_functions', 'test_lake_queries',
            'test_host_federation', 'test_schema_connection', 'test_value_boundaries', 'test_extension.ExternalStorageTests'], env, ROOT / 'extension/tests')
        streaming_env = dict(env, CXXFLAGS='-DORCHID_TEST_ARROW_STREAM', ORCHID_BUILD_DIR=str(output / 'streaming-extension'))
        run('duckdb-streaming-build', [sys.executable, ROOT / 'extension/scripts/build.py'], streaming_env)
        streaming_env.update(ORCHID_STREAM_TESTS='1', ORCHID_EXTENSION=str(output / 'streaming-extension/orchid.duckdb_extension'))
        run('duckdb-streaming', [args.python, '-m', 'unittest', '-v', 'test_host_streaming'], streaming_env, ROOT / 'extension/tests')
        run('native-test-harness-build', ['cargo','build','--locked','-p','orchiddb-compiler-native','--no-default-features'], env)
        native_name = 'liborchiddb_compiler.dylib' if sys.platform == 'darwin' else 'liborchiddb_compiler.so'
        env['ORCHIDDB_NATIVE_LIBRARY'] = str(ROOT / 'target/debug' / native_name)
        harness = output / 'python-harness'
        shutil.copytree(ROOT / 'clients/python/src/orchiddb', harness / 'orchiddb', dirs_exist_ok=True, ignore=shutil.ignore_patterns('native', '__pycache__'))
        (harness / 'orchiddb/CORE_REVISION').write_text(report['revision'] + '\n')
        env['PYTHONPATH'] = str(harness)
        run('trino-duckdb-residual', [args.python, '-m', 'unittest', '-v', 'test_residual'], env, HERE)
        # Fail closed if policy service is unavailable. Restore only this fixture.
        subprocess.run(COMPOSE + ['stop', 'opa'], check=True, stdout=subprocess.DEVNULL)
        try:
            try:
                query('SELECT id FROM postgresql.public.documents', ENDPOINT, user='alice')
            except RuntimeError as error:
                if not any(word in str(error).lower() for word in ('opa', 'policy')):
                    raise
            else:
                raise AssertionError('query succeeded without OPA')
        finally:
            subprocess.run(COMPOSE + ['start', 'opa'], check=True, stdout=subprocess.DEVNULL)
        report['suites']['opa-unavailable'] = {'status': 'PASS'}
        report['status'] = 'PASS'
    except Exception as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        raise
    finally:
        save()
        print(f'Report: {output / "report.json"}', flush=True)


if __name__ == '__main__':
    main()
