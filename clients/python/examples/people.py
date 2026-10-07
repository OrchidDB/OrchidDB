import duckdb
from orchiddb import Compiler, DuckDBEngine, Graph
connection = duckdb.connect()
connection.execute("CREATE TABLE people(id BIGINT, name VARCHAR)")
connection.execute("INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace')")
graph = Graph(Compiler(), DuckDBEngine(connection),
    tables=[{"name":"people", "columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
    nodes=[{"label":"Person","table":"people","id":"id","properties":{"name":"name"}}])
with graph.query_arrow("MATCH (p:Person) RETURN p.name AS name") as reader:
    print(reader.schema)
    names = []
    for batch in reader:
        print(batch)
        names.extend(batch.column("name").to_pylist())
    assert sorted(names) == ["Ada", "Grace"]
assert connection.execute("SELECT 42").fetchone() == (42,)
connection.close()
