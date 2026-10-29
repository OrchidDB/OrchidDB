# OrchidDB for Rust

Retain schema on a connection and execute queries directly. Add a local path
dependency on `clients/rust`; the application supplies its database driver.

From the extracted OrchidDB source release, add this to your application's
`Cargo.toml` (adjust the path to the extracted directory):

```toml
[dependencies]
orchiddb-client = { path = "../orchiddb-source/clients/rust" }
```

Keep the complete source directory: the client depends on the shared core two
directories above it. This version is distributed through GitHub source releases,
not crates.io. Enable `features = ["orchid-catalog"]` on this dependency to use
`OrchidCatalog` with `Connection::with_catalog`.

```rust,ignore
use orchiddb_client::{Connection, Query, Schema};
let schema = Schema::from_json(&std::fs::read_to_string("schema.json")?)?;
let mut graph = Connection::new(session, schema);
let mut rows = graph.query(
    Query::cypher("MATCH (p:Person) WHERE p.name=$name RETURN p.name")
        .parameter("name", "Ada")
).await?;
for batch in &mut rows {
    let batch = batch?; // Arrow RecordBatch
}
```

`session` implements `SqlSession` and borrows or owns your driver connection.
It executes the internal `SqlWork` supplied by OrchidDB and exposes an Arrow
reader. No compiler or compiled-plan API is exported for application queries.
Schema configuration rejects query fields. Queries use `Query::cypher`.

From the repository root, run the complete borrowed-connection example:

```sh
cargo run --locked --manifest-path clients/rust/examples/Cargo.toml --features bundled
```

[The example](examples/borrowed_duckdb.rs) includes a concrete adapter, tables,
UDF signature, result consumption, and caller-owned rollback. Readers borrow
the session; retained Arrow batches keep their buffers. No database driver is
included in this client's normal dependencies.

`FederatedConnection` registers Schema with named `EngineSession` adapters and
executes the same separate Query value. Built-in source dialects are PostgreSQL, DuckDB, and Trino. `Connection::generate_statistics` takes a bounded collection adapter;
`save_statistics`, `load_statistics`, and `clear_statistics` manage snapshots.

[Source builds and API migration](../README.md)
