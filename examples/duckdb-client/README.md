# Caller-owned DuckDB

```sh
cargo run --manifest-path examples/duckdb-client/Cargo.toml
```

Prints `ADA` and `GRACE`. The application creates its own connection and SQL
function, starts a transaction, compiles a graph query, and consumes a borrowed
DuckDB cursor. It then rolls back and verifies that the connection and transaction
remained under application control.

This standalone application declares DuckDB itself. Its `orchiddb` dependency uses
ordinary default features (no runtime driver). Replace the connection setup with
your own database, extensions, UDFs, or cached session. The adapter makes no setup,
commit, rollback, or close calls. Errors during streaming are driver errors; the
caller decides transaction recovery. This synchronous driver runs on the calling
thread even though the generic session interface is asynchronous.

You can also skip the adapter and prepare `compiled.sql` directly. SQL includes
specialized parameter values; cache prepared plans using the complete compilation
inputs, never query text alone. Keep metadata consistent with the selected session.

The example defaults to building its own bundled driver. To use an existing
compatible DuckDB library instead, disable the example's bundling feature and
point the driver to your installation:

```sh
DUCKDB_LIB_DIR=/path/to/duckdb/lib cargo run \
  --manifest-path examples/duckdb-client/Cargo.toml --no-default-features
```

This flag belongs to the example application. OrchidDB itself has no default
driver feature in either configuration.
