# Client APIs

Map your existing tables, compile a Cypher, Gremlin, or mapped SPARQL read query to SQL, then consume native Arrow batches from your own engine. The compiler receives schema and mapping metadata, not your data. Clients do not bundle DuckDB.

Version 0.1.0 is published. Each repository includes working source examples; packaged native compilers currently target macOS ARM64.

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
cargo add orchiddb-client@0.1.0 --no-default-features
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

In a client checkout, run `cargo run --manifest-path examples/Cargo.toml --features bundled` for mappings, a caller-defined function, a borrowed connection, and rollback. The standalone example resolves OrchidDB 0.1.0 from crates.io. The example owns its DuckDB dependency; `--features bundled` builds that driver.

## Java

[Repository](https://github.com/OrchidDB/OrchidDB-java) · [Arrow guide](https://github.com/OrchidDB/OrchidDB-java/blob/main/docs/arrow.md).

Use Java 17+ and add both published Maven dependencies:

```xml
<dependency>
  <groupId>com.orchiddb</groupId>
  <artifactId>orchiddb-java</artifactId>
  <version>0.1.0</version>
</dependency>
<dependency>
  <groupId>com.orchiddb</groupId>
  <artifactId>orchiddb-java</artifactId>
  <version>0.1.0</version>
  <classifier>macos-aarch64</classifier>
  <scope>runtime</scope>
</dependency>
```

```java
var compiler = io.orchiddb.NativeSqlCompiler.load();
```

Maven resolves the compiler JAR; `load()` verifies, extracts and loads it automatically. No manual binary download, library path, Rust toolchain or source checkout is needed. Version 0.1.0 includes **macOS ARM64 JVMs only**; other platform compiler JARs are not published for this version.

Run `./scripts/run-example.sh ArrowBatches` from a Java client checkout. Its standalone example POM downloads the published Maven packages and requires no Rust checkout. Run `./scripts/run-gremlin-example.sh` for the optional Gremlin module.

For a complete application example, see [ArrowBatches.java](https://github.com/OrchidDB/OrchidDB-java/blob/main/orchiddb-java/src/test/java/io/orchiddb/examples/ArrowBatches.java).

`JdbcEngine.withArrow` takes your JDBC connection, Arrow allocator, and driver export callback. `Graph.queryArrow` returns an `ArrowResult`. Java vectors are borrowed: consume them before advancing or closing the result. Copy/transfer data explicitly if it must outlive that scope. Your parent allocator and connection remain caller-owned.

Java requires an Arrow-compatible JVM setup, including `--add-opens=java.base/java.nio=ALL-UNNAMED`; the example launcher handles it. Supply your own JDBC driver and Arrow memory implementation as described in the Arrow guide; DuckDB JDBC is test-only in the client repository. The optional `com.orchiddb:orchiddb-gremlin:0.1.0` dependency adds fluent TinkerPop traversal integration; other clients accept Gremlin text.

## Python

[Repository and complete example](https://github.com/OrchidDB/OrchidDB-python). Install from PyPI:

```sh
python -m pip install "orchiddb[arrow]==0.1.0"
```

To run the checked-in example in a fresh virtual environment, use `python -m pip install -r examples/requirements.txt` followed by `python examples/people.py`.

The published wheel bundles the compiler for macOS 26+ on Apple Silicon. Supply your own database driver. Other platforms require a [source build](installation.md#shared-native-compiler).

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
npm install @orchiddb/client@0.1.0
```

From a client checkout, run `cd examples && npm ci && npm start`. Its package.json installs the published client and the application-owned DuckDB-Wasm driver.

The published package includes the macOS ARM64 compiler. `Compiler.compile(request)` returns a SQL plan. Your `ExecutionEngine.execute(plan)` returns a schema, async Arrow batch iterator, and `close()` method. The `batches(result)` helper closes resources on completion, failure, and early exit.

The example uses DuckDB-Wasm's native Arrow stream. The binding itself runs in Node.js; it is not a browser compiler. Compilation is synchronous, so use a worker for latency-sensitive services. Pass exact signed 64-bit parameters as `bigint`, for example `9007199254740993n`. Unsafe integer Numbers, non-finite Numbers, and bigint values outside signed int64 are rejected, including inside nested parameters. Follow the producer's batch lifetime contract. Version 0.1.0 is available on npm as `@orchiddb/client`.

## Elixir

[Repository](https://github.com/OrchidDB/OrchidDB-elixir). With Elixir, Erlang headers, `make`, and a C compiler:

```sh
cd examples
mix deps.get
# Set ORCHIDDB_NATIVE_LIBRARY to the downloaded compiler library.
mix run duckdb.exs
```

The standalone example uses Hex package `orchiddb` 0.1.0. Download the [macOS ARM64 compiler](https://github.com/OrchidDB/OrchidDB-native/releases/download/v0.1.0/orchiddb-compiler-v0.1.0-aarch64-apple-darwin.tar.gz), extract it, and point `ORCHIDDB_NATIVE_LIBRARY` at its `lib/liborchiddb_compiler.dylib`. Other platforms require a [source build](installation.md#shared-native-compiler). `OrchidDB.compile(request)` returns `{:ok, plan}` or `{:error, reason}`. The C NIF runs compilation on a dirty CPU scheduler.

Optional `OrchidDB.query_arrow(connection, request, callback)` uses ADBC. Its Arrow C Stream pointer is valid only inside the callback and must not escape it. The connection stays caller-owned. Integration tests execute compiled queries through the real DuckDB ADBC driver and verify native Arrow ingestion, large integers, nulls, caller rollback, and cleanup after consumer errors. Add `{:orchiddb, "~> 0.1.0"}` to your Mix dependencies to install the published Hex package. The native compiler is still required separately.

## C++

Download the [0.1.0 macOS ARM64 binary package](https://github.com/OrchidDB/OrchidDB-cpp/releases/download/v0.1.0/orchiddb-cpp-0.1.0-Darwin-arm64.tar.gz) ([SHA-256 checksums](https://github.com/OrchidDB/OrchidDB-cpp/releases/download/v0.1.0/SHA256SUMS)). It includes C++17 headers, CMake configuration, the JSON dependency, and the compiled native library. No Rust build is required. Other platforms can [build from source](https://github.com/OrchidDB/OrchidDB-cpp).

Extract the archive and pass its directory as `-DCMAKE_PREFIX_PATH=/path/to/extracted/package`. In your application's CMakeLists.txt:

```cmake
find_package(OrchidDB CONFIG REQUIRED)
target_link_libraries(your_app PRIVATE OrchidDB::orchiddb)
```

Set `ORCHIDDB_NATIVE_LIBRARY` to the extracted `lib/liborchiddb_compiler.dylib` before running your application.

`ExecutionEngine` supplies an `ArrowResult`; stream, schema, and batch use separate move-only RAII wrappers and release callbacks. Batches can outlive the stream.

The [DuckDB adapter](https://github.com/OrchidDB/OrchidDB-cpp/blob/main/examples/duckdb_engine.hpp) and integration example show ownership and rollback. To run against the downloaded package, configure `cmake -S examples -B examples/build` with `CMAKE_PREFIX_PATH` pointing to the extracted archive and `DUCKDB_INCLUDE_DIR` / `DUCKDB_LIBRARY` pointing to your DuckDB installation. Then run `cmake --build examples/build` and `ctest --test-dir examples/build --output-on-failure`. The published CMake archive includes the native compiler.

## Data lake access and performance

The [CLI](cli.md) bundles DuckDB and loads its official Iceberg extension by default. Embedded clients leave catalog, credentials, extensions, and SQL views to the application. Map a view over `iceberg_scan` like any other table.

Arrow avoids per-cell row-object conversion at the interface. It does not guarantee zero-copy, bounded memory, streaming database execution, or cancellation support for every driver. See [Arrow results](results.md) for lifetime rules and the optional managed runtime's result containers.
