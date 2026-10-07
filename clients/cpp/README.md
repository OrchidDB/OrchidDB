# OrchidDB for C++

C++17 connection-based execution with move-only Arrow C stream results.
Register schema separately from query text:

```cpp
#include <orchiddb/orchiddb.hpp>
#include <fstream>

std::ifstream input("schema.json");
auto schema = orchiddb::Json::parse(input);
orchiddb::Connection graph(engine, schema); // caller-owned ExecutionEngine
// Returns Arrow results, not SQL.
auto result = graph.query(
    "MATCH (p:Person) WHERE p.name=$name RETURN p.name",
    {{"name", "Ada"}});
while (auto batch = result.next()) {
    // Consume batch.get(): ArrowArray with column buffers and validity bits.
}
```

The runnable [people example](examples/people.cpp) registers a schema, runs two
parameterized queries, and prints `Ada: 1` and `Grace: 2`.

Use [the DuckDB adapter](examples/duckdb_engine.hpp) with an existing
`duckdb_connection`. Engine adapters receive internal SQL work and return an
`ArrowResult`; connection and transaction ownership stay with the application.
Arrow stream, schema, and batch buffers have independent RAII lifetimes.
Only runtime implementation details remain in `orchiddb::detail`.

For a GitHub release archive, point CMake at the extracted prefix and set
`ORCHIDDB_NATIVE_LIBRARY` to its `lib/liborchiddb_compiler.so` (Linux) or
`lib/liborchiddb_compiler.dylib` (macOS).

Build the shared runtime with `make native` from the repository root. Configure
CMake with `clients/cpp`, then link `OrchidDB::orchiddb` plus your own database
driver. Set `ORCHIDDB_NATIVE_LIBRARY` to the staged library under `target/debug`
(`liborchiddb_compiler.dylib` on macOS or `.so` on Linux).

`graph.query(text, parameters, language)` supports Cypher, Gremlin, and SPARQL.
RDF rules, functions, permissions, and table routing belong in schema, not the
query. `query_federated` takes text plus named execution engines and a result
callback. `generate_statistics`, `save_statistics`, `load_statistics`, and
`clear_statistics` use the registered schema. RemoteEngine owns its HTTP session
and supports Quickwit/Elasticsearch independently of SQL connections.

[Source builds and API migration](../README.md)
