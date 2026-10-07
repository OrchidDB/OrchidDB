# OrchidDB C++ client

Source builds use the shared core in this monorepo. See [build and test instructions](../README.md).

C++17 compiler binding plus move-only Arrow C stream results. Your application owns the database, connection, schema mappings, UDFs, extensions, transactions and caches. Query results stay in the application. Explicit statistics generation sends bounded collection batches to the shared core, which has no DuckDB dependency.

```cpp
#include <orchiddb/orchiddb.hpp>
orchiddb::Compiler compiler; // ORCHIDDB_NATIVE_LIBRARY, or OS shared-library path
orchiddb::Json request = /* compiler JSON v1 with query, tables and graph mappings */;
auto plan = compiler.compile(request);
auto result = engine.execute(plan); // your ExecutionEngine
const auto schema = result.schema();
while (auto batch = result.next()) {
    // ArrowArray struct, column buffers, offsets and validity bitmaps.
    consume(batch.get());
}
```

The [complete DuckDB integration example](tests/integration.cpp) and [borrowed connection adapter](examples/duckdb_engine.hpp) show real data, nulls, large integers, rollback and result ownership. DuckDB is a test/application dependency. This example uses DuckDB's Arrow query API, which materializes the query before exposing columnar batches; Arrow does not imply streaming query execution or zero-copy for all engines.

Implement `ExecutionEngine` (`id`, `dialect`, `execute`) to support another Arrow-capable backend. `ArrowResult` takes ownership of a supplied `ArrowArrayStream` and clears the source. Stream, schema and batch have independent release callbacks and move-only RAII ownership. Batches remain valid after advancing/closing the stream. Connections must outlive result production. Neither OrchidDB nor its adapters commit or close your connection. Stream callbacks are serialized by the caller; islands execute sequentially through `query_federated`. Postgres SQL rendering is supported. SQL-session tests use DuckDB; optional HTTP tests exercise Quickwit and Elasticsearch.

## Run against the published binary

For a packaged CMake distribution, extract its platform archive containing the compiler, headers, JSON dependency, and CMake configuration. For this checkout, use the source build below; the source import does not publish a new binary archive.

With your DuckDB 1.5.2 headers/library available, run the example:

```sh
cmake -S examples -B examples/build \
  -DCMAKE_PREFIX_PATH=/absolute/path/orchiddb-cpp-0.3.0-Darwin-arm64 \
  -DDUCKDB_INCLUDE_DIR=/path/to/duckdb/include \
  -DDUCKDB_LIBRARY=/path/to/libduckdb.dylib
cmake --build examples/build
export ORCHIDDB_NATIVE_LIBRARY=/absolute/path/orchiddb-cpp-0.3.0-Darwin-arm64/lib/liborchiddb_compiler.dylib
ctest --test-dir examples/build --output-on-failure
```

This uses the installed CMake package, not the client checkout's headers. It verifies Arrow values, nulls, 64-bit IDs, rollback, resource ownership and connection reuse.

## Build and test

Run `make native` at the repository root, then from `clients/cpp/`:

```sh
export ORCHIDDB_NATIVE_LIBRARY=/absolute/path/liborchiddb_compiler.dylib # .so on Linux
cmake -S . -B build -DORCHIDDB_BUILD_TESTS=ON \
  -DDUCKDB_INCLUDE_DIR=/path/to/duckdb/include -DDUCKDB_LIBRARY=/path/to/libduckdb.dylib
cmake --build build
ctest --test-dir build --output-on-failure
cmake --install build --prefix "$HOME/.local"
```

`ORCHIDDB_NATIVE_LIBRARY=/path/to/library ./scripts/test.sh` also fetches the test-only DuckDB driver and runs this suite on Linux/macOS. Set `DUCKDB_ROOT` to reuse a local driver.

Normal builds do not require DuckDB or Arrow C++. The standard Arrow C ABI header is vendored with its Apache license; nlohmann_json 3.12 is required (pinned fetch fallback is provided).

Installed usage: `find_package(OrchidDB CONFIG REQUIRED)` then `target_link_libraries(app PRIVATE OrchidDB::orchiddb)`. Point `ORCHIDDB_NATIVE_LIBRARY` at the compiler shared library. For compiler request fields see [compiler documentation](https://docs.orchiddb.com/sql-compiler.html). SQL parameters are specialized literals; recompile after relevant values/schema/mappings/functions change.

## Releases

Build and stage the shared native library locally with `make native` from the repository root. Package and publish only validated artifacts from a clean monorepo revision; GitHub Actions builds are not used.

## One-time statistics

```cpp
orchiddb::Statistics statistics(compiler);
orchiddb::generate_statistics(statistics, request, engine);
auto plan = statistics.compile(request);
auto results = orchiddb::query(statistics, request, engine);
statistics.save("statistics.json"); // Restore with statistics.load(path).
// statistics.report() explains collected and skipped work.
statistics.clear();
```

The shared Rust coordinator chooses collection SQL and computes every summary.
`ExecutionEngine::collect_statistics(work)` reuses the caller's session, enforcing
`timeout_ms`, `max_rows`, and `max_bytes`. It returns `{"rows":[...]}` or
`{"ipc":"base64 Arrow IPC stream"}`. Its default implementation reports bounded
execution as unsupported. The included DuckDB adapter implements collection,
transport caps and interruption deadlines. Alternatively, call
`statistics.generate(request, callback)` for application-owned adapters.

`CompiledQuery::diagnostics` retains the complete compiler response, including
statistics usage, plan estimates, and representation/layout decisions. Native
catalog handles avoid repeated serialization; snapshots are portable JSON.
Generation is explicit, may use multiple calls, and replaces a catalog only on
success. Compilation never reads data or refreshes statistics. `Statistics`
releases its catalog on destruction. `Compiler::statistics_command` exposes the
shared protocol for applications that manage collection and cancellation.

Collectors must set `truncated: true` when transport limits stop a response before
EOF; the coordinator retains those observations as a partial sample. It must not
infer a complete source row count from a shortened response.

## Permission pushdown

The request uses provider-neutral permission relation data. Add the permission source and its columns to `tables`, set top-level `authorization`, and attach `permission_scopes` to a node. The C++ helpers construct these JSON fields:

```cpp
auto direct = orchiddb::permission_relation("effective_grants", "document", "view");
auto project = orchiddb::permission_relation("effective_grants", "project", "view");
request["authorization"] = orchiddb::authorization("user", "alice");
request["nodes"][0]["permission_scopes"] = {
  orchiddb::permission_scope("id", direct),
  orchiddb::permission_scope("project_id", project)
};
```

The relation must already contain effective grants for the principal. Multiple scopes are OR membership filters, and duplicate or overlapping grants do not duplicate graph rows.

## Quickwit and Elasticsearch

```cypher
MATCH (q:Question)-[match:SIMILAR_TO]->(d:Document)
RETURN q.id, d.title, match.score
ORDER BY match.score DESC
```

Define `SIMILAR_TO` with `text.bm25(source.body, target.body)` and a per-source
limit. Register a `text` engine with dialect `quickwit` or `elasticsearch` in the
compiler request. The [search mapping documentation](https://docs.orchiddb.com/remote-engines.html)
shows both remote-owned documents and a remote index over SQL-owned documents.
The latter retrieves keys and scores remotely and reads document properties from
SQL.

Use an optional HTTP session alongside your existing SQL adapter:

```cpp
orchiddb::Compiler compiler;
DuckDBEngine local(connection); // caller-owned DuckDB connection
orchiddb::RemoteEngine text(compiler, "quickwit", {
  {"endpoint", "https://quickwit.example.com"},
  {"authentication", {{"type", "bearer"}, {"token", token}}},
  {"page_size", 1000}, {"request_timeout_ms", 30000}
});
orchiddb::query_federated(compiler, request,
  {{"local", &local}, {"text", &text}},
  [](orchiddb::ArrowResult& result) { /* consume result here */ });
text.clear_metadata_cache(); // after changing remote mappings
text.close(); // also released automatically by the destructor
```

Select `"elasticsearch"` to use Elasticsearch. The native library must include
the corresponding optional engine; SQL-only clients do not load the HTTP entry
point. The native library remains loaded for the process lifetime because its
shared runtime owns worker threads; individual HTTP sessions are released by
`close()` or destruction. Authentication and connections remain outside compiled plans. Basic,
bearer, and API-key authentication are supported. Request timeouts, pagination,
batch sizes, and response limits use the shared native transport. Partial or
oversized results fail explicitly.

Custom request engines implement `ExecutionEngine::execute_requests(requests,
columns)` and return `{"ipc": base64_arrow_stream}` or `{"rows": typed_rows}`.
The declared column types govern row encoding: JSON-domain cells contain JSON
text, SQL nulls are null cells, and integer cells may use decimal strings to
preserve precision. SQL engines continue returning Arrow C streams. Both
correlated SQL operations and remote requests compose through the same registry.
Use `execute_federated` when you already have a `CompiledQuery`. Session lifetimes
remain application-owned, including after a consumer throws an exception.

Local tests cover dependent SQL operations, empty inputs, and later SQL islands.
Set `ORCHIDDB_REMOTE_FIXTURE` to a generated live fixture JSON file when running
`ctest` to exercise both HTTP engines, pagination, correlated BM25, authoritative
SQL joins, JSON properties, and lifecycle behavior. No runtime tests run in CI.
