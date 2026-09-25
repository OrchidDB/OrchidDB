# Quickstart

Query existing tables through a graph mapping. The standalone CLI bundles DuckDB and loads Iceberg by default; no graph store is created.

## Build the CLI

```sh
git clone https://github.com/OrchidDB/OrchidDB-cli.git
cd OrchidDB-cli
cargo build --locked --release
```

The checkout includes `examples/setup.sql`:

```sql
CREATE TABLE people(id BIGINT, name VARCHAR);
INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace');
```

And `examples/people.json`, describing the table schema and its graph mapping:

```json
{
  "version": 1,
  "dialect": "duckdb",
  "language": "cypher",
  "query": "MATCH (p:Person) RETURN p.name AS name ORDER BY name",
  "tables": [{"name": "people", "columns": [
    {"name": "id", "data_type": "int64", "nullable": false},
    {"name": "name", "data_type": "string", "nullable": true}
  ]}],
  "nodes": [{"label": "Person", "table": "people", "id": "id",
    "properties": {"name": "name"}}]
}
```

## Execute the query

```sh
./target/release/orchiddb query examples/people.json --init examples/setup.sql --format table
```

The result contains Ada and Grace. The first query needs network access to install Iceberg; append `--no-iceberg` to this table-only example to skip extension setup.

## Inspect SQL or export Arrow

```sh
./target/release/orchiddb compile examples/people.json
./target/release/orchiddb query examples/people.json --init examples/setup.sql > people.arrow
```

Compilation does not open a database. Query output defaults to an Arrow IPC stream, so use `--format table` for terminal output. Each invocation above uses a fresh in-memory database.

## Use your data lake

Replace the example setup with your own catalog configuration and a view over an Iceberg table:

```sql
CREATE VIEW people AS
SELECT id, name FROM iceberg_scan('/path/to/table/metadata/v1.metadata.json');
```

Keep the graph mapping aligned with the view's real schema. The compiler sees the metadata; DuckDB accesses the source data.

## Embed in your application

Choose [Rust, Java, Python, JavaScript/TypeScript, Elixir, or C++](client-apis.md). These clients compile the same graph-language requests but leave engine configuration and execution to your application. Results use Arrow batches.

For graph persistence and mutations owned by OrchidDB, see the separate [managed runtime](managed-graphs.md). Its capabilities and APIs differ from compiler-only clients.
