"""Start the local Trino integration fixture. No cloud resources or CI jobs."""
from pathlib import Path
import json
import subprocess
import time
import urllib.request
from query import query

ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / 'target/trino'
NETWORK = 'orchiddb-trino-test'


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def container(name, image, *args):
    existing = subprocess.run(['docker', 'inspect', name], capture_output=True, text=True)
    if existing.returncode:
        run('docker', 'run', '-d', '--name', name, '--network', NETWORK, *args, image)
    else:
        info = json.loads(existing.stdout)[0]
        if info['Config']['Image'] != image:
            raise RuntimeError(f'{name} exists with a different image')
        if NETWORK not in info['NetworkSettings']['Networks']:
            run('docker', 'network', 'connect', NETWORK, name)
        run('docker', 'start', name)


def main():
    STATE.mkdir(parents=True, exist_ok=True)
    if subprocess.run(['docker', 'network', 'inspect', NETWORK], capture_output=True).returncode:
        run('docker', 'network', 'create', NETWORK)
    container('orchiddb-trino-pg', 'postgres:16-bookworm', '-e', 'POSTGRES_PASSWORD=fixture', '-e', 'POSTGRES_DB=iceberg')
    deadline = time.monotonic() + 60
    while subprocess.run(['docker','exec','orchiddb-trino-pg','pg_isready','-U','postgres'],capture_output=True).returncode:
        if time.monotonic() > deadline:
            raise RuntimeError('Iceberg metadata database did not start')
        time.sleep(1)
    subprocess.run(['docker','exec','-i','orchiddb-trino-pg','psql','-U','postgres','-d','iceberg','-v','ON_ERROR_STOP=1'], input=(ROOT/'tools/trino/iceberg-jdbc.sql').read_text(),text=True,check=True)
    container('orchiddb-trino-loki', 'grafana/loki:3.5.0', '-p', '127.0.0.1:13100:3100')
    prom = STATE / 'prometheus.yml'
    prom.write_text('global:\n  scrape_interval: 1s\nscrape_configs:\n  - job_name: orchid\n    static_configs:\n      - targets: ["localhost:9090"]\n')
    container('orchiddb-trino-prom', 'prom/prometheus:v3.4.0', '-v', f'{prom}:/etc/prometheus/prometheus.yml:ro', '-p', '127.0.0.1:19090:9090')
    container('orchiddb-trino', 'trinodb/trino:484', '-p', '127.0.0.1:18080:8080')
    catalogs = {
        'iceberg': '''connector.name=iceberg
iceberg.catalog.type=jdbc
iceberg.jdbc-catalog.catalog-name=orchid
iceberg.jdbc-catalog.driver-class=org.postgresql.Driver
iceberg.jdbc-catalog.connection-url=jdbc:postgresql://orchiddb-trino-pg:5432/iceberg
iceberg.jdbc-catalog.connection-user=postgres
iceberg.jdbc-catalog.connection-password=fixture
iceberg.jdbc-catalog.default-warehouse-dir=local:///tmp/warehouse
fs.local.enabled=true
local.location=/tmp/warehouse
''',
        'loki': 'connector.name=loki\nloki.uri=http://orchiddb-trino-loki:3100\n',
        'prometheus': 'connector.name=prometheus\nprometheus.uri=http://orchiddb-trino-prom:9090\nprometheus.query.chunk.size.duration=1h\nprometheus.max.query.range.duration=1d\n',
        'memory': 'connector.name=memory\n',
    }
    for name, body in catalogs.items():
        path = STATE / (name + '.properties')
        path.write_text(body)
        run('docker', 'cp', str(path), 'orchiddb-trino:/etc/trino/catalog/' + path.name)
    run('docker', 'exec', 'orchiddb-trino', 'mkdir', '-p', '/tmp/warehouse')
    run('docker', 'restart', 'orchiddb-trino')
    deadline = time.monotonic() + 120
    while True:
        try:
            query('SELECT 1')
            break
        except Exception:
            if time.monotonic() > deadline:
                raise
            time.sleep(1)
    print(query('SHOW CATALOGS'))
    query("CREATE SCHEMA IF NOT EXISTS iceberg.orchid WITH (location='local:///tmp/warehouse/orchid')")
    query('CREATE TABLE IF NOT EXISTS iceberg.orchid.documents (id BIGINT, title VARCHAR, embedding ARRAY(DOUBLE))')
    if not query('SELECT id FROM iceberg.orchid.documents LIMIT 1'):
        query("INSERT INTO iceberg.orchid.documents VALUES (1,'Orchid',ARRAY[1.0,0.0])")
        query("INSERT INTO iceberg.orchid.documents VALUES (2,'Gardening',ARRAY[0.0,1.0])")
    now = time.time_ns()
    payload = {'streams': [{'stream': {'job': 'orchid', 'level': 'info'}, 'values': [[str(now), 'orchid integration ready']]}]}
    request = urllib.request.Request('http://127.0.0.1:13100/loki/api/v1/push',data=json.dumps(payload).encode(),headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(request,timeout=30) as response:
        assert response.status == 204
    print('Trino, Iceberg, Loki and Prometheus ready',flush=True)


if __name__ == '__main__':
    main()
