"""Real engine matrix. Set ORCHIDDB_TEST_PG_URL to enable PostgreSQL coverage."""
import copy
import os
from decimal import Decimal
import pytest
import duckdb
from orchiddb import Compiler, DuckDBEngine, PostgresEngine, query_federated


def request(target='d', people='p', links='d'):
    return dict(version=1, dialect='postgres' if target == 'p' else 'duckdb',
        language='cypher', query='MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 25 RETURN a.name AS a, b.name AS b ORDER BY a, b',
        engines={'p':{'dialect':'postgres'},'d':{'dialect':'duckdb'}},execution_engine=target,
        tables=[dict(name='people',engine=people,columns=[dict(name=n,data_type=t) for n,t in [('id','int64'),('name','string'),('age','int64'),('score','float64'),('tags','list:string')]]),
                dict(name='links',engine=links,columns=[dict(name=n,data_type='int64') for n in ['id','src','dst']])],
        nodes=[dict(label='Person',table='people',id='id',properties={n:n for n in ['name','age','score','tags']})],
        edges=[dict(label='KNOWS',table='links',id='id',source='src',target='dst',source_label='Person',target_label='Person')])

@pytest.fixture
def engines():
    url=os.getenv('ORCHIDDB_TEST_PG_URL')
    if not url: pytest.skip('ORCHIDDB_TEST_PG_URL is required for real PostgreSQL tests')
    import psycopg
    p=psycopg.connect(url,autocommit=True)
    d=duckdb.connect()
    for conn in [p,d]:
        conn.execute('CREATE TEMPORARY TABLE people(id BIGINT, name VARCHAR, age BIGINT, score DOUBLE PRECISION, tags VARCHAR[])')
        conn.execute("INSERT INTO people VALUES (1,'Ada',30,1.25,ARRAY['x','y']),(2,'Bob',20,2.5,ARRAY['y']),(3,'Cy',NULL,NULL,NULL)")
        conn.execute('CREATE TEMPORARY TABLE links(id BIGINT, src BIGINT, dst BIGINT)')
        conn.execute('INSERT INTO links VALUES (1,1,2),(2,2,3),(3,1,3)')
    yield {'p':PostgresEngine(p),'d':DuckDBEngine(d)}
    p.close();d.close()

@pytest.fixture
def compiler():return Compiler()

def rows(compiler, r, engines):
    with query_federated(compiler,r,engines,batch_size=2) as result:
        return [list(row.values()) for row in result.read_all().to_pylist()]

@pytest.mark.parametrize('target,people,links',[('d','d','d'),('p','p','p'),('d','p','d'),('p','d','p'),('d','p','p'),('p','d','d')])
@pytest.mark.parametrize('language',['cypher','gremlin','sparql'])
def test_language_matrix(compiler,engines,target,people,links,language):
    r=request(target,people,links);r['language']=language
    expected=[['Ada','Bob'],['Ada','Cy']]
    if language=='gremlin':
        r['query']="g.V().hasLabel('Person').has('age',gt(25)).out('KNOWS').values('name').order()"
        expected=[['Bob'],['Cy']]
    if language=='sparql':
        r['ontology']={'classes':[{'iri':'urn:Person','label':'Person'}], 'properties':[{'iri':'urn:name','label':'Person','property':'name'},{'iri':'urn:age','label':'Person','property':'age'}], 'relationships':[{'iri':'urn:knows','label':'KNOWS','source_label':'Person','target_label':'Person'}]}
        r['query']='SELECT ?a ?b WHERE { ?x <urn:age> ?age; <urn:name> ?a; <urn:knows> ?y . ?y <urn:name> ?b . FILTER(?age > 25) } ORDER BY ?a ?b'
    assert rows(compiler,r,engines)==expected

@pytest.mark.parametrize('target',['d','p'])
@pytest.mark.parametrize('expression',[
    'toUpper(p.name)', 'toLower(p.name)', 'substring(p.name,1,2)', 'left(p.name,1)', 'right(p.name,1)',
    'size(p.tags)', 'p.tags[0]', 'p.tags[-1]', 'p.tags[0..1]', "'x' IN p.tags",
    'round(p.score,1)', 'floor(p.score)', 'ceil(p.score)', 'abs(p.score)', 'sqrt(p.score)', 'log(p.score)',
    "p.name STARTS WITH 'A'", "p.name ENDS WITH 'a'", "p.name CONTAINS 'd'", "p.name =~ 'A.*'",
    'coalesce(p.age,0)', 'toString(p.age)', 'p.age + 2', 'p.age / 2.0',
    'p.age / 2', 'p.age % 4', 'p.age ^ 2', 'size(p.name)', 'reverse(p.name)',
    "replace(p.name,'a','X')", 'trim(p.name)', 'ltrim(p.name)', 'rtrim(p.name)',
])
def test_operators(compiler,engines,target,expression):
    r=request(target,target,target);r['query']=f'MATCH (p:Person) RETURN {expression} AS v ORDER BY p.id'
    # p.id is not a mapped property: order by the stable name instead.
    r['query']=r['query'].replace('ORDER BY p.id','ORDER BY p.name')
    baseline=copy.deepcopy(r);baseline['dialect']='duckdb';baseline['execution_engine']='d'
    for t in baseline['tables']:t['engine']='d'
    assert rows(compiler,r,engines)==rows(compiler,baseline,engines)

@pytest.mark.parametrize('target,source',[('d','p'),('p','d')])
def test_pushdown_and_cleanup(compiler,engines,target,source):
    r=request(target,source,target)
    plan=compiler.compile(r)
    assert any('WHERE' in t['sql'] and '25' in t['sql'] for t in plan.diagnostics['transfers'])
    assert all('score' not in t['sql'] and 'tags' not in t['sql'] for t in plan.diagnostics['transfers'])
    assert rows(compiler,r,engines)==[['Ada','Bob'],['Ada','Cy']]
    r['query']='MATCH (p:Person) WHERE p.age > 25 RETURN count(p) AS n'
    plan=compiler.compile(r)
    assert len(plan.diagnostics['transfers'])==1
    assert 'count(' in plan.diagnostics['transfers'][0]['sql'].lower()
    assert rows(compiler,r,engines)==[[1]]
    with pytest.raises(RuntimeError,match='consumer failed'):
        with query_federated(compiler,r,engines) as result:
            raise RuntimeError('consumer failed')
    conn=engines[target].connection
    sql="SELECT table_name FROM information_schema.tables WHERE table_name LIKE '__orchiddb_exchange_%'"
    assert conn.execute(sql).fetchall()==[]

@pytest.mark.parametrize('target',['d','p'])
def test_declared_operator_mapping(compiler,engines,target):
    r=request(target,target,target);r['query']='MATCH (p:Person) RETURN shout(p.name) AS s ORDER BY p.name'
    r['functions']=[dict(name='shout',target='upper',parameters=['string'],returns='string')]
    assert rows(compiler,r,engines)==[['ADA'],['BOB'],['CY']]

@pytest.mark.parametrize('target,source',[('d','p'),('p','d')])
def test_lists_empty_results_and_nulls_cross_exchange(compiler,engines,target,source):
    r=request(target,source,target)
    r['query']='MATCH (p:Person) RETURN p.name AS name, p.tags AS tags ORDER BY p.name'
    assert rows(compiler,r,engines)==[['Ada',['x','y']],['Bob',['y']],['Cy',None]]
    r['query']='MATCH (p:Person) WHERE p.age > 100 RETURN p.name AS name'
    assert rows(compiler,r,engines)==[]


def test_postgres_error_preserves_caller_transaction_and_creates_no_exchanges(compiler,engines):
    pg=engines['p'].connection
    pg.execute("CREATE FUNCTION pg_temp.fail(text) RETURNS text LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'deliberate test failure'; END $$")
    pg.autocommit=False
    pg.execute("INSERT INTO people VALUES (99,'uncommitted',1,1,NULL)")
    r=request('p','d','p');r['query']='MATCH (p:Person) RETURN fail(p.name) AS n'
    r['functions']=[dict(name='fail',target='pg_temp.fail',parameters=['string'],returns='string')]
    with pytest.raises(Exception,match='deliberate test failure'):
        rows(compiler,r,engines)
    assert pg.execute("SELECT count(*) FROM people WHERE id=99").fetchone()==(1,)
    assert pg.execute("SELECT table_name FROM information_schema.tables WHERE table_name LIKE '__orchiddb_exchange_%'").fetchall()==[]
    pg.rollback()
    assert pg.execute("SELECT count(*) FROM people WHERE id=99").fetchone()==(0,)
    pg.rollback()

@pytest.mark.parametrize('target',['d','p'])
@pytest.mark.parametrize('suffix,expected',[("length()",[[5],[3],[2]]),("substring(1,3)",[['da'],['ob'],['y']]),("substring(3,5)",[['😀'],[''],['']])])
def test_gremlin_utf16_operators(compiler,engines,target,suffix,expected):
    # The supplementary character takes two UTF-16 units.
    for engine in engines.values():
        engine.connection.execute("UPDATE people SET name = name || '😀' WHERE id=1")
    r=request(target,target,target);r['language']='gremlin'
    r['query']="g.V().hasLabel('Person').order().by('name').values('name')."+suffix
    assert rows(compiler,r,engines)==expected
    # Compare the same dynamic expression on both dialects.
    baseline=copy.deepcopy(r);baseline['dialect']='duckdb';baseline['execution_engine']='d'
    for t in baseline['tables']:t['engine']='d'
    assert rows(compiler,r,engines)==rows(compiler,baseline,engines)

@pytest.mark.parametrize('target,source',[('d','p'),('p','d'),('p','p')])
def test_binary_identity_preserves_bytes(compiler,engines,target,source):
    data=b"a\\b'\xf0\x9f\x98\x80"
    for engine in engines.values():
        engine.connection.execute('CREATE TEMP TABLE binary_people(id '+('BYTEA' if engine.dialect=='postgres' else 'BLOB')+', name VARCHAR)')
        engine.connection.execute('INSERT INTO binary_people VALUES ('+('%s,%s' if engine.dialect=='postgres' else '?,?')+')',(data,'binary'))
    r=request(target,source,source)
    r['tables']=[dict(name='binary_people',engine=source,columns=[dict(name='id',data_type='binary'),dict(name='name',data_type='string')])]
    r['nodes']=[dict(label='Blob',table='binary_people',id='id',properties={'id':'id','name':'name'})];r['edges']=[]
    r['query']='MATCH (b:Blob) RETURN b.id AS id'
    assert rows(compiler,r,engines)==[[data]]
    r['language']='sparql';r['ontology']={'classes':[dict(iri='urn:Blob',label='Blob')],'properties':[dict(iri='urn:name',label='Blob',property='name')]}
    r['query']='SELECT ?s WHERE { ?s <urn:name> "binary" }'
    assert rows(compiler,r,engines)==[['urn:orchiddb:Blob:'+data.hex()]]

class ReadOnlyConnection:
    """Reject database setup during execution, after caller-owned fixture setup."""
    def __init__(self, connection):
        self.connection = connection
        self.statements = []

    def __getattr__(self, name):
        if name in ('register', 'unregister', 'copy'):
            raise AssertionError('Query execution must not register database objects or copy into tables')
        return getattr(self.connection, name)

    def execute(self, sql, *args, **kwargs):
        assert sql.lstrip().split()[0].upper() in ('SELECT', 'WITH'), sql
        self.statements.append(sql)
        self.connection.execute(sql, *args, **kwargs)
        return self

    def cursor(self, *args, **kwargs):
        return ReadOnlyConnection(self.connection.cursor(*args, **kwargs))

    def __enter__(self):
        self.connection.__enter__()
        return self

    def __exit__(self, *args):
        return self.connection.__exit__(*args)


@pytest.mark.parametrize('target,source',[('d','p'),('p','d')])
def test_mixed_execution_issues_only_read_queries(compiler, engines, target, source):
    for engine in engines.values():
        engine.connection = ReadOnlyConnection(engine.connection)
    r = request(target, source, target)
    assert rows(compiler, r, engines) == [['Ada', 'Bob'], ['Ada', 'Cy']]


@pytest.mark.parametrize('target,source',[('d','p'),('p','d')])
def test_bound_strings_are_data(compiler, engines, target, source):
    value = "a\\'b''c😀'); DROP TABLE people; --"
    for engine in engines.values():
        placeholder = '%s' if engine.dialect == 'postgres' else '?'
        engine.connection.execute(f'UPDATE people SET name={placeholder} WHERE id=1', (value,))
    r = request(target, source, target)
    r['query'] = 'MATCH (p:Person) WHERE p.age > 25 RETURN p.name AS name'
    assert rows(compiler, r, engines) == [[value]]

@pytest.mark.parametrize('target,source',[('d','p'),('p','d')])
def test_exchange_preserves_decimal_date_timestamp_and_empty_lists(compiler, engines, target, source):
    import datetime
    values = (1, Decimal('12345678901234567890.123456'), datetime.date(2024, 2, 29),
              datetime.datetime(2024, 2, 29, 12, 34, 56, 123456), [], [Decimal("12345678901234567890.123456"), None], datetime.time(12,34,56,123456))
    for engine in engines.values():
        engine.connection.execute('CREATE TEMP TABLE typed_values(id BIGINT, amount DECIMAL(30,6), day DATE, moment TIMESTAMP, items BIGINT[], amounts DECIMAL(30,6)[], clock TIME)')
        placeholders = ','.join(['%s' if engine.dialect == 'postgres' else '?'] * 7)
        engine.connection.execute(f'INSERT INTO typed_values VALUES ({placeholders})', values)
    r = request(target, source, source)
    r['tables'] = [dict(name='typed_values', engine=source, columns=[dict(name=n, data_type=t) for n,t in [
        ('id','int64'), ('amount','decimal:30:6'), ('day','date'), ('moment','timestamp'), ('items','list:int64'), ('amounts','list:decimal:30:6'), ('clock','time')]])]
    r['nodes'] = [dict(label='Typed',table='typed_values',id='id',properties={n:n for n in ['amount','day','moment','items','amounts','clock']})]
    r['edges'] = []
    r['query'] = 'MATCH (v:Typed) RETURN v.amount AS amount, v.day AS day, v.moment AS moment, v.items AS items, v.amounts AS amounts, v.clock AS clock'
    assert rows(compiler,r,engines) == [list(values[1:])]

@pytest.mark.parametrize('suffix,values', [
    ('asNumber()', ['1e999999999999999999999', '1e-999999999999999999999', 'invalid']),
    ('asNumber(GType.DOUBLE)', ['1.25', '-1e999', '-1e-999']),
    ('asNumber(GType.FLOAT)', ['1e39', '1e-60', '1.25']),
    ('asNumber(GType.LONG)', ['9223372036854775807', '-9223372036854775808', '0' * 200000 + '1']),
    ('asNumber(GType.BIGDECIMAL)', ['1.234567', '-1.234567', '0' * 200000 + '1']),
])
def test_safe_numeric_casts_preserve_overflow_and_underflow(compiler, engines, suffix, values):
    for engine in engines.values():
        placeholder = '%s' if engine.dialect == 'postgres' else '?'
        for index, value in enumerate(values, 1):
            engine.connection.execute(f'UPDATE people SET name={placeholder}, age={index} WHERE id={index}', (value,))
    results = []
    for target in ['d', 'p']:
        r = request(target, target, target)
        r['language'] = 'gremlin'
        r['query'] = "g.V().hasLabel('Person').order().by('age').values('name')." + suffix
        results.append(rows(compiler, r, engines))
    assert results[1] == results[0]

@pytest.mark.parametrize('target,source', [('d','p'), ('p','d'), ('p','p')])
def test_nested_lists_remain_ragged_and_preserve_null_children(compiler, engines, target, source):
    from psycopg.types.json import Jsonb
    values = [[1], [2, 3], None, []]
    for name, engine in engines.items():
        column_type = 'JSONB[]' if name == 'p' else 'BIGINT[][]'
        engine.connection.execute(f'CREATE TEMP TABLE nested_values(id BIGINT, items {column_type})')
        placeholder = '%s' if name == 'p' else '?'
        payload = [None if v is None else Jsonb(v) for v in values] if name == 'p' else values
        engine.connection.execute(f'INSERT INTO nested_values VALUES (1,{placeholder})', (payload,))
    r = request(target, source, source)
    r['tables'] = [dict(name='nested_values', engine=source, columns=[dict(name='id', data_type='int64'), dict(name='items',data_type='list:list:int64')])]
    r['nodes'] = [dict(label='Nested',table='nested_values',id='id',properties={'items':'items'})]
    r['edges'] = []
    r['query'] = 'MATCH (n:Nested) RETURN n.items AS items, n.items[1] AS second, n.items[2] AS null_child, n.items[3] AS empty_child, size(n.items) AS size'
    assert rows(compiler, r, engines) == [[values, [2, 3], None, [], 4]]
    r['query'] = 'MATCH (n:Nested) UNWIND n.items AS child RETURN child AS child, size(child) AS size ORDER BY size'
    assert rows(compiler, r, engines) == [[[], 0], [[1], 1], [[2, 3], 2], [None, None]]


@pytest.mark.parametrize('query,expected', [
    ('RETURN [[],[]] AS xs', [[[], []]]),
    ('RETURN [[null],[null,null]] AS xs', [[[None], [None, None]]]),
    ('RETURN [[[1]],[[2],[3,4]]] AS xs', [[[[1]], [[2], [3,4]]]]),
])
def test_nested_constant_lists_match_duckdb(compiler, engines, query, expected):
    for target in ['d', 'p']:
        r = request(target, target, target)
        r['query'] = query
        assert rows(compiler, r, engines) == [expected]

@pytest.mark.parametrize('target,source', [('d','p'), ('p','d'), ('p','p')])
@pytest.mark.parametrize('kind', ['binary','date','time','decimal'])
def test_nested_typed_values_match_across_engines(compiler, engines, target, source, kind):
    import datetime
    value = {'binary': b"\x00\xff'\\", 'date': datetime.date(2024,2,29),
             'time': datetime.time(12,34,56,123456), 'decimal': Decimal('12345678901234567890.123456')}[kind]
    data_type = 'decimal:30:6' if kind == 'decimal' else kind
    values = [[value, None], [None, value]]
    for name, engine in engines.items():
        sql_type = {'binary': 'BYTEA' if name=='p' else 'BLOB', 'date':'DATE', 'time':'TIME', 'decimal':'DECIMAL(30,6)'}[kind]
        engine.connection.execute(f'CREATE TEMP TABLE nested_typed(id BIGINT, items {sql_type}[][])')
        placeholder = '%s' if name=='p' else '?'
        engine.connection.execute(f'INSERT INTO nested_typed VALUES (1,{placeholder})', (values,))
    r = request(target, source, source)
    r['tables'] = [dict(name='nested_typed',engine=source,columns=[dict(name='id',data_type='int64'),dict(name='items',data_type='list:list:'+data_type)])]
    r['nodes'] = [dict(label='Typed',table='nested_typed',id='id',properties={'items':'items'})]
    r['edges'] = []
    r['query'] = 'MATCH (n:Typed) RETURN n.items AS items, n.items[0] AS first'
    assert rows(compiler,r,engines) == [[values, values[0]]]
