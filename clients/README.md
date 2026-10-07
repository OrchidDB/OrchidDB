# Clients and CLI

These clients and the standalone CLI share the core in this repository. Clone
`https://github.com/OrchidDB/OrchidDB.git` once. Commands below start at the
repository root. No sibling checkouts, client Git submodules, or remote source
pins are required. Original import revisions are recorded in [imports.json](imports.json).

## Build

```sh
cargo build --locked -p orchiddb-client
make cli
make native
```

`make native` builds the C ABI once and stages it for Python, JavaScript, and
C++. It generates ignored `CORE_REVISION` metadata from the actual library.
Elixir can use that same library via `ORCHIDDB_NATIVE_LIBRARY`. Java has a separate
JNI binding that also depends on the local core. Rust crates share the root
Cargo workspace, lockfile, and target cache; the default Cargo build remains the
core library. Set `CARGO_TARGET_DIR` to reuse a different cache.

## Local tests

```sh
make clients-check
make clients-test

# Build and stage the native library before foreign-language tests.
make native
export ORCHIDDB_NATIVE_LIBRARY="$PWD/target/debug/liborchiddb_compiler.dylib"
# On Linux, use liborchiddb_compiler.so instead.

python3 -m venv clients/python/.venv
clients/python/.venv/bin/pip install -e 'clients/python[test]'
clients/python/.venv/bin/python -m pytest clients/python/tests

(cd clients/js && npm ci && npm test)
(cd clients/java && ./scripts/build.sh)
(cd clients/elixir && mix deps.get && mix test)
clients/cpp/scripts/test.sh
```

Java needs Java 17+ and Maven; select a suitable `JAVA_HOME`. Elixir needs Erlang,
Elixir 1.15+, and a C compiler. C++ needs CMake and a C++17 compiler; its test
script downloads a pinned DuckDB driver unless `DUCKDB_ROOT` supplies it. The CLI
has a bundled DuckDB driver and its Iceberg tests may download the official
extension. PostgreSQL and external service tests use the environment settings
in each client and skip when those services are not configured.

## Source and releases

GitHub source publication uses the single OrchidDB repository. Builds and tests
run locally; imported per-client GitHub Actions workflows are not used. Existing
package names and versions remain unchanged. This source consolidation does not
republish packages or move existing release tags.

Package only clean, validated builds. `CORE_REVISION` is generated build metadata,
not a committed cross-repository pin. Native packages record the actual source
revision and checksum. Keep this metadata with bundled libraries; never substitute
another library under an existing release version. The extension's local release
pipeline remains under `scripts/release/`.

## Backend conformance

The full pinned suites are run by `conformance/upstream/engine_matrix.py` after
building `conformance/build-orchiddb.sh` and the Java fixture adapter. Supply
`ORCHIDDB_TEST_PG_URL` for a dedicated PostgreSQL test database. This matrix
requires all Cypher and Gremlin cases to pass and allows only the existing
SPARQL omissions whose IDs and case hashes match the committed baseline.
It also rejects SQL execution on the wrong engine and Gremlin instance restarts.
