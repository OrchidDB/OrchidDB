# Clients and CLI

Clients register graph schema on a connection, then execute query text with
separate parameters. There is no public SQL-compilation API. SQL lowering,
function mapping, and federation planning are shared implementation details.
Applications own database sessions and transactions; closing graph results does
not close or commit the underlying database connection.

Schema configuration can be an application object loaded from JSON or YAML.
It contains source tables, graph mappings, function declarations, and optional
RDF, permission, remote-engine, and statistics metadata. Query text, language,
parameters, principal, and target dialect are not schema fields; passing them
in schema configuration is rejected. The target dialect comes from the engine.
The CLI reads JSON schema files. See [people.json](../cli/examples/people.json)
and the separate [people.cypher](../cli/examples/people.cypher).

## GitHub release assets

`make release` builds every client, the CLI, and the extension into one upload
directory with checksums. No registry publishing is needed. See the
[complete release command](../README.md#build-the-complete-github-release).

## Build this checkout

```sh
make clients-check
make cli
make native
```

`make native` stages the shared native runtime for Python, Node, and C++.
Elixir can load it through `ORCHIDDB_NATIVE_LIBRARY`. Java builds its JNI library
with `clients/java/scripts/build.sh`. Rust crates use the root workspace and lockfile.

This connection API is a source change, not a newly published package release.
Use this checkout instead of older registry packages. Original import revisions
are recorded in [imports.json](imports.json). Native SDK transport is ABI 2;
rebuild the native runtime and clients together. Published artifacts are unchanged.

## Interfaces

| Interface | Entry point |
| --- | --- |
| Python | `Connection(engine, schema).query(text, parameters=...)` |
| Node / TypeScript | `new Connection(engine, schema).query(text, options)` |
| Rust | `Connection::new(session, schema).query(Query::cypher(text))` |
| Java | `new OrchidDB(engine).graph(mapping).query(Query.cypher(text))` |
| Java JSON schema | `Connection.fromSchemaFile(path, engine).query(text)` |
| C++ | `Connection(engine, schema).query(text, parameters)` |
| Elixir | `OrchidDB.connect(engine, schema)` then `OrchidDB.query(connection, text)` |
| CLI | `orchiddb query 'MATCH ...' --schema schema.json` |

JSON serialization in the SDK's private native transport is not a customer query
API. Backend adapter interfaces receive internal SQL work in order to execute it;
applications receive rows or Arrow results.

## Local verification

```sh
make clients-test
# Foreign clients need make native first.
export ORCHIDDB_NATIVE_LIBRARY="$PWD/target/debug/liborchiddb_compiler.dylib"
# Linux: use liborchiddb_compiler.so.
PYTHONPATH=clients/python/src python -m pytest clients/python/tests
(cd clients/js && npm ci && npm test)
(cd clients/java && ./scripts/build.sh -Pgremlin)
(cd clients/elixir && mix deps.get && mix test)
clients/cpp/scripts/test.sh
```

PostgreSQL tests use `ORCHIDDB_TEST_PG_URL` for Python/Rust,
`ORCHIDDB_TEST_PG_URI` for Node, and `ORCHIDDB_TEST_PG_JDBC` for Java. Optional remote-service fixtures use
[scripts/clients_remote_fixture.py](../scripts/clients_remote_fixture.py).
Builds and tests run locally, never through GitHub Actions. The full shared-core
language conformance matrix is documented in the root README.
