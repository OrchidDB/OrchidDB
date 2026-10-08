import os
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
IMAGE = 'starrocks/allin1-ubuntu@sha256:5cbf09c4c788d2ca41a8e38599b650f04dbc6af0de51ebda0952770fb8333e5d'
NAME = 'orchiddb-starrocks'
PORT = os.environ.get('ORCHIDDB_TEST_STARROCKS_PORT', '19030')


def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def main():
    conformance = "--conformance" in sys.argv[1:]
    existing = subprocess.run(['docker', 'inspect', NAME], capture_output=True, text=True)
    if existing.returncode:
        run('docker', 'run', '--detach', '--name', NAME, '-p', f'127.0.0.1:{PORT}:9030', IMAGE)
    else:
        container = json.loads(existing.stdout)[0]
        images = {IMAGE, 'starrocks/allin1-ubuntu@sha256:1dfce10bb2fc1beca7d47f82dabb093bb34dd73fc94014212ac902eef17e2ef3'}
        if container['Config']['Image'] not in images:
            raise SystemExit(f'{NAME} exists with a different image')
        bindings = container['HostConfig']['PortBindings'].get('9030/tcp', [])
        if not any(p['HostIp'] == '127.0.0.1' and p['HostPort'] == PORT for p in bindings):
            raise SystemExit(f'{NAME} is not bound to 127.0.0.1:{PORT}')
        run('docker', 'start', NAME, stdout=subprocess.DEVNULL)
    deadline = time.monotonic() + 180
    while True:
        ready = subprocess.run(['docker', 'exec', NAME, 'mysql', '-h127.0.0.1', '-P9030', '-uroot', '-e', 'SELECT current_version(); SHOW BACKENDS'], capture_output=True, text=True)
        lines = [line.split('\t') for line in ready.stdout.splitlines()]
        backend_header = next((index for index, row in enumerate(lines) if row and row[0] == 'BackendId'), None)
        alive = backend_header is not None and any(dict(zip(lines[backend_header], row)).get('Alive') == 'true' for row in lines[backend_header + 1:])
        if ready.returncode == 0 and '4.1.6-' in ready.stdout and alive:
            print(ready.stdout, flush=True)
            break
        if time.monotonic() >= deadline:
            raise SystemExit('StarRocks 4.1 did not become ready within 180 seconds')
        time.sleep(2)
    python = Path(os.environ.get('TEST_PYTHON', ROOT / 'target/starrocks-env/bin/python'))
    if not python.exists():
        run(sys.executable, '-m', 'venv', str(python.parent.parent))
    imports = subprocess.run([str(python), '-c', 'import pytest, pyarrow, pymysql'], capture_output=True)
    if imports.returncode:
        run(str(python), '-m', 'pip', 'install', 'pytest>=8', 'pyarrow>=18', 'pymysql>=1.1')
    if conformance:
        imports = subprocess.run([str(python), '-c', 'import lark, neo4j, pyoxigraph, rdflib, gherkin'], capture_output=True)
        if imports.returncode:
            run(str(python), '-m', 'pip', 'install', '-r', 'conformance/requirements.txt')
        if '--no-build' not in sys.argv[1:]:
            run('bash', 'conformance/build-orchiddb.sh')
        import uuid
        database = 'orchiddb_conformance_' + uuid.uuid4().hex
        java_home = os.environ.get('JAVA_HOME')
        if sys.platform == 'darwin':
            java_home = subprocess.check_output(['/usr/libexec/java_home', '-F', '-v', '21'], text=True).strip()
        java = str(Path(java_home) / 'bin/java') if java_home else 'java'
        profile = os.environ.get('CONFORMANCE_CARGO_PROFILE', 'dev')
        profile = 'debug' if profile == 'dev' else profile
        target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / profile
        env = dict(os.environ, CONFORMANCE_PYTHON=str(python.absolute()),
                   CONFORMANCE_ORCHIDDB_BINARY=str(target / 'upstream'),
                   ORCHIDDB_JVM_STORE=str(target / 'orchiddb-jvm-store'),
                   ORCHIDDB_JVM_CLASSPATH=str(ROOT / 'jvm/target/classes') + os.pathsep + (ROOT / 'jvm/target/classpath.txt').read_text().strip(),
                   CONFORMANCE_TINKERPOP_SOURCE=str(ROOT / 'conformance/upstream/cache/tinkerpop'),
                   CONFORMANCE_JAVA=java, ORCHIDDB_JAVA=java,
                   ORCHIDDB_TEST_STARROCKS_PORT=PORT, ORCHIDDB_TEST_STARROCKS_DATABASE=database)
        run('docker', 'exec', NAME, 'mysql', '-h127.0.0.1', '-P9030', '-uroot', '-e', f'CREATE DATABASE `{database}`')
        try:
            run(str(python), 'conformance/upstream/engine_matrix.py', '--engine', 'starrocks',
                '--output-dir', 'target/conformance/starrocks', env=env)
        finally:
            run('docker', 'exec', NAME, 'mysql', '-h127.0.0.1', '-P9030', '-uroot', '-e', f'DROP DATABASE `{database}`')
        return
    run(sys.executable, 'scripts/clients.py', 'native')
    extension = 'dylib' if sys.platform == 'darwin' else 'so'
    env = dict(os.environ, PYTHONPATH=str(ROOT / 'clients/python/src'),
               ORCHIDDB_NATIVE_LIBRARY=str(ROOT / f'target/debug/liborchiddb_compiler.{extension}'),
               ORCHIDDB_TEST_STARROCKS_PORT=PORT)
    run(str(python), '-m', 'pytest', 'clients/python/tests/test_starrocks.py', '-q', env=env)


if __name__ == '__main__':
    main()
