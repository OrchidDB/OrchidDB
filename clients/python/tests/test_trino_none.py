"""Live source-only regressions using query-local fixtures, never shared tables."""
import json
import os
import sys
from dataclasses import replace
from urllib.parse import urlparse

import pytest

FIXTURE = """people(id,name,quantity,amount,created,payload) AS (VALUES
 (BIGINT '9007199254740993','é','1_0.5',DECIMAL '12345678901234567890.12',TIMESTAMP '2026-01-02 03:04:05.123456 UTC',X'0001ff'),
 (BIGINT '2',CAST(NULL AS VARCHAR),'bad',CAST(NULL AS DECIMAL(22,2)),CAST(NULL AS TIMESTAMP(6) WITH TIME ZONE),CAST(NULL AS VARBINARY)),
 (BIGINT '3','é',CAST(NULL AS VARCHAR),DECIMAL '-0.01',TIMESTAMP '2026-01-02 03:04:05.123457 UTC',X'')),
 links(id,src,dst) AS (VALUES (BIGINT '1',BIGINT '9007199254740993',BIGINT '2'),
 (BIGINT '2',BIGINT '2',BIGINT '3'),(BIGINT '3',BIGINT '3',BIGINT '9007199254740993'))"""

def fixture_sql(sql):
    for prefix in ('WITH RECURSIVE ', 'WITH '):
        offset = sql.find(prefix) if prefix == 'WITH RECURSIVE ' else (0 if sql.startswith(prefix) else -1)
        if offset >= 0:
            return sql[:offset] + prefix + FIXTURE + ', ' + sql[offset+len(prefix):]
    return 'WITH ' + FIXTURE + ' ' + sql


@pytest.fixture
def connection():
    if not os.getenv('ORCHIDDB_TEST_TRINO_NONE'):
        pytest.skip('ORCHIDDB_TEST_TRINO_NONE is required')
    from orchiddb import Connection, TrinoSource
    from trino.dbapi import connect
    import requests
    endpoint = urlparse(os.getenv('ORCHIDDB_TEST_TRINO_URL', 'http://127.0.0.1:18080'))
    user = os.getenv('ORCHIDDB_TEST_TRINO_USER', 'orchiddb-tests')
    requests_seen = []
    class Transport(requests.Session):
        def send(self, request, **kwargs):
            requests_seen.append(request.headers.get('X-Trino-User'))
            return super().send(request, **kwargs)
    transport = Transport()
    db = connect(host=endpoint.hostname, port=endpoint.port, user=user,
                 http_scheme=endpoint.scheme, http_session=transport, timezone='UTC')
    class Source(TrinoSource):
        def query_arrow(self, query, batch_size=65536):
            return super().query_arrow(replace(query, sql=fixture_sql(query.sql)), batch_size)
    class Tripwire:
        dialect = 'duckdb'
        def query_inputs(self, *args, **kwargs):
            raise AssertionError('DuckDB completion attempted')
    columns = [('id','int64'),('name','string'),('quantity','string'),('amount','decimal:22:2'),
               ('created','timestamptz'),('payload','binary')]
    schema = {'native_values': True, 'sources': {'db': {'adapter':'trino'}}, 'tables':[
        {'name':'people','source':'db','columns':[{'name':n,'data_type':t} for n,t in columns]},
        {'name':'links','source':'db','columns':[{'name':n,'data_type':'int64'} for n in ['id','src','dst']]}],
        'nodes':[{'label':'Person','table':'people','id':'id','properties':{n:n for n,t in columns}}],
        'edges':[{'label':'LINK','table':'links','id':['id','src','dst'],'source_label':'Person',
                  'target_label':'Person','source':'src','target':'dst','properties':{}}]}
    with Connection('none', schema, sources={'db': Source(db)}, duckdb=Tripwire(),
                    library=os.environ['ORCHIDDB_NATIVE_LIBRARY']) as graph:
        yield graph, db
    db.close()
    transport.close()
    assert all(identity == user for identity in requests_seen)
    assert "duckdb" not in sys.modules


def rows(graph, query, **kwargs):
    with graph.query(query, batch_size=1, **kwargs) as reader:
        return [list(row.values()) for batch in reader for row in batch.to_pylist()]


def reference(db, sql):
    cursor = db.cursor()
    try:
        cursor.execute(fixture_sql(sql))
        return cursor.fetchall()
    finally:
        cursor.close()


def test_scalar_semantics_and_temporal_precision(connection):
    graph, db = connection
    assert rows(graph, 'MATCH(p:Person) RETURN p.name,count(p) AS n ORDER BY p.name') == reference(
        db, 'SELECT name,count(*) FROM people GROUP BY name ORDER BY name NULLS LAST')
    assert rows(graph, 'MATCH(p:Person) RETURN sum(toFloat(p.quantity)) AS n') == [[10.5]]
    assert rows(graph, 'MATCH(p:Person) WHERE p.created<datetime($cutoff) RETURN p.id',
                parameters={'cutoff':'2026-01-02T03:04:05.123457Z'}) == [[9007199254740993]]
    assert rows(graph, 'MATCH(p:Person) WHERE p.name=$name RETURN count(p)',
                parameters={'name':"é' OR 1=1 --"}) == [[0]]


def test_graph_descriptor_preserves_exact_property_types(connection):
    graph, _ = connection
    actual = rows(graph, 'MATCH(p:Person {id:$id}) RETURN p', parameters={'id':9007199254740993})[0][0]
    props = [('id','int64'),('name','string'),('quantity','string'),('amount','decimal:22:2'),
             ('created','timestamptz'),('payload','binary')]
    ty = 'struct_fields:' + json.dumps([('kind','string'),('id','int64'),('label','string'),
        ('properties','struct_fields:'+json.dumps(props)),('source_keys','boolean'),('id_type','string')])
    expected = graph._runtime.operation_command({'op':'decode_graph_values','data_type':ty,'values':[{
        'kind':'node','id':'9007199254740993','label':'Person','source_keys':True,'id_type':'Int64',
        'properties':{'id':'9007199254740993','name':'é','quantity':'1_0.5',
            'amount':'12345678901234567890.12','created':'2026-01-02T03:04:05.123456Z','payload':'\\x0001ff'}}]})[0]
    assert actual == expected


def test_traversal_multiplicity_and_zero_hop(connection):
    graph, db = connection
    for bound, include_self in [('1..2',False),('0..2',True)]:
        expected = ('SELECT id,id FROM people UNION ALL ' if include_self else '') + '''
          SELECT src,dst FROM links UNION ALL
          SELECT a.src,b.dst FROM links a JOIN links b ON a.dst=b.src WHERE a.id<>b.id'''
        expected = reference(db, 'SELECT * FROM ('+expected+') t ORDER BY 1,2')
        actual = rows(graph, 'MATCH(a:Person)-[:LINK*'+bound+']->(b:Person) RETURN a.id,b.id ORDER BY a.id,b.id')
        assert actual == expected


def test_early_close_and_error_recovery(connection):
    graph, db = connection
    with graph.query('MATCH(p:Person) RETURN p', batch_size=1) as reader:
        assert reader.read_next_batch().num_rows == 1
    cursor = db.cursor()
    try:
        with pytest.raises(Exception): cursor.execute('SELECT nonexistent_column')
    finally:
        cursor.close()
    assert rows(graph, 'MATCH(p:Person) RETURN count(p)') == [[3]]


def test_whole_paths_and_composite_edges(connection):
    graph, _ = connection
    # Each source has a single outgoing edge: 0, 1 and 2 hops yield nine paths.
    paths = rows(graph, 'MATCH p=(a:Person)-[:LINK*0..2]->(b:Person) RETURN p')
    assert len(paths) == 9
    assert len({row[0]['__orchiddb_value_v1'] for row in paths}) == 9
    edges = rows(graph, 'MATCH(a:Person)-[e:LINK]->(b:Person) RETURN e')
    assert len(edges) == 3
    assert len({row[0]['__orchiddb_value_v1'] for row in edges}) == 3
    assert rows(graph, 'MATCH(p:Person) WHERE p.id=-1 RETURN p') == []
    null_node = rows(graph, 'MATCH(a:Person {id:2}) OPTIONAL MATCH(a)-[:LINK]->(b:Person {id:-1}) RETURN b')
    assert null_node == [[None]]


def test_unbounded_traversal_is_rejected_before_source_execution(connection):
    graph, _ = connection
    with pytest.raises(Exception, match='unbounded traversal'):
        rows(graph, 'MATCH(a:Person)-[:LINK*]->(b:Person) RETURN a.id,b.id LIMIT 1')
