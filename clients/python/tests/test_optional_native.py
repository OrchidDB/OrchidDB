"""End-to-end source plans through the shared native runtime.

DuckDB below is a test fixture for an application-owned SQL source. The Orchid
native library is built without DuckDB; completion uses a separate lazy adapter.
"""
import os
from contextlib import contextmanager
import pytest
import pyarrow as pa
import duckdb
from orchiddb import Connection, DuckDBEngine, LazyDuckDBEngine

pytestmark = pytest.mark.skipif(not os.environ.get('ORCHIDDB_NATIVE_LIBRARY'), reason='requires local native build')
SCHEMA = {
    'sources': {'db': {'adapter': 'postgres'}},
    'tables': [{'name': 'people', 'source': 'db', 'columns': [{'name': 'id', 'data_type': 'int64'}]}],
    'nodes': [{'label': 'Person', 'table': 'people', 'id': 'id', 'properties': {'id': 'id'}}],
    'functions': [{'name': 'twice', 'target': 'twice', 'parameters': ['int64'], 'returns': 'int64', 'local': True}],
}
class Source:
    dialect = 'postgres'
    def __init__(self, db): self.db = db
    @contextmanager
    def query_arrow(self, work, batch_size):
        assert 'twice' not in work.sql
        with self.db.sql(work.sql).to_arrow_reader(batch_size) as reader:
            yield reader

def twice(args, rows):
    return pa.array([None if v.as_py() is None else v.as_py()*2 for v in args[0]], type=pa.int64())

@pytest.mark.parametrize('engine', ['none', 'duckdb'])
def test_compiled_local_pipeline_skips_completion(engine):
    def forbidden(): pytest.fail('completion factory invoked')
    with duckdb.connect() as source:
        source.execute('CREATE TABLE people(id BIGINT); INSERT INTO people VALUES (1),(2)')
        with Connection(engine, SCHEMA, sources={'db': Source(source)}, functions={'twice': twice}, duckdb=LazyDuckDBEngine(forbidden)) as graph:
            with graph.query('MATCH (p:Person) RETURN twice(p.id) AS value') as result:
                assert sorted(result.read_all().column(0).to_pylist()) == [2, 4]

def test_compiled_local_predicate_uses_lazy_completion():
    created = []
    with duckdb.connect() as source, duckdb.connect() as completion:
        source.execute('CREATE TABLE people(id BIGINT); INSERT INTO people VALUES (1),(2)')
        def factory():
            created.append(True)
            return DuckDBEngine(completion)
        with Connection('duckdb', SCHEMA, sources={'db': Source(source)}, functions={'twice': twice}, duckdb=LazyDuckDBEngine(factory)) as graph:
            assert created == []
            with graph.query('MATCH (p:Person) WHERE twice(p.id) > 2 RETURN p.id AS id') as result:
                assert result.read_all().column(0).to_pylist() == [2]
            assert len(created) == 1
            assert completion.execute('SELECT 1').fetchone() == (1,)
