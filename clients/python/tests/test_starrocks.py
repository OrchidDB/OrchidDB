import os
import uuid

import pytest

from orchiddb import Connection, StarRocksEngine


@pytest.fixture(scope="module")
def database():
    pymysql = pytest.importorskip("pymysql")
    port = os.getenv("ORCHIDDB_TEST_STARROCKS_PORT")
    if not port:
        pytest.skip("ORCHIDDB_TEST_STARROCKS_PORT is required")
    db = pymysql.connect(host="127.0.0.1", port=int(port), user="root", autocommit=True)
    name = "orchiddb_test_" + uuid.uuid4().hex
    try:
        with db.cursor() as cursor:
            cursor.execute(f"CREATE DATABASE `{name}`")
            cursor.execute(f"USE `{name}`")
            cursor.execute("SET enable_recursive_cte=true")
            cursor.execute('CREATE TABLE people(id BIGINT, name VARCHAR(128), age BIGINT, score DOUBLE, tags ARRAY<VARCHAR(128)>) DISTRIBUTED BY HASH(id) BUCKETS 1 PROPERTIES("replication_num"="1")')
            cursor.execute("INSERT INTO people VALUES (1,'Ada',37,1.25,['x','y']),(2,'Bob',25,2.5,['y']),(3,'Cy',29,NULL,[]),(4,NULL,NULL,0,NULL)")
            cursor.execute('CREATE TABLE links(id BIGINT, src BIGINT, dst BIGINT) DISTRIBUTED BY HASH(id) BUCKETS 1 PROPERTIES("replication_num"="1")')
            cursor.execute("INSERT INTO links VALUES (1,1,2),(2,2,3),(3,1,3)")
        yield db
    finally:
        with db.cursor() as cursor:
            cursor.execute(f"DROP DATABASE IF EXISTS `{name}`")
        db.close()


@pytest.fixture(scope="module")
def connection(database):
    schema = dict(
        tables=[dict(name="people", columns=[dict(name=n, data_type=t) for n, t in
                    [("id", "int64"), ("name", "string"), ("age", "int64"), ("score", "float64"), ("tags", "list:string")]]),
                dict(name="links", columns=[dict(name=n, data_type="int64") for n in ["id", "src", "dst"]])],
        nodes=[dict(label="Person", table="people", id="id", properties={n:n for n in ["name", "age", "score", "tags"]})],
        edges=[dict(label="KNOWS", table="links", id="id", source="src", target="dst", source_label="Person", target_label="Person")],
        ontology=dict(classes=[dict(iri="urn:Person", label="Person")],
                      properties=[dict(iri="urn:"+n, label="Person", property=n) for n in ["name", "age"]],
                      relationships=[dict(iri="urn:knows", label="KNOWS", source_label="Person", target_label="Person")]))
    with Connection(StarRocksEngine(database), schema, library=os.environ.get("ORCHIDDB_NATIVE_LIBRARY")) as graph:
        yield graph


def rows(connection, query, **kwargs):
    with connection.query(query, batch_size=2, **kwargs) as result:
        return [list(row.values()) for row in result.read_all().to_pylist()]


@pytest.mark.parametrize("language,query,expected", [
    ("cypher", "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 25 RETURN a.name AS a, b.name AS b ORDER BY a,b", [["Ada","Bob"],["Ada","Cy"]]),
    ("gremlin", "g.V().hasLabel('Person').has('age',gt(25)).out('KNOWS').values('name').order()", [["Bob"],["Cy"]]),
    ("sparql", "SELECT ?a ?b WHERE { ?x <urn:age> ?age; <urn:name> ?a; <urn:knows> ?y . ?y <urn:name> ?b . FILTER(?age > 25) } ORDER BY ?a ?b", [["Ada","Bob"],["Ada","Cy"]]),
])
def test_languages(connection, language, query, expected):
    assert rows(connection, query, language=language) == expected


@pytest.mark.parametrize("expression,expected", [
    ("toUpper(p.name)", "ADA"), ("toLower(p.name)", "ada"),
    ("size(p.name)", 3), ("substring(p.name,1,2)", "da"),
    ("p.tags", ["x","y"]), ("size(p.tags)", 2),
    ("p.tags[0]", "x"), ("p.tags[-1]", "y"),
    ("'x' IN p.tags", True), ("abs(p.score)", 1.25),
    ("floor(p.score)", 1.0), ("ceil(p.score)", 2.0),
    ("p.name + '!'", "Ada!"), ("p.age / 2", 18), ("p.age / 2 > 18", False),
    ("fn.character_length(p.name)", 3), ("fn.contains(p.name,'d')", True),
    ("fn.concat(p.name,null,'!')", "Ada!"), ("fn.reverse(p.name)", "adA"),
])
def test_expressions(connection, expression, expected):
    assert rows(connection, "MATCH (p:Person) WHERE p.age=37 RETURN " + expression + " AS value") == [[expected]]


def test_parameters(connection):
    for value in ["it's", "it''s", "a\\b", "a\\'b", "é😀", "a\0b"]:
        assert rows(connection, "RETURN $value AS value", parameters={"value":value}) == [[value]]


def test_aggregation(connection):
    assert rows(connection, "MATCH (p:Person) RETURN count(p) AS n, sum(p.age) AS total") == [[4,91]]


def test_optional_and_nulls(connection):
    assert rows(connection, "MATCH (p:Person) OPTIONAL MATCH (p)-[:KNOWS]->(q:Person) RETURN p.name AS p, count(q) AS n ORDER BY n DESC, p") == [["Ada",2],["Bob",1],["Cy",0],[None,0]]


def test_empty_result(connection):
    assert rows(connection, "MATCH (p:Person) WHERE p.age<0 RETURN p.name") == []


def test_connection_ownership(connection, database):
    with connection.query("RETURN 1 AS value") as result:
        assert result.read_next_batch().num_rows == 1
        with pytest.raises(RuntimeError, match="already active"):
            with connection.query("RETURN 2"):
                pass
    assert rows(connection, "RETURN 3") == [[3]]
    with database.cursor() as cursor:
        cursor.execute("SELECT 4")
        assert cursor.fetchone() == (4,)


@pytest.mark.parametrize("query,expected", [
    ("MATCH (p:Person) RETURN p.name AS name ORDER BY name SKIP 1 LIMIT 2", [["Bob"],["Cy"]]),
    ("MATCH (p:Person) RETURN count(DISTINCT p.age)", [[3]]),
    ("MATCH (p:Person) WHERE p.name='Cy' RETURN 'x' IN p.tags", [[False]]),
    ("MATCH (p:Person) WHERE p.name IS NULL RETURN 'x' IN p.tags", [[None]]),
    ("MATCH (p:Person) WHERE p.age=37 RETURN p.tags[20]", [[None]]),
    ("MATCH (p:Person) WHERE p.age=37 RETURN fn.octet_length(p.name), fn.starts_with(p.name,'A'), fn.ends_with(p.name,'a')", [[3,True,True]]),
])
def test_query_shapes(connection, query, expected):
    assert rows(connection, query) == expected


def test_execution_error_releases_connection(connection, database):
    with database.cursor() as cursor:
        cursor.execute("ALTER TABLE people RENAME people_saved")
    try:
        with pytest.raises(Exception):
            rows(connection, "MATCH (p:Person) RETURN p.name")
    finally:
        with database.cursor() as cursor:
            cursor.execute("ALTER TABLE people_saved RENAME people")
    assert rows(connection, "MATCH (p:Person) RETURN count(p)") == [[4]]


def test_unsupported_collection_is_explicit(connection):
    from orchiddb import QueryError
    with pytest.raises(QueryError, match="Non-null collection"):
        rows(connection, "MATCH (p:Person) RETURN collect(p.age)")


def test_bounded_paths(connection):
    assert rows(connection, "MATCH (a:Person)-[:KNOWS*1..2]->(b:Person) WHERE a.name='Ada' RETURN b.name AS name ORDER BY name") == [["Bob"],["Cy"],["Cy"]]


@pytest.mark.parametrize("query,expected", [
    ("RETURN 1 AS `select`", [[1]]),
    ("RETURN null AS value", [[None]]),
    ("MATCH (p:Person) WHERE p.age=37 RETURN -p.age / 2", [[-18]]),
    ("MATCH (p:Person) WHERE p.age=37 RETURN p.score / 2", [[0.625]]),
    ("MATCH (p:Person) WHERE p.age=37 RETURN p.tags[-20]", [[None]]),
])
def test_edge_cases(connection, query, expected):
    assert rows(connection, query) == expected


def test_portable_array_length(connection):
    assert rows(connection, "MATCH (p:Person) WHERE p.age=37 RETURN fn.array_length(p.tags)") == [[2]]


def test_unmapped_function_is_explicit(connection):
    from orchiddb import QueryError
    with pytest.raises(QueryError, match="no starrocks mapping"):
        rows(connection, "MATCH (p:Person) RETURN fn.initcap(p.name)")
