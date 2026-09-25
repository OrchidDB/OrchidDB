# CLI reference

The standalone [OrchidDB-cli](https://github.com/OrchidDB/OrchidDB-cli) bundles DuckDB and loads the official Iceberg extension by default. It compiles graph reads to SQL and returns Arrow results.

## Invocation

```sh
orchiddb compile REQUEST.json
orchiddb query REQUEST.json [--database FILE] [--init SQL_FILE] [--format arrow|table] [--no-iceberg]
orchiddb --version
```

`REQUEST.json` uses [compiler protocol version 1](sql-compiler.md), including language, query, dialect, table schemas, and graph mappings. The CLI executes DuckDB SQL only. `compile` prints SQL without opening a database or installing extensions.

## Query options

| Option | Description | Default |
| --- | --- | --- |
| `--database FILE` | DuckDB database path | In-memory database |
| `--init SQL_FILE` | Run caller-supplied setup SQL before the query | No setup |
| `--format arrow` | Arrow IPC stream on stdout | Default output |
| `--format table` | Human-readable output, one batch at a time | Opt-in |
| `--no-iceberg` | Skip Iceberg installation/loading | Iceberg enabled |

Use space-separated option values. The request is a file, not inline query text. This binary does not accept the legacy managed CLI's `--query`, `--language`, or `--explain` flags.

## Run the included example

From an `OrchidDB-cli` checkout after [installation](installation.md):

```sh
orchiddb query examples/people.json --init examples/setup.sql --format table
orchiddb compile examples/people.json
orchiddb query examples/people.json --init examples/setup.sql > people.arrow
```

The setup creates source tables and the request maps them to a `Person` label. Arrow IPC goes to stdout; errors go to stderr. A failure returns a nonzero exit status. See the [quickstart](quickstart.md) for complete input files.

## Iceberg and engine setup

Queries first load the cached Iceberg extension or install the signed official extension from DuckDB's extension service. Installation failure is reported; there is no silent fallback. Use `--no-iceberg` for queries that do not need Iceberg.

Your initialization file can configure credentials, plugins, UDFs, attached catalogs, and views:

```sql
CREATE VIEW people AS
SELECT id, name FROM iceberg_scan('/path/to/table/metadata/v1.metadata.json');
```

Map that view in your request. Actual storage access requires your own credentials and catalog setup. Setup SQL is executed as supplied. The CLI owns its database connection; applications that already own an engine should use a [language client](client-apis.md).

## Managed runtime CLI

Core retains a separate legacy binary behind `--features duckdb`, used by some managed graph examples. Its mutation/query flags are different. The standalone CLI described here supports the SQL compiler's read subset, not the managed runtime's full conformance scope.
