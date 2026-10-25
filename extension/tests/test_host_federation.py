"""DuckDB caller transaction -> batched Weaviate -> streaming DuckDB join."""
import json
import re
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from test_extension import connect


def plan():
    column = lambda name: dict(name=name, data_type="int64", nullable=False)
    request = dict(version=1, engine="weaviate", api="weaviate", operation="search",
                   index="Docs", metric="cosine", limit=1, parameters=[None],
                   columns=[dict(source="parameter", parameter=0), dict(source="field", path=["doc_id"])],
                   body=dict(vector=[1.0, 0.0], query=dict(match_all={}), score_filters=[]))
    return dict(version=1, dialect="duckdb", execution_engine="local",
                sql='SELECT count(*) AS n, sum(r.source) AS total FROM hits r JOIN host_source h ON r.source=h.id',
                logical_plan="", fields=["n", "total"], field_types=["int64", "int64"], result_form="rows",
                constraint_proofs=[], layout_selections=[], representation_selections=[], plan_estimates=[],
                statistics_usage=None, optimizer_decisions=[], transfers=[dict(
                    source_engine="local", source_dialect="duckdb", sql="SELECT id FROM host_source ORDER BY id",
                    target_relation="hits", columns=[column("source"), column("doc")],
                    request=dict(engine="vectors", input_columns=[column("id")], template=dict(
                        adapter="weaviate", parameters=1, request=request,
                        bindings=[dict(pointer="/parameters/0", parameter=0, encoding="value")])) )])


class HostFederationTests(unittest.TestCase):
    def test_remote_batching_host_transaction_and_schema_only_binding(self):
        seen = []
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def reply(self, value):
                body = json.dumps(value).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def do_GET(self):
                seen.append(("schema", self.path))
                self.reply(dict(properties=[dict(name="doc_id", dataType=["int"])],
                                vectorIndexConfig=dict(distance="cosine")))
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                aliases = re.findall(r'(q\d+)\s*:', body["query"])
                seen.append(("batch", len(aliases)))
                self.reply(dict(data=dict(Get={alias: [dict(doc_id=10, _additional=dict(id="uuid", distance=0.1))]
                                               for alias in aliases})))
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        db = connect()
        try:
            db.execute("SELECT * FROM orchid_register_source('vectors','weaviate',?)", [json.dumps(dict(
                endpoint=f"http://127.0.0.1:{server.server_port}", authentication=dict(type="bearer", token="secret-test-token")))]).fetchall()
            db.execute("BEGIN")
            db.execute("CREATE TEMP TABLE host_source AS SELECT range::BIGINT AS id FROM range(100)")
            compiled = json.dumps(plan())
            explain = str(db.execute("EXPLAIN SELECT * FROM __orchid_federated(?)", [compiled]).fetchall())
            self.assertEqual(seen, [])
            self.assertNotIn("secret-test-token", explain)
            db.execute("CREATE SEQUENCE federation_bind_sequence")
            effect = plan()
            effect["sql"] = "SELECT nextval('federation_bind_sequence') AS ticket FROM hits LIMIT 1"
            db.execute("EXPLAIN SELECT * FROM __orchid_federated(?)", [json.dumps(effect)]).fetchall()
            self.assertEqual(db.execute("SELECT nextval('federation_bind_sequence')").fetchone(), (1,))
            self.assertEqual(seen, [])
            self.assertEqual(db.execute("SELECT * FROM __orchid_federated(?)", [compiled]).fetchall(), [(100, 4950)])
            self.assertEqual([n for kind, n in seen if kind == "batch"], [64, 36])
            self.assertEqual(len([1 for kind, _ in seen if kind == "schema"]), 1)
            dependent = plan()
            dependent["transfers"].append(dict(
                source_engine="local", source_dialect="duckdb", sql="SELECT source FROM hits",
                target_relation="scored", columns=[dict(name="score", data_type="int64", nullable=False)],
                operation=dict(engine="local", input_columns=[dict(name="source", data_type="int64", nullable=False)],
                    template=dict(dialect="duckdb", parameters=1,
                                  sql="SELECT id * 2 AS score FROM host_source WHERE id = $1"))))
            dependent["sql"] = "SELECT count(*) AS n, sum(score) AS total FROM scored"
            self.assertEqual(db.execute("SELECT * FROM __orchid_federated(?)", [json.dumps(dependent)]).fetchall(), [(100, 9900)])
            db.execute("ROLLBACK")
            self.assertEqual(db.execute("SELECT 42").fetchone(), (42,))
        finally:
            db.close()
            server.shutdown()
            server.server_close()
            thread.join()

    def test_interrupt_cancels_waiting_remote_source(self):
        started, release = threading.Event(), threading.Event()
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_GET(self):
                body = json.dumps(dict(properties=[dict(name="doc_id", dataType=["int"])],
                                       vectorIndexConfig=dict(distance="cosine"))).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def do_POST(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                started.set()
                release.wait(10)
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        server_thread = threading.Thread(target=server.serve_forever, daemon=True)
        server_thread.start()
        db = connect()
        errors = []
        query_thread = None
        try:
            db.execute("SELECT * FROM orchid_register_source('vectors','weaviate',?)", [json.dumps(dict(
                endpoint=f"http://127.0.0.1:{server.server_port}"))]).fetchall()
            db.execute("CREATE TEMP TABLE host_source AS SELECT range::BIGINT AS id FROM range(100)")
            def query():
                try:
                    db.execute("SELECT * FROM __orchid_federated(?)", [json.dumps(plan())]).fetchall()
                except Exception as error:
                    errors.append(str(error))
            query_thread = threading.Thread(target=query, daemon=True)
            query_thread.start()
            self.assertTrue(started.wait(5), "source request never started")
            db.interrupt()
            query_thread.join(3)
            self.assertFalse(query_thread.is_alive(), "cancellation waited for HTTP timeout")
            self.assertTrue(errors, "interrupted source returned a successful result")
            self.assertEqual(db.execute("SELECT 42").fetchone(), (42,))
        finally:
            release.set()
            if query_thread:
                query_thread.join(5)
            db.close()
            server.shutdown()
            server.server_close()
            server_thread.join()

    def test_limit_returns_first_batch_without_waiting_for_next_remote_page(self):
        release = threading.Event()
        calls = []
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def reply(self, value):
                body = json.dumps(value).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def do_GET(self):
                self.reply(dict(properties=[dict(name="doc_id", dataType=["int"])],
                                vectorIndexConfig=dict(distance="cosine")))
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                calls.append(body)
                if len(calls) > 1:
                    release.wait(10)
                    return
                aliases = re.findall(r'(q\d+)\s*:', body["query"])
                self.reply(dict(data=dict(Get={alias: [dict(doc_id=10, _additional=dict(id="uuid", distance=0.1))]
                                               for alias in aliases})))
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        server_thread = threading.Thread(target=server.serve_forever, daemon=True)
        server_thread.start()
        db = connect()
        result = []
        query_thread = None
        try:
            db.execute("SELECT * FROM orchid_register_source('vectors','weaviate',?)", [json.dumps(dict(
                endpoint=f"http://127.0.0.1:{server.server_port}"))]).fetchall()
            db.execute("CREATE TEMP TABLE host_source AS SELECT range::BIGINT AS id FROM range(100)")
            limited = plan()
            limited["sql"] = "SELECT source FROM hits"
            def query():
                try:
                    result.append(db.execute("SELECT * FROM __orchid_federated(?) LIMIT 1", [json.dumps(limited)]).fetchall())
                except Exception as error:
                    result.append(error)
            query_thread = threading.Thread(target=query, daemon=True)
            query_thread.start()
            query_thread.join(3)
            self.assertFalse(query_thread.is_alive(), "first batch waited for a later remote page")
            self.assertEqual(result, [[(0,)]])
            self.assertEqual(db.execute("SELECT 42").fetchone(), (42,))
        finally:
            release.set()
            if query_thread:
                query_thread.join(5)
            db.close()
            server.shutdown()
            server.server_close()
            server_thread.join()

    def test_registered_schema_compiles_remote_scan_into_host_query(self):
        seen = []
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_POST(self):
                seen.append((self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"])))))
                body = json.dumps(dict(hits=dict(total=dict(value=1, relation="eq"),
                    hits=[dict(_id="doc-10", _source=dict(doc_id=10))]))).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        db = connect()
        try:
            db.execute("CALL orchid_register_source('documents','elasticsearch',?)", [json.dumps(dict(
                endpoint=f"http://127.0.0.1:{server.server_port}"))]).fetchall()
            schema = dict(engines=dict(local=dict(dialect="duckdb")), sources=dict(documents=dict(adapter="elasticsearch")),
                execution_engine="local", tables=[dict(name="docs", engine="documents", columns=[dict(name="doc_id", data_type="int64")])],
                nodes=[dict(label="Document", table="docs", id="doc_id", properties=dict(id="doc_id"))])
            db.execute("CALL orchid_register_schema('remote_graph',?)", [json.dumps(schema)]).fetchall()
            query = "MATCH (d:Document) RETURN d.id AS id LIMIT 1"
            db.execute("EXPLAIN SELECT * FROM orchid_query('remote_graph',?)", [query]).fetchall()
            self.assertEqual(seen, [])
            self.assertEqual(db.execute("SELECT * FROM orchid_query('remote_graph',?)", [query]).fetchall(), [(10,)])
            self.assertEqual(len(seen), 1)
            self.assertIn("docs", seen[0][0])
        finally:
            db.close()
            server.shutdown()
            server.server_close()
            thread.join()
