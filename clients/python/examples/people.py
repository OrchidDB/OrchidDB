import duckdb
from orchiddb import Connection, DuckDBEngine

schema = {
    "tables": [{"name": "people", "columns": [
        {"name": "id", "data_type": "int64"}, {"name": "name", "data_type": "string"}]}],
    "nodes": [{"label": "Person", "table": "people", "id": "id", "properties": {"name": "name"}}],
}
with duckdb.connect() as database:
    database.execute("CREATE TABLE people(id BIGINT, name VARCHAR)")
    database.execute("INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace')")
    with Connection(DuckDBEngine(database), schema) as connection:
        with connection.query("MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
                              parameters={"name": "Ada"}) as reader:
            print(reader.read_all().to_pylist())
        with connection.query("MATCH (p:Person) RETURN count(p) AS people") as reader:
            print(reader.read_all().to_pylist())
    assert database.execute("SELECT 42").fetchone() == (42,)
