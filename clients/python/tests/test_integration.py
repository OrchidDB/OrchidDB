import os
import duckdb
import pyarrow as pa
import pytest
from orchiddb import Authorization, Compiler, DuckDBEngine, Graph, CompilationError, PermissionRelation, PermissionScope

@pytest.fixture
def graph():
    connection = duckdb.connect()
    connection.execute("CREATE TABLE people(id BIGINT, name VARCHAR)")
    connection.execute("INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace'), (3, NULL)")
    connection.create_function("decorate", lambda s: s + "!", ["VARCHAR"], "VARCHAR")
    graph = Graph(Compiler(), DuckDBEngine(connection),
        tables=[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
        nodes=[{"label":"Person","table":"people","id":"id","properties":{"name":"name"}}],
        functions=[{"name":"decorate","target":"decorate","parameters":["string"],"returns":"string"}])
    yield graph
    connection.close()

def test_arrow_schema_batches_null_and_retained_batch(graph):
    with graph.query_arrow("MATCH (p:Person) RETURN p.name AS name ORDER BY p.id", batch_size=1) as reader:
        assert isinstance(reader, pa.RecordBatchReader)
        batches = list(reader)
        assert len(batches) == 3
        assert batches[0].column(0).to_pylist() == ["Ada"]
        assert batches[2].column(0).null_count == 1
    assert batches[0].column(0).to_pylist() == ["Ada"]

def test_parameter_udf_and_caller_transaction(graph):
    conn = graph.engine.connection
    conn.execute("BEGIN")
    conn.execute("INSERT INTO people VALUES (4, 'Linus')")
    with graph.query_arrow("MATCH (p:Person) WHERE p.name=$name RETURN decorate(p.name) AS name", parameters={"name":"Linus"}) as reader:
        assert reader.read_all().column(0).to_pylist() == ["Linus!"]
    conn.execute("ROLLBACK")
    assert conn.execute("SELECT count(*) FROM people").fetchone() == (3,)

def test_gremlin_and_failure_cleanup(graph):
    with graph.query_arrow("g.V(1).values('name')", language="gremlin") as reader:
        assert reader.read_all().column(0).to_pylist() == ["Ada"]
        with pytest.raises(RuntimeError, match="already active"):
            with graph.query_arrow("RETURN 1"):
                pass
    with pytest.raises(CompilationError):
        graph.plan("MATCH (p:Person) DELETE p")
    with graph.query_arrow("RETURN 42 AS answer") as reader:
        assert reader.read_all().column(0).to_pylist() == [42]

def test_injection_is_literal(graph):
    with graph.query_arrow("MATCH (p:Person) WHERE p.name=$name RETURN p.name", parameters={"name":"Ada'; DROP TABLE people; --"}) as reader:
        assert reader.read_all().num_rows == 0
    assert graph.engine.connection.execute("SELECT count(*) FROM people").fetchone() == (3,)

def test_permission_scopes_filter_direct_and_project_grants():
    connection = duckdb.connect()
    connection.execute("CREATE TABLE documents(id BIGINT, project_id BIGINT, title VARCHAR)")
    connection.execute("INSERT INTO documents VALUES (1,10,'direct'),(2,20,'project'),(3,30,'denied')")
    connection.execute("CREATE TABLE effective_grants(resource_type VARCHAR, resource_rel VARCHAR, resource_id VARCHAR, subject_type VARCHAR, subject_rel VARCHAR, subject_id VARCHAR)")
    connection.execute("INSERT INTO effective_grants VALUES ('document','view','1','user','','alice'),('project','view','20','user','','alice'),('project','view','30','user','','bob')")
    scopes = [
        PermissionScope("id", PermissionRelation.flat("effective_grants", "document", "view")).to_dict(),
        PermissionScope("project_id", PermissionRelation.flat("effective_grants", "project", "view")).to_dict(),
    ]
    graph = Graph(Compiler(), DuckDBEngine(connection), tables=[
        {"name":"documents", "columns":[{"name":"id","data_type":"int64"},{"name":"project_id","data_type":"int64"},{"name":"title","data_type":"string"}]},
        {"name":"effective_grants", "columns":[{"name":name,"data_type":"string"} for name in ("resource_type","resource_rel","resource_id","subject_type","subject_rel","subject_id")]},
    ], nodes=[{"label":"Document","table":"documents","id":"id","properties":{"title":"title","project_id":"project_id"},"permission_scopes":scopes}])
    try:
        with graph.query_arrow("MATCH (d:Document) RETURN d.title AS title ORDER BY title", authorization=Authorization("user", "alice")) as reader:
            assert reader.read_all().column(0).to_pylist() == ["direct", "project"]
        with pytest.raises(CompilationError, match="requires a principal"):
            graph.plan("MATCH (d:Document) RETURN d.title")
    finally:
        connection.close()

def test_sparql_constant(graph):
    with graph.query_arrow("SELECT (42 AS ?answer) WHERE {}", language="sparql") as reader:
        assert reader.read_all().column(0).to_pylist() == [42]

def test_consumer_exception_and_database_error_release_lease(graph):
    with pytest.raises(ZeroDivisionError):
        with graph.query_arrow("RETURN 1") as reader:
            reader.read_next_batch()
            1 / 0
    graph.engine.connection.execute("DROP TABLE people")
    with pytest.raises(duckdb.Error):
        with graph.query_arrow("MATCH (p:Person) RETURN p.name"):
            pass
    with graph.query_arrow("RETURN 42 AS answer") as reader:
        assert reader.read_all().column(0).to_pylist() == [42]

def test_rdf_rules_query_the_same_application_table(graph):
    rdf = Graph(graph.compiler, graph.engine, tables=graph.metadata['tables'], rdf=[{
        'table': 'people',
        'subject': {'kind': 'template', 'prefix': 'urn:person:', 'columns': ['id']},
        'predicate': {'kind': 'constant', 'value': 'urn:name'},
        'object': {'kind': 'literal', 'column': 'name'},
    }])
    with rdf.query_arrow('SELECT ?name WHERE {?s <urn:name> ?name} ORDER BY ?name', language='sparql') as reader:
        assert reader.read_all().column(0).to_pylist() == ['Ada', 'Grace']


def test_generate_persist_and_reuse_statistics_all_languages(graph, tmp_path):
    analysis = graph.generate_statistics()
    assert analysis['snapshot']['sources']['people']['sample_rows'] == 3
    assert analysis['snapshot']['sources']['people']['estimated_rows'] == 3
    assert analysis['report']['accepted_rows'] >= 3
    queries = [('cypher', "MATCH (p:Person) WHERE p.name = 'Ada' RETURN p.name"),
               ('gremlin', "g.V().hasLabel('Person').has('name', 'Ada').values('name')"),
               ('sparql', 'SELECT (42 AS ?answer) WHERE {}')]
    for language, query in queries:
        plan = graph.plan(query, language=language)
        assert 'logical_plan' in plan.diagnostics
        with graph.engine.query_arrow(plan) as reader:
            assert reader.read_all().num_rows == 1
    path = tmp_path / 'statistics.json'
    graph.save_statistics(path)
    old = graph.compiler._catalog_id
    graph.clear_statistics()
    graph.load_statistics(path)
    assert graph.compiler._catalog_id != old
    assert graph.compiler.statistics_snapshot == analysis['snapshot']
    assert graph.compiler.statistics_report == analysis['report']
    with graph.query_arrow('MATCH (p:Person) RETURN count(p) AS n') as reader:
        assert reader.read_all().column(0).to_pylist() == [3]
    graph.clear_statistics()
