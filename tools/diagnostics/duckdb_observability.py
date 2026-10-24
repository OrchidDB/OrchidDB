#!/usr/bin/env python3
"""Execute compiler SQL with real community extensions and a local HTTP fixture.

Requires duckdb; optionally ORCHIDDB_LOKI_EXTENSION points to a local build.
Invoked by tests/duckdb_observability.rs, not a public compiler API.
"""
import datetime
import http.server
import json
import os
import sys
import threading
import urllib.parse

import duckdb

engine, sql = sys.argv[1:3]
paged = len(sys.argv) > 3 and sys.argv[3] == "paged"
requests = []
base = int(datetime.datetime(2026, 10, 24, tzinfo=datetime.timezone.utc).timestamp())


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        params = urllib.parse.parse_qs(parsed.query)
        requests.append((parsed.path, params))
        if engine == "prometheus":
            data = {"resultType": "matrix", "result": [{
                "metric": {"__name__": "up", "job": "api"},
                # Include a false-positive sample to exercise the residual SQL.
                "values": [[base + 60, "1"], [base + 120, "2"], [base + 121, "0"]],
            }]}
        else:
            data = {"resultType": "streams", "result": [{
                "stream": {"job": "api"},
                "values": ([[str((base + 60) * 10**9 + i * 1000), "timeout"]
                            for i in range(5002)
                            if (base + 60) * 10**9 + i * 1000 >= int(params["start"][0])][:int(params["limit"][0])]
                           if paged else [[str((base + 60) * 10**9 + 123), "timeout"],
                                          [str((base + 61) * 10**9 + 456), "timeout extra"]]),
            }]}
        body = json.dumps({"status": "success", "data": data}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    local = os.environ.get("ORCHIDDB_LOKI_EXTENSION") if engine == "loki" else None
    connection = duckdb.connect(config={"allow_unsigned_extensions": "true"} if local else {})
    if local:
        connection.execute("LOAD '" + local.replace("'", "''") + "'")
    else:
        connection.execute(f"INSTALL {engine} FROM community")
        connection.execute(f"LOAD {engine}")
    # Deliberately non-UTC: mapped timestamps must remain UTC regardless of session.
    connection.execute("SET TimeZone='America/Los_Angeles'")
    sql = sql.replace("http://127.0.0.1:1", f"http://127.0.0.1:{server.server_port}")
    rows = connection.execute(sql).fetchall()
    assert requests, "No extension HTTP request occurred"
    for path, params in requests:
        assert 'job="api"' in params["query"][0], params
        if engine == "prometheus":
            assert path == "/api/v1/query_range", path
            assert datetime.datetime.fromisoformat(params["start"][0].replace("Z", "+00:00")).timestamp() == base + 60, params
            assert float(params["step"][0]) == 60, params
            assert ' > 0' in params["query"][0], params
        else:
            assert path == "/loki/api/v1/query_range", path
            assert '|= "timeout"' in params["query"][0], params
            assert params["direction"] == ["forward"], params
    if paged:
        assert len(requests) == 2, requests
    print(json.dumps({"rows": rows, "requests": requests}, default=str))
finally:
    server.shutdown()
    server.server_close()
