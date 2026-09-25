# CLI reference

Run a graph query from your shell, read query files, and inspect a plan with the crabgraph binary.

## Invocation

```sh
crabgraph [OPTIONS]
```

From a source checkout, use `cargo run --bin crabgraph -- [OPTIONS]`. The arguments after `--` are passed to the binary.

## Options

| Option | Description | Default |
| --- | --- | --- |
| `--database PATH` | Persistent DuckDB graph file. | Fresh in-memory graph |
| `--language LANG` | Query language: `cypher` or `gremlin`. | `cypher` |
| `--query TEXT` | Query supplied directly in the command. | Read file or stdin |
| `--file PATH` | File containing a query. | Read stdin |
| `--sql-only` | Require SQL-only read execution. | Hybrid reads |
| `--explain` | Print Graph IR without executing. | Execute query |
| `-h`, `--help` | Print help and exit. | — |

Value options also accept `--name=value` syntax. Query input precedence is `--query`, then `--file`, then standard input.

## Query a database

```sh
crabgraph --database social.duckdb \
  --query "MATCH (p:Person) RETURN p.name ORDER BY p.name"
```

To start a fresh ephemeral session:

```sh
crabgraph --query 'RETURN 1 AS value'
```

## Read a file

Save one query as `people.cypher`:

```cypher
MATCH (p:Person)
RETURN p.name
ORDER BY p.name
```

Run it with:

```sh
crabgraph --database social.duckdb --file people.cypher
```

Or send it through stdin:

```sh
cat people.cypher | crabgraph --database social.duckdb
```

## Select Gremlin

```sh
crabgraph --database social.duckdb --language gremlin \
  --query "g.V().hasLabel('Person').values('name').order()"
```

## Explain a query

```sh
crabgraph --explain --query \
  "MATCH (p:Person)-[:KNOWS]->(friend) RETURN friend.name"
```

The output is the graph plan. For generated SQL over mapped tables, use the Rust API's `explain_cypher` method.

## Output and shell integration

The CLI writes tab-separated column headers and rows to stdout. Backend diagnostics and errors go to stderr. A failed command exits with a nonzero status.

```sh
crabgraph --database social.duckdb \
  --query "MATCH (p:Person) RETURN p.name" \
  > people.tsv 2> query.log
```

For application-controlled parameter binding and Arrow-native results, use the [Rust API](rust-api.md).
