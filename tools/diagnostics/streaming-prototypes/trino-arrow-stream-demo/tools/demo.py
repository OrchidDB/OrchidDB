#!/usr/bin/env python3
"""Start isolated services, run real Trino queries, save evidence, then stop them."""
import json, pathlib, subprocess, time, urllib.request, urllib.error
ROOT = pathlib.Path(__file__).resolve().parents[1]
RESULTS = ROOT / 'results'
BASE = 'http://127.0.0.1:18081'
COMPOSE = ['docker', 'compose', '-p', 'orchid-arrow-demo', '-f', str(ROOT/'compose.yaml')]

def query(sql):
    request = urllib.request.Request(BASE+'/v1/statement', data=sql.encode(),
        headers={'X-Trino-User':'arrow-demo', 'Content-Type':'text/plain'})
    rows = []
    deadline = time.monotonic() + 120
    while True:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
        if 'error' in payload: raise RuntimeError(payload['error']['message'])
        rows.extend(payload.get('data', []))
        if 'nextUri' not in payload: return rows
        if time.monotonic() > deadline: raise TimeoutError(sql)
        request = urllib.request.Request(payload['nextUri'], headers={'X-Trino-User':'arrow-demo'})

JOIN = '''SELECT m.source_id, d.title, m.score
FROM remote.default.matches m
JOIN memory.default.documents d ON d.id = m.document_id
WHERE m.score >= 0.8
ORDER BY m.source_id'''

def main():
    RESULTS.mkdir(exist_ok=True)
    report = {'tests': [], 'sql': JOIN}
    with (RESULTS/'server.log').open('w') as log:
        server = subprocess.Popen([str(ROOT/'server/target/release/trino-arrow-fixture')], stdout=log, stderr=log)
        try:
            for _ in range(50):
                if server.poll() is not None: raise RuntimeError('Rust server exited; see results/server.log')
                try:
                    urllib.request.urlopen('http://127.0.0.1:18881/health', timeout=1).close(); break
                except OSError: time.sleep(.1)
            subprocess.run(COMPOSE+['up','-d'], check=True)
            deadline = time.monotonic() + 120
            while True:
                try: query('SELECT 1'); break
                except Exception:
                    if time.monotonic() > deadline: raise
                    time.sleep(1)
            report['trino_version'] = query('SELECT version()')[0][0]
            before = (RESULTS/'server.log').read_text()
            plan = query('EXPLAIN SELECT source_id, score FROM remote.default.matches WHERE score >= 0.8')
            assert (RESULTS/'server.log').read_text() == before, 'EXPLAIN fetched remote rows'
            (RESULTS/'plan.txt').write_text(plan[0][0]+'\n')
            report['tests'].append('planning does not fetch remote data')
            query('CREATE SCHEMA IF NOT EXISTS memory.default')
            query('CREATE TABLE memory.default.documents (id BIGINT, title VARCHAR)')
            query("INSERT INTO memory.default.documents VALUES (10,'Arrow streaming'),(20,'Other document')")
            expected = [[i,'Arrow streaming',0.95 if i%2 else 0.9] for i in range(1,7)]
            rows = query(JOIN)
            assert rows == expected, rows
            report['rows'] = rows
            report['tests'].append('join, filter, and sort')
            assert query('SELECT count(*) FROM remote.default.matches') == [[12]]
            report['tests'].append('zero-column projection / count(*)')
            assert query('SELECT score, source_id FROM remote.default.matches WHERE document_id=20 ORDER BY source_id') == [[0.6,i] for i in range(1,7)]
            report['tests'].append('projected and reordered columns with filter')
            assert query('SELECT source_id, count(*) FROM remote.default.matches GROUP BY source_id ORDER BY source_id') == [[i,2] for i in range(1,7)]
            report['tests'].append('both partitions: no lost or duplicated sources')
            assert query(JOIN) == expected
            report['tests'].append('deterministic replay on a new scan')
            try:
                query('SELECT * FROM remote.default.broken')
                raise AssertionError('broken endpoint unexpectedly succeeded')
            except RuntimeError as error:
                assert 'HTTP 503' in str(error), error
                report['tests'].append('remote error propagates to SQL client')
            report['passed'] = True
            (RESULTS/'verification.json').write_text(json.dumps(report,indent=2)+'\n')
            print(json.dumps(report,indent=2))
        finally:
            logs = subprocess.run(COMPOSE+['logs','--no-color'], capture_output=True, text=True)
            (RESULTS/'trino.log').write_text(logs.stdout+logs.stderr)
            subprocess.run(COMPOSE+['down'], check=False)
            server.terminate()
            try: server.wait(timeout=10)
            except subprocess.TimeoutExpired: server.kill(); server.wait()

if __name__ == '__main__': main()
