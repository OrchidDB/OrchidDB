# OrchidDB CLI

A Rust CLI with bundled DuckDB 1.5.2 and the official DuckDB Iceberg extension
loaded by default. Graph queries compile to SQL and results use native Arrow
batches. First use installs the signed Iceberg extension and requires network
access to extensions.duckdb.org; subsequent runs use the local extension cache.
It fails clearly if installation/loading fails. The extension is downloaded,
not statically embedded in the executable. [DuckDB Iceberg documentation](https://duckdb.org/docs/stable/core_extensions/iceberg/overview).

```sh
cargo run -p orchiddb-cli -- query examples/people.json --init examples/setup.sql --format table
cargo run -p orchiddb-cli -- query examples/people.json --init examples/setup.sql > results.arrow
cargo run -p orchiddb-cli -- compile examples/people.json
```

`query` defaults to Arrow IPC on stdout; diagnostics use stderr. `--format table`
prints batches for humans. `--database FILE` opens a persistent DuckDB database.
`--init setup.sql` executes your SQL first, allowing credentials, plugins, UDFs,
views over `iceberg_scan`, and attached catalogs. Treat setup SQL as executable
application code. Schema/mappings in the request must match those sources.
`--no-iceberg` explicitly disables extension setup for queries that do not use it.
Compile mode never opens a database or installs extensions.

For Iceberg, create a view in setup SQL such as:

```sql
CREATE VIEW people AS
SELECT id, name FROM iceberg_scan('/path/to/table/metadata/v1.metadata.json');
```

Then use the same mapping in examples/people.json. No data is copied into a graph
store. The CLI owns its connection; embedding applications should use
[Rust client](../clients/rust) for caller-owned sessions.
Arrow batches avoid per-cell object conversion, but DuckDB query execution may
materialize results internally. Supported graph language scope is the compiler's
SQL-lowerable subset, not every managed-runtime feature.

## Build, test, release

`cargo test` includes an actual default Iceberg load and scan of an empty Iceberg v2 table
(network access on first run).
`cargo build --release` bundles DuckDB. A matching external DuckDB library can be
used for development with `DUCKDB_LIB_DIR` and `--no-default-features`.
Builds and packaging run locally from this monorepo. The CLI depends on the
local Rust client and core; no separate client repositories are fetched.
See [client development](../clients/README.md). GitHub is the publication destination.
The existing OrchidDB license applies; see LICENSE.md.

## Generate statistics once

```sh
orchiddb statistics examples/people.json --init examples/setup.sql \
  --no-iceberg --output statistics.json
orchiddb compile examples/people.json --statistics statistics.json --explain-json
orchiddb query examples/people.json --statistics statistics.json \
  --init examples/setup.sql --no-iceberg --format table
```

`statistics` uses the application-configured DuckDB session to execute bounded
collection requests selected by the shared core. The adapter enforces deadlines
through DuckDB interruption, and caps returned rows and bytes. The command saves
a portable snapshot and prints the coverage report. Generate it again when data
changes; there is no background refresh or collection profile configuration.

`--statistics` installs the snapshot for compilation. Without it behavior is
unchanged. `compile --explain-json` prints full plans and diagnostics; on `query`,
`--explain-json` writes diagnostics to stderr while preserving result stdout.
Compilation from a saved snapshot does not open the database.

## Quickwit and Elasticsearch

The release binary includes both optional remote adapters. Map the remote index
or a SQL-owned document table using the
[remote engine guide](https://docs.orchiddb.com/remote-engines.html). Keep connection
options separate from the query in an engine-ID keyed JSON file:

```json
{"text": {"endpoint": "http://localhost:7280", "page_size": 1000}}
```

```sh
orchiddb query search.json --engines engines.json --init setup.sql --no-iceberg
```

`search.json` selects the adapter through `engines.text.dialect` (`quickwit` or
`elasticsearch`). DuckDB coordinates the resulting SQL islands. The engine options
also accept authentication and bounded timeouts/results, as documented for the
language clients. Compiling the same request never connects to either service.
Source builds may disable the `quickwit` and `elasticsearch` Cargo features.
