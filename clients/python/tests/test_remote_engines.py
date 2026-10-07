"""Live service tests share the release fixture; they never emulate HTTP responses."""
import json
import os
from pathlib import Path
import duckdb
import pytest
from orchiddb import DuckDBEngine, RemoteEngine
from orchiddb._runtime import _Runtime as Compiler
from orchiddb.federation import _query_federated as query_federated


def fixtures():
    path = os.environ.get("ORCHIDDB_REMOTE_FIXTURE")
    return json.loads(Path(path).read_text())["cases"] if path else []


@pytest.mark.parametrize("case", fixtures(), ids=lambda c: c.get("name", c["adapter"]))
def test_live_remote_federation(case):
    with Compiler() as compiler, duckdb.connect() as connection:
        for sql in case.get("setup_sql", []):
            connection.execute(sql)
        request = case["request"]
        with RemoteEngine(case["adapter"], case["endpoint"], page_size=2) as remote:
            engines = {name: remote if desc["dialect"] == case["adapter"] else DuckDBEngine(connection)
                       for name, desc in request["engines"].items()}
            with query_federated(compiler, request, engines, batch_size=2) as reader:
                table = reader.read_all()
                rows = [[table.column(j)[i].as_py() for j in range(table.num_columns)]
                        for i in range(table.num_rows)]
            assert rows == case["expected_rows"]
            # Result cleanup leaves both caller sessions usable.
            remote.clear_metadata_cache()
            assert connection.execute("SELECT 42").fetchone() == (42,)
        with pytest.raises(RuntimeError, match="closed"):
            remote.clear_metadata_cache()
        remote.close()


def test_dependent_operations_use_current_plan_and_close_source_first():
    from contextlib import contextmanager
    from orchiddb._runtime import CompiledQuery
    import pyarrow as pa
    log = []
    columns = [{"name": "id", "data_type": "int64", "nullable": False}]
    remote = dict(source_engine="db", source_dialect="duckdb", sql="SELECT input",
                  target_relation="remote", columns=columns,
                  request=dict(engine="search", input_columns=columns, template={"adapter": "quickwit"}))
    later = dict(source_engine="db", source_dialect="duckdb", sql="STALE",
                 target_relation="joined", columns=columns)
    def plan(transfers):
        return CompiledQuery("FINAL", ("id",), "duckdb", diagnostics={"execution_engine": "db", "transfers": transfers})
    class FakeCompiler:
        def prepare(self, request): return plan([remote, later])
        def bind_operation(self, p, relation, *, reader):
            assert reader.read_all().column(0).to_pylist() == [9007199254740993]
            return {"engine": "search", "requests": [{"key": 9007199254740993}]}
        def bind_arrow(self, p, relation, reader):
            reader.read_all()
            return plan([dict(later, sql="BOUND")]) if relation == "remote" else plan([])
    class Sql:
        dialect = "duckdb"
        @contextmanager
        def query_arrow(self, query, batch_size):
            log.append(query.sql)
            assert query.sql != "STALE"
            try:
                yield pa.RecordBatchReader.from_batches(pa.schema([("id", pa.int64())]),
                    [pa.record_batch([[9007199254740993]], names=["id"])])
            finally: log.append("closed " + query.sql)
    class Search:
        dialect = "quickwit"
        @contextmanager
        def execute_requests(self, requests, cols):
            assert log[-1] == "closed SELECT input"
            assert requests == [{"key": 9007199254740993}]
            try:
                yield pa.RecordBatchReader.from_batches(pa.schema([("id", pa.int64())]),
                    [pa.record_batch([[9007199254740993]], names=["id"])])
            finally: log.append("closed request")
    with query_federated(FakeCompiler(), {}, {"db": Sql(), "search": Search()}) as reader:
        assert reader.read_all().column(0).to_pylist() == [9007199254740993]
    assert log == ["SELECT input", "closed SELECT input", "closed request", "BOUND", "closed BOUND", "FINAL", "closed FINAL"]


def test_closed_request_does_not_query_sql_and_propagates_error():
    from orchiddb._runtime import CompiledQuery
    transfer = dict(source_engine="search", source_dialect="quickwit", sql="",
                    target_relation="remote", columns=[],
                    request=dict(engine="search", input_columns=[], template={"adapter": "quickwit"}))
    class FakeCompiler:
        def prepare(self, request):
            return CompiledQuery("FINAL", (), "duckdb", diagnostics={"execution_engine": "db", "transfers": [transfer]})
        def bind_operation(self, plan, relation, *, rows):
            assert rows == [[]]
            return {"engine": "search", "requests": [{}]}
    class Sql:
        dialect = "duckdb"
        def query_arrow(self, *args): raise AssertionError("SQL should not execute")
    class Search:
        dialect = "quickwit"
        def execute_requests(self, *args): raise RuntimeError("remote failed")
    with pytest.raises(RuntimeError, match="remote failed"):
        with query_federated(FakeCompiler(), {}, {"db": Sql(), "search": Search()}):
            pass


@pytest.mark.parametrize("case", [c for c in fixtures() if c.get("name", "").endswith("paged-scan")],
                         ids=lambda c: c["adapter"])
def test_live_remote_error_preserves_caller_sessions(case):
    import copy
    request = copy.deepcopy(case["request"])
    old = next(t["name"] for t in request["tables"] if t.get("engine") != request["execution_engine"])
    missing = old + "_missing"
    def replace(value):
        if isinstance(value, str): return missing if value == old else value
        if isinstance(value, list): return [replace(v) for v in value]
        if isinstance(value, dict): return {k: replace(v) for k, v in value.items()}
        return value
    with Compiler() as compiler, duckdb.connect() as connection:
        with RemoteEngine(case["adapter"], case["endpoint"]) as remote:
            engines = {name: remote if desc["dialect"] == case["adapter"] else DuckDBEngine(connection)
                       for name, desc in request["engines"].items()}
            with pytest.raises(Exception, match="404|not found|does not exist|not_found"):
                with query_federated(compiler, replace(request), engines):
                    pytest.fail("Missing index must raise, not return partial rows")
            assert connection.execute("SELECT 42").fetchone() == (42,)
            with query_federated(compiler, case["request"], engines) as reader:
                assert reader.read_all().num_rows == len(case["expected_rows"])


@pytest.mark.parametrize("case", [c for c in fixtures() if c.get("name", "").endswith("correlated-bm25")],
                         ids=lambda c: c["adapter"])
def test_live_remote_correlation_preserves_large_integer(case):
    import copy
    case = copy.deepcopy(case)
    identifier = 9007199254740993
    case["setup_sql"].append(f"UPDATE queries SET id={identifier} WHERE id=10")
    case["request"]["query"] = case["request"]["query"].replace("RETURN q.id,", "RETURN toString(q.id),")
    expected = [[identifier if row[0] == 10 else row[0], *row[1:]] for row in case["expected_rows"]]
    expected.sort(key=lambda row: (row[0], row[1]))
    case["expected_rows"] = [[str(row[0]), *row[1:]] for row in expected]
    test_live_remote_federation(case)
