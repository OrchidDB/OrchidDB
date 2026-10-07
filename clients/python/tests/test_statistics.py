from contextlib import contextmanager
import base64
import pyarrow as pa
import pytest
from orchiddb import Compiler


class Coordinator:
    def __init__(self):
        self.calls = []
        self.done = False
    def __call__(self, command):
        self.calls.append(command)
        op = command['op']
        task = dict(id='scan', source='people', kind='sample', sql='SELECT id FROM people LIMIT 2', dialect='duckdb', max_rows=2, max_bytes=65536, timeout_ms=1000)
        if op == 'begin':
            return dict(id='analysis', request=task)
        if op == 'submit':
            if command.get('done', True):
                return dict(id='analysis', request=None)
            return dict(id='analysis', request=task)
        if op == 'finish':
            return dict(catalog_id='new', snapshot={'version': 1}, report={'complete': True})
        if op == 'install':
            return dict(catalog_id='loaded')
        return {}


def compiler():
    c = Compiler.__new__(Compiler)
    c._catalog_id = 'old'
    c.statistics_snapshot = {'old': True}
    c.statistics_report = None
    c.statistics_command = Coordinator()
    return c


def test_driver_sends_arrow_caps_rows_closes_and_replaces(tmp_path):
    c = compiler()
    class Engine:
        dialect = 'duckdb'
        closed = False
        @contextmanager
        def statistics_arrow(self, request):
            try:
                yield [pa.record_batch({'id': [1, 2, 3]})]
            finally:
                self.closed = True
    engine = Engine()
    c.generate_statistics({}, engine)
    batch_command = next(x for x in c.statistics_command.calls if 'ipc' in x)
    table = pa.ipc.open_stream(base64.b64decode(batch_command['ipc'])).read_all()
    assert table.to_pylist() == [{'id': 1}, {'id': 2}]
    assert engine.closed
    assert c._catalog_id == 'new'
    assert c.statistics_command.calls[-1] == dict(op='release', catalog_id='old')
    path = tmp_path / 'stats.json'
    c.save_statistics(path)
    c.load_statistics(path)
    assert c._catalog_id == 'loaded'
    c.clear_statistics()
    assert c.statistics_snapshot is None


def test_unsupported_adapter_reports_failure_without_query():
    c = compiler()
    class Engine:
        dialect = 'duckdb'
        def query_arrow(self, *args):
            pytest.fail('Must not execute an unbounded fallback')
    c.generate_statistics({}, Engine())
    error = next(x['error'] for x in c.statistics_command.calls if 'error' in x)
    assert 'bounded' in error


def test_interruption_cancels_analysis_preserving_catalog():
    c = compiler()
    class Engine:
        dialect = 'duckdb'
        @contextmanager
        def statistics_arrow(self, request):
            raise KeyboardInterrupt()
            yield
    with pytest.raises(KeyboardInterrupt):
        c.generate_statistics({}, Engine())
    assert c._catalog_id == 'old'
    assert c.statistics_command.calls[-1] == dict(op='cancel', id='analysis')


def test_duckdb_deadline_interrupts_scan_and_releases_connection():
    import duckdb
    from orchiddb import DuckDBEngine
    connection = duckdb.connect()
    engine = DuckDBEngine(connection)
    try:
        with pytest.raises(duckdb.InterruptException):
            with engine.statistics_arrow(dict(sql='SELECT sum(i) FROM range(10000000000) t(i)', timeout_ms=10, max_rows=2)) as reader:
                reader.read_all()
        assert connection.execute('SELECT 42').fetchone() == (42,)
        with engine.statistics_arrow(dict(sql='SELECT 1', timeout_ms=1000, max_rows=2)) as reader:
            assert reader.read_all().num_rows == 1
    finally:
        connection.close()
