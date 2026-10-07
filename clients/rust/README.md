# OrchidDB for Rust

Retain schema on a connection and execute queries directly. Add a local path
dependency on `clients/rust`; the application supplies its database driver.

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
Schema configuration rejects query fields. Query language is selected with
`Query::cypher`, `Query::gremlin`, or `Query::sparql`; RDF rules stay in Schema.

From the repository root, run the complete borrowed-connection example:

```sh
cargo run --manifest-path clients/rust/examples/Cargo.toml --features bundled
```

[The example](examples/borrowed_duckdb.rs) includes a concrete adapter, tables,
UDF signature, result consumption, and caller-owned rollback. Readers borrow
the session; retained Arrow batches keep their buffers. No database driver is
included in this client's normal dependencies.

`FederatedConnection` registers Schema with named `EngineSession` adapters and
executes the same separate Query value. HTTP adapters are feature-gated under
`engines`. `Connection::generate_statistics` takes a bounded collection adapter;
`save_statistics`, `load_statistics`, and `clear_statistics` manage snapshots.

[Source builds and API migration](../README.md)
