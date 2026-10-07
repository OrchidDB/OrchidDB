# OrchidDB CLI

Register schema configuration and execute graph query text separately. Run from
the repository root:

```sh
make cli
./target/debug/orchiddb query 'MATCH (p:Person) RETURN p.name AS name ORDER BY name' \
  --schema cli/examples/people.json --init cli/examples/setup.sql --no-iceberg --format table

./target/debug/orchiddb query --file cli/examples/people.cypher \
  --schema cli/examples/people.json --init cli/examples/setup.sql --no-iceberg > results.arrow
```

Both execute the query and return Ada and Grace. The JSON file contains only
schema and mapping configuration. Query files contain plain query text. Old
combined request documents and the `compile` command are rejected.

`--language cypher|gremlin|sparql` selects the query language (default: Cypher).
`--parameters parameters.json` supplies a separate JSON object of bound values.
`--database FILE` selects a persistent DuckDB database. `--init FILE` executes
application-owned SQL setup, including tables, views, credentials, and extensions.
Query results default to Arrow IPC on stdout; `--format table` displays them.
Diagnostics use stderr. The CLI currently executes on DuckDB; SDK connections
also support PostgreSQL. The optional Orchid DuckDB extension is built separately.

DuckDB and the CLI's native driver are bundled. Iceberg loads by default and may
be downloaded from DuckDB's extension service on first use. `--no-iceberg`
disables that setup for queries that do not need Iceberg.

## Statistics

```sh
./target/debug/orchiddb statistics --schema cli/examples/people.json \
  --init cli/examples/setup.sql --no-iceberg --output statistics.json
./target/debug/orchiddb query --file cli/examples/people.cypher \
  --schema cli/examples/people.json --statistics statistics.json \
  --init cli/examples/setup.sql --no-iceberg --format table
```

Statistics generation uses bounded reads and saves a reusable schema-specific
snapshot. It is explicit; queries do not trigger background refreshes.

For remote engines, keep engine/table routing in the schema and connection
credentials in a separate `--engines engines.json` file. The same query interface
executes the resulting cross-engine plan internally.

Builds and tests run locally. See [client development](../clients/README.md).
