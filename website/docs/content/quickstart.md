# Quickstart

Query existing tables through a graph mapping. The standalone CLI bundles DuckDB and loads Iceberg by default; no graph store is created.

## Install the CLI

```sh
curl -fsSL https://install.orchiddb.com | bash
export PATH="$HOME/.local/bin:$PATH"
orchiddb --version
```

The published v0.1.0 binary supports macOS ARM64 (Apple Silicon) and includes DuckDB. No Rust toolchain or source checkout is needed. See [installation](installation.md) for version pinning, other platforms, and source builds.

## Create the input files

In a working directory, save this as `setup.sql`:

```sql
CREATE TABLE people(id BIGINT, name VARCHAR);
INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace');
```

Save this as `people.json`, describing the table schema and its graph mapping:

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
orchiddb query people.json --init setup.sql --format table --no-iceberg
```

The result contains Ada and Grace. This table-only example uses `--no-iceberg` to skip extension setup. By default, the CLI loads the official Iceberg extension; its first installation requires network access.

## Inspect SQL or export Arrow

```sh
orchiddb compile people.json
orchiddb query people.json --init setup.sql --no-iceberg > people.arrow
```

Compilation does not open a database. Query output defaults to an Arrow IPC stream, so use `--format table` for terminal output. Each invocation above uses a fresh in-memory database.

## Use your data lake

Replace the example setup with your own catalog configuration and a view over an Iceberg table:

```sql
CREATE VIEW people AS
SELECT id, name FROM iceberg_scan('/path/to/table/metadata/v1.metadata.json');
```

Keep the graph mapping aligned with the view's real schema, and omit `--no-iceberg` when querying it. The compiler sees the metadata; DuckDB accesses the source data.

## Embed in your application

Choose [Rust, Java, Python, JavaScript/TypeScript, Elixir, or C++](client-apis.md). These clients compile the same graph-language requests but leave engine configuration and execution to your application. Results use Arrow batches.

For persisted graph records, see [managed graphs](managed-graphs.md). For reads and writes over existing tables through the shared engine, see [mapped graphs](mapped-graphs.md).
