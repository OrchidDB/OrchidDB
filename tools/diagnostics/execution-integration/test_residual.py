"""One embedding harness: real Trino/OPA -> local UDF -> caller DuckDB."""
from contextlib import contextmanager
import os
from pathlib import Path
import sys
import unittest
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/trino'))
import duckdb
import pyarrow as pa
from orchiddb import Connection, DuckDBEngine, LazyDuckDBEngine
from orchiddb.execution import _logical_arrow_type
from query import query
TABLE = 'postgresql.public.documents'
SCHEMA = {
    'sources': {'trino': {'adapter': 'trino'}},
    'tables': [{'name': TABLE, 'source': 'trino', 'columns': [{'name': 'id', 'data_type': 'int64'}]}],
    'nodes': [{'label': 'Doc', 'table': TABLE, 'id': 'id', 'properties': {'id': 'id'}}],
    'functions': [{'name': 'twice', 'target': 'twice', 'parameters': ['int64'], 'returns': 'int64', 'local': True}],
}
class Source:
    dialect = 'trino'
    def __init__(self): self.calls = []
    @contextmanager
    def query_arrow(self, work, batch_size):
        self.calls.append(work.sql)
        if 'twice' in work.sql:
            raise AssertionError('local function was sent to Trino')
        values = query(work.sql, os.environ['ORCHIDDB_TEST_OPA_URL'], user='alice')
        schema = pa.schema([pa.field(name, _logical_arrow_type(ty, pa))
            for name, ty in zip(work.fields, work.diagnostics['field_types'])])
        with pa.RecordBatchReader.from_batches(schema, pa.Table.from_pylist(values, schema=schema).to_batches(max_chunksize=batch_size)) as reader:
            yield reader
class ResidualTests(unittest.TestCase):
    def test_residual_uses_duckdb_once_after_authorized_source(self):
        source, created, udf_rows = Source(), [], []
        def twice(args, count):
            values = args[0].to_pylist()
            udf_rows.extend(values)
            return pa.array([None if n is None else n * 2 for n in values], type=pa.int64())
        with duckdb.connect() as completion:
            def factory():
                created.append(True)
                return DuckDBEngine(completion)
            with Connection('duckdb', SCHEMA, sources={'trino': source}, functions={'twice': twice}, duckdb=LazyDuckDBEngine(factory)) as graph:
                with graph.query('MATCH (d:Doc) WHERE twice(d.id) > 2 RETURN d.id AS id') as result:
                    self.assertEqual(result.read_all().column(0).to_pylist(), [3])
            self.assertEqual(created, [True])
            self.assertEqual(len(source.calls), 1)
            self.assertEqual(sorted(udf_rows), [1, 3])
            self.assertEqual(completion.execute('SELECT 42').fetchone(), (42,))
    def test_none_rejects_residual_before_source_or_udf_calls(self):
        source = Source()
        def forbidden(*args): self.fail('preflight must prevent UDF execution')
        with Connection('none', SCHEMA, sources={'trino': source}, functions={'twice': forbidden}) as graph:
            with self.assertRaisesRegex(Exception, 'DuckDB|duckdb|relational'):
                with graph.query('MATCH (d:Doc) WHERE twice(d.id) > 2 RETURN d.id AS id') as result:
                    result.read_all()
        self.assertEqual(source.calls, [])
if __name__ == '__main__':
    unittest.main()
