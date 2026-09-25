# Client APIs

Map your existing tables, compile a Cypher, Gremlin, or mapped SPARQL read query to SQL, then consume native Arrow batches from your own engine. The compiler receives schema and mapping metadata, not your data. Clients do not bundle DuckDB.

Each repository includes working source examples and release packaging. Registry packages are not published yet; follow the source instructions below.

## Shared execution contract

1. Your application opens/configures the database, plugins, UDFs, transactions, and caches.
2. Supply table schemas and graph mappings to the version-1 compiler.
3. Compile a supported read query into SQL and output field names.
4. Execute against one compatible engine and consume its Arrow result.
5. Release result resources; the connection remains yours.

DuckDB is the tested execution target. Core also renders PostgreSQL SQL. ClickHouse execution and cross-engine federation are not implemented. The engine adapter boundary allows other backends without transferring ownership of connections to the compiler.

Parameters are specialized into SQL. Cache keys must include their values, schema, mappings, ontology, functions, and dialect. Recompile after these change. Gremlin/SPARQL parameter bindings are not implemented. The compiler-only subset is narrower than the [managed runtime conformance results](conformance.md).

## Rust

[Repository and example](https://github.com/OrchidDB/OrchidDB-rust). Add the client to your application:

```sh
cargo add orchiddb-client --git https://github.com/OrchidDB/OrchidDB-rust --no-default-features
```

`SqlSession` declares a dialect and returns an Arrow `RecordBatchReader`. Its result may borrow the session. Retained batches own reference-counted buffers after reader drop. The driver is your dependency.

```rust,ignore
let plan = orchiddb_client::compile(request).await?;
let mut batches = orchiddb_client::execute(&mut your_session, &plan).await?;
for batch in &mut batches {
    let batch = batch?;
    // Read typed Arrow arrays from batch.
}
```

In a client checkout, run `cargo run --example borrowed_duckdb` for mappings, a caller-defined function, a borrowed connection, and rollback. Crates.io publication requires packaging the core compiler and its modified parser first; use a reviewed Git revision today.

## Java

[Repository](https://github.com/OrchidDB/OrchidDB-java) · [Arrow guide](https://github.com/OrchidDB/OrchidDB-java/blob/main/docs/arrow.md).

Follow the repository setup to check out its pinned core, build the JNI compiler, and run:

```sh
./scripts/run-example.sh ArrowBatches
```

`JdbcEngine.withArrow` takes your JDBC connection, Arrow allocator, and driver export callback. `Graph.queryArrow` returns an `ArrowResult`. Java vectors are borrowed: consume them before advancing or closing the result. Copy/transfer data explicitly if it must outlive that scope. Your parent allocator and connection remain caller-owned.

Java requires an Arrow-compatible JVM setup, including `--add-opens=java.base/java.nio=ALL-UNNAMED`; the example launcher handles it. DuckDB JDBC is test-only. Maven Central packaging is configured but not published. The optional `orchiddb-gremlin` module adds fluent TinkerPop traversal integration; other clients accept Gremlin text.

## Python

[Repository and complete example](https://github.com/OrchidDB/OrchidDB-python). In a source checkout:

```sh
python -m venv .venv
. .venv/bin/activate
python -m pip install -e '.[test]'
# Set ORCHIDDB_NATIVE_LIBRARY using the installation guide.
python examples/people.py
```

The test extra supplies DuckDB and PyArrow for the example; they are not bundled with the compiler. Build the [matching native compiler](installation.md#shared-native-compiler) first.

```python
from orchiddb import Compiler, DuckDBEngine, Graph
# connection, tables, and nodes are supplied by your application.
graph = Graph(Compiler(), DuckDBEngine(connection), tables=tables, nodes=nodes)
with graph.query_arrow("MATCH (p:Person) RETURN p.name AS name") as reader:
    for batch in reader:
        print(batch.num_rows)
```

The context closes the reader, not the connection. Retained PyArrow batches own their buffers. Do not reuse the borrowed connection while the result is open. Implement `ArrowEngine` for another Arrow-capable backend. Release workflows build platform wheels for PyPI.

## JavaScript / TypeScript

[Repository](https://github.com/OrchidDB/OrchidDB-js). Node.js 20+:

```sh
npm ci
npm run build
# Set ORCHIDDB_NATIVE_LIBRARY using the installation guide.
node examples/duckdb-wasm.mjs
```

Build the [native compiler](installation.md#shared-native-compiler) first. `Compiler.compile(request)` returns a SQL plan. Your `ExecutionEngine.execute(plan)` returns a schema, async Arrow batch iterator, and `close()` method. The `batches(result)` helper closes resources on completion, failure, and early exit.

The example uses DuckDB-Wasm's native Arrow stream. The binding itself runs in Node.js; it is not a browser compiler. Compilation is synchronous, so use a worker for latency-sensitive services. Pass exact signed 64-bit parameters as `bigint`, for example `9007199254740993n`. Unsafe integer Numbers, non-finite Numbers, and bigint values outside signed int64 are rejected, including inside nested parameters. Follow the producer's batch lifetime contract. npm packaging is configured for `@orchiddb/client`, not published.

## Elixir

[Repository](https://github.com/OrchidDB/OrchidDB-elixir). With Elixir, Erlang headers, `make`, and a C compiler:

```sh
mix deps.get
# Set ORCHIDDB_NATIVE_LIBRARY using the installation guide.
mix run examples/duckdb.exs
```

Build the [matching native compiler](installation.md#shared-native-compiler) first. `OrchidDB.compile(request)` returns `{:ok, plan}` or `{:error, reason}`. The C NIF runs compilation on a dirty CPU scheduler.

Optional `OrchidDB.query_arrow(connection, request, callback)` uses ADBC. Its Arrow C Stream pointer is valid only inside the callback and must not escape it. The connection stays caller-owned. Integration tests execute compiled queries through the real DuckDB ADBC driver and verify native Arrow ingestion, large integers, nulls, caller rollback, and cleanup after consumer errors. Hex source packaging is configured, not published.

## C++

[Repository](https://github.com/OrchidDB/OrchidDB-cpp). Requires C++17 and CMake:

```sh
cmake -S . -B build
cmake --build build
cmake --install build --prefix "$HOME/.local"
```

Build the [native compiler](installation.md#shared-native-compiler) and set its library path. Consumers use `find_package(OrchidDB CONFIG REQUIRED)` and link `OrchidDB::orchiddb`. `ExecutionEngine` supplies an `ArrowResult`; stream, schema, and batch use separate move-only RAII wrappers and release callbacks. Batches can outlive the stream.

The [DuckDB adapter](https://github.com/OrchidDB/OrchidDB-cpp/blob/main/examples/duckdb_engine.hpp) and integration example show ownership and rollback. `./scripts/test.sh` downloads a checksum-pinned test driver and runs them. CMake release archives include the native compiler; no release is published yet.

## Data lake access and performance

The [CLI](cli.md) bundles DuckDB and loads its official Iceberg extension by default. Embedded clients leave catalog, credentials, extensions, and SQL views to the application. Map a view over `iceberg_scan` like any other table.

Arrow avoids per-cell row-object conversion at the interface. It does not guarantee zero-copy, bounded memory, streaming database execution, or cancellation support for every driver. See [Arrow results](results.md) for lifetime rules and the optional managed runtime's result containers.
