"""Live DB-API/Arrow transport checks; compiler coverage lives in tests/trino.rs."""
import os
import pytest

from orchiddb import TrinoEngine
from orchiddb._runtime import CompiledQuery


@pytest.fixture
def engine():
    if not os.getenv('ORCHIDDB_TEST_TRINO'):
        pytest.skip('ORCHIDDB_TEST_TRINO is required')
    from trino.dbapi import connect
    connection = connect(host='127.0.0.1', port=18080, user='orchiddb-tests')
    yield TrinoEngine(connection)
    connection.close()


def compiled(sql, types):
    return CompiledQuery(version=1, dialect='trino', sql=sql, fields=(), diagnostics={'field_types': types})


def test_typed_batches_and_connection_ownership(engine):
    query = compiled("SELECT * FROM (VALUES (BIGINT '1', ARRAY[1.0,2.0]), (BIGINT '2', ARRAY[3.0,4.0])) AS t(id, embedding)", ['int64','list:float64'])
    with engine.query_arrow(query, batch_size=1) as reader:
        batches = list(reader)
    assert [b.num_rows for b in batches] == [1,1]
    assert batches[0].to_pylist() == [{'id':1, 'embedding':[1.0,2.0]}]
    with engine.query_arrow(compiled('SELECT 1 AS n', ['int64'])) as reader:
        assert reader.read_all().to_pylist() == [{'n':1}]
    cursor=engine.connection.cursor()
    cursor.execute('SELECT 2')
    assert cursor.fetchall() == [[2]]
    cursor.close()


def test_failure_releases_reader_lease(engine):
    with pytest.raises(Exception):
        with engine.query_arrow(compiled('SELECT missing_column', ['int64'])):
            pass
    with engine.query_arrow(compiled('SELECT 1 AS n', ['int64'])) as reader:
        with pytest.raises(RuntimeError, match='already active'):
            with engine.query_arrow(compiled('SELECT 2 AS n', ['int64'])):
                pass
        assert reader.read_all().num_rows == 1


def test_null_empty_and_decimal(engine):
    query=compiled("SELECT CAST(NULL AS BIGINT) AS n, CAST(ARRAY[] AS ARRAY(DOUBLE)) AS a, DECIMAL '123.45' AS d", ['int64','list:float64','decimal:5:2'])
    from decimal import Decimal
    with engine.query_arrow(query) as reader:
        assert reader.read_all().to_pylist() == [{'n':None,'a':[],'d':Decimal('123.45')}]


def test_graph_connection_search(engine):
    library=os.getenv('ORCHIDDB_NATIVE_LIBRARY')
    if not library:
        pytest.skip('ORCHIDDB_NATIVE_LIBRARY is required for the full client path')
    from orchiddb import Connection
    schema={
        'tables':[{'name':'documents','columns':[
            {'name':'id','data_type':'int64'}, {'name':'title','data_type':'string'},
            {'name':'embedding','data_type':'list:float64'}]}],
        'nodes':[{'label':'Document','table':'documents','id':'id','properties':{'title':'title','embedding':'embedding'}}],
        'source_metadata':[{'table':'documents','format':'trino','options':{
            'connector':'iceberg','catalog':'iceberg','schema':'orchid','table':'documents'}}],
    }
    with Connection(engine,schema,library=library) as graph:
        with graph.query('MATCH (d:Document) WITH d, vector.cosine_similarity($embedding,d.embedding) AS score RETURN d.title AS title,score ORDER BY score DESC', parameters={'embedding':[1.0,0.0]}) as reader:
            assert reader.read_all().to_pylist() == [{'title':'Orchid','score':1.0},{'title':'Gardening','score':0.0}]


def test_source_error_survives_failed_cancellation_and_releases_lease():
    original = RuntimeError('source connection lost')
    class Cursor:
        description = [('n',)]
        def __init__(self, fail): self.fail, self.done = fail, False
        def execute(self, sql):
            if self.fail: raise original
        def fetchmany(self, size):
            if self.done: return []
            self.done = True
            return [[1]]
        def close(self):
            if self.fail: raise RuntimeError('cancellation connection also lost')
    class Connection:
        fail = True
        def cursor(self):
            result = Cursor(self.fail)
            self.fail = False
            return result
    engine = TrinoEngine(Connection())
    with pytest.raises(RuntimeError) as caught:
        with engine.query_arrow(compiled('SELECT 1', ['int64'])): pass
    assert caught.value is original
    with engine.query_arrow(compiled('SELECT 1', ['int64'])) as reader:
        assert reader.read_all().to_pylist() == [{'n':1}]
