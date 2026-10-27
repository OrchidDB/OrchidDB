from contextlib import contextmanager
import pyarrow as pa
import pytest
from orchiddb.plan import execute_plan

COL = {'name': 'id', 'data_type': 'int64', 'nullable': True}
class Source:
    dialect = 'postgres'
    def __init__(self): self.calls = 0; self.closed = False
    @contextmanager
    def query_arrow(self, work, batch_size):
        self.calls += 1
        assert work.sql == 'SELECT id FROM people'
        try:
            with pa.RecordBatchReader.from_batches(pa.schema([('id', pa.int64())]), [pa.record_batch([pa.array([1, 2])], names=['id'])]) as reader:
                yield reader
        finally:
            self.closed = True

class Tripwire:
    dialect = 'duckdb'
    def query_inputs(self, *args): raise AssertionError('DuckDB must not run')

def plan(engine='none', local=False):
    nodes = [{'kind': 'source_sql', 'source': 'db', 'dialect': 'postgres', 'sql': 'SELECT id FROM people', 'columns': [COL]}]
    if local:
        nodes.append({'kind': 'local_project', 'input': 0, 'columns': [COL], 'expressions': [{'kind': 'call', 'function': 'double', 'arguments': [{'kind': 'column', 'index': 0}], 'returns': 'int64'}]})
    return {'version': 1, 'engine': engine, 'nodes': nodes, 'result': len(nodes)-1}

@pytest.mark.parametrize('engine', ['none', 'duckdb'])
def test_direct_without_engine(engine):
    source = Source()
    with execute_plan(plan(engine), {'db': source}, {}, Tripwire()) as reader:
        assert reader.read_all().column(0).to_pylist() == [1, 2]
    assert source.calls == 1 and source.closed

def test_local_udf_and_preflight():
    source = Source()
    with pytest.raises(ValueError, match='Missing local function'):
        with execute_plan(plan(local=True), {'db': source}, {}): pass
    assert source.calls == 0
    def double(args, rows):
        assert rows == 2
        return pa.array([x.as_py()*2 for x in args[0]])
    with execute_plan(plan(local=True), {'db': source}, {'double': double}) as reader:
        assert reader.read_all().column(0).to_pylist() == [2, 4]

def test_residual_duckdb():
    import duckdb
    from orchiddb.execution import DuckDBEngine
    p = plan('duckdb')
    p['nodes'].append({'kind': 'duckdb', 'sql': 'SELECT id FROM input WHERE id > 1', 'inputs': [{'name': 'input', 'node': 0}], 'columns': [COL]})
    p['result'] = 1
    with duckdb.connect() as db:
        with execute_plan(p, {'db': Source()}, {}, DuckDBEngine(db)) as reader:
            assert reader.read_all().column(0).to_pylist() == [2]
        assert db.execute('SELECT 1').fetchone() == (1,)

def test_disabled_engine_rejects_before_source():
    p = plan()
    p['nodes'].append({'kind': 'duckdb', 'sql': 'SELECT * FROM input', 'inputs': [{'name': 'input', 'node': 0}], 'columns': [COL]})
    p['result'] = 1
    source = Source()
    with pytest.raises(ValueError, match='DuckDB'):
        with execute_plan(p, {'db': source}, {}): pass
    assert source.calls == 0
