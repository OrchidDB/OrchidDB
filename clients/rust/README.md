# OrchidDB Rust client

Source builds use the shared core in this monorepo. See [build and test instructions](../README.md).

Compile Cypher, Gremlin text, or SPARQL to SQL. Execute through your own
`SqlSession`, whose result implements Arrow 58 `RecordBatchReader`.
No database driver is a production dependency. You own connections, transactions,
extensions, UDFs, schema discovery, and cache invalidation. SQL federation APIs
compile and bind typed SQL islands for caller-owned execution across engines.

```rust,ignore
let plan = orchiddb_client::compile(request).await?;
let mut batches = orchiddb_client::execute(&mut your_session, &plan).await?;
for batch in &mut batches {
    let batch = batch?; // Native Arrow RecordBatch; no cell-by-cell conversion.
}
```

Add the published client to your application's Cargo.toml:

```toml
[dependencies]
orchiddb-client = { version = "=0.3.0", default-features = false }
```

Run the complete caller-owned DuckDB example:

```sh
cargo run --manifest-path examples/Cargo.toml --features bundled
```

The standalone example depends on crates.io packages, including its own DuckDB driver. Its `bundled` feature builds DuckDB; OrchidDB itself contains no driver. To use an existing matching
DuckDB 1.5.2 library, set `DUCKDB_LIB_DIR` and omit that feature.
Retained Rust batches retain their buffers after advancing/dropping the reader.
Dropping readers releases the mutable session borrow. Arrow batch transport does
not guarantee streaming execution inside every database.

See [the example](examples/borrowed_duckdb.rs) for mappings, declared UDF signatures,
borrowed connections, native Arrow schema/batches, and caller-controlled rollback.

## Permission pushdown

Supply effective grants as a mapped table and add scopes to the node request. A scope matches the permission relation's resource ID against a node source column; multiple scopes are ORed, so direct object grants and project or tenant grants can both authorize a row. The compiler fails closed if scopes are present without a principal. The relation must already contain effective grants; this client does not connect to a permission service or sync grants.

The JSON v1 request accepts top-level `authorization: {subject_type, subject_id}` and a node's `permission_scopes`, where each scope has `resource_column` and a `relation` containing `table`, `resource_type`, and `permission`. `PermissionRelation::flat(table, resource_type, permission)` uses the conventional relation columns, and `PermissionScope::new` and `Authorization::new` build typed values.
[Core protocol documentation](../../README.md#standalone-compilation-and-engine-adapters).

## Releases

Version 0.3.0 is published on crates.io as `orchiddb-client`, depending on
`orchiddb = 0.3.0`. The engine embeds its modified SPARQL parser; no separate
parser crate is needed. The library checkout retains its pinned Git dependency for core development; the standalone example uses the published registry dependency.

Run the manual `release.yml` workflow with `ref=main`, `publish=false` to test
and verify both registry archives before either is published. Download the
checksummed `verified-crates` workflow artifact for inspection. Nothing is
uploaded to crates.io unless `publish=true` is explicitly selected with a
matching existing version tag. Publish the engine first, then the client.
Both use the `CARGO_REGISTRY_TOKEN` secret in the `crates-io` environment.
The CLI is distributed separately through GitHub Releases.

## One-time statistics

`Statistics` retains a shared-core catalog for repeated compilation. Generation
can involve multiple SQL requests, all chosen by Rust core; client adapters do
not calculate estimates or choose collection profiles.

```rust,ignore
let mut statistics = orchiddb_client::Statistics::default();
statistics.generate(request.clone(), |work| {
    // Reuse the application's session. The adapter must enforce timeout_ms,
    // max_rows, and max_bytes. Return rows or base64 Arrow IPC, or an error when
    // bounded execution is unavailable. See statistics::StatisticsCollector
    // for stateful adapters borrowing a connection.
    application.collect_bounded(work)
}).await?;
let plan = statistics.compile(request).await?;
statistics.save("statistics.json")?;
statistics.clear().await?;
statistics.load("statistics.json").await?;
```

`compile_plan(request)` returns the normal typed `CompiledSql` for
`execute(session, &plan)`, reusing the cached statistics Arc directly.
`report()` exposes collection coverage and skipped work; `snapshot()` exposes the
portable JSON catalog. `compile()` preserves all diagnostics, including
`statistics_usage`, `plan_estimates`, layout and representation decisions. With
no catalog it uses the existing compiler. Regeneration replaces the previous
catalog only after success. No compilation reads data and no background refresh
runs. Clear retained catalogs after their last user finishes; dropping `Statistics`
also releases its native handle. Dropping an in-progress generation future
cancels its coordinator state and retains the previous catalog.

The lower-level `statistics::command` API exposes begin/next/submit/finish,
install/release, and cached compile for applications that need their own batch
or cancellation orchestration. Explicit cancellation should send `cancel` for
an in-progress analysis. Callback failures report skipped sources; aborting the
whole operation preserves the previously installed catalog.

Collectors must set `truncated: true` when transport limits stop a response before
EOF; the coordinator retains those observations as a partial sample. It must not
infer a complete source row count from a shortened response.

For a complete adapter with a DuckDB interruption deadline, run
`cargo run --example statistics` (with the same DuckDB linkage as the other
examples). The example borrows the application's connection; it does not create
an OrchidDB-owned database session.

## Remote text engines

Enable either adapter independently with the `quickwit` and `elasticsearch`
Cargo features. Register `remote::transport::QuickwitSession` or
`ElasticsearchSession` alongside your SQL `federation::Session` and execute the
compiled plan using `federation::execute`. Sessions stay owned by your application.
The compiler builds search requests for `SIMILAR_TO` relationships, then binds
remote matches into the remaining SQL islands.

See the [search engine documentation](https://docs.orchiddb.com/remote-engines.html)
for relationship mappings, SQL-owned documents with external indexes, connection
options, and the supported query operations. The client adds no SQL driver.

Local live validation uses `ORCHIDDB_REMOTE_FIXTURE` generated by the shared repository's `scripts/clients_remote_fixture.py`, then
`cargo test --features quickwit,elasticsearch`.
