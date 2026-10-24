"""Small Trino protocol client for local conformance tests (no extra packages)."""
import json
import os
import sys
import urllib.request


def query(sql, endpoint=None):
    endpoint = endpoint or os.environ.get('ORCHIDDB_TEST_TRINO_URL', 'http://127.0.0.1:18080')
    request = urllib.request.Request(endpoint + '/v1/statement', data=sql.encode(),
                                     headers={'X-Trino-User': 'orchiddb-tests'})
    rows, columns = [], []
    while True:
        with urllib.request.urlopen(request, timeout=120) as response:
            page = json.load(response)
        if 'error' in page:
            raise RuntimeError(page['error']['message'])
        columns = page.get('columns', columns)
        rows.extend(page.get('data', []))
        if 'nextUri' not in page:
            return [dict(zip((c['name'] for c in columns), row)) for row in rows]
        request = urllib.request.Request(page['nextUri'], headers={'X-Trino-User': 'orchiddb-tests'})


if __name__ == '__main__':
    for row in query(sys.argv[1] if len(sys.argv) > 1 else sys.stdin.read()):
        print(json.dumps(row))
