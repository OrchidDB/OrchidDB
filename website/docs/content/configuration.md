# Configuration

Configure engine execution, Cargo features, and development checks for a reproducible integration.

## Engine settings

| Setting | API | Effect |
| --- | --- | --- |
| Storage path | `GraphEngine::open(path)` | Open a persistent graph file. |
| Ephemeral storage | `GraphEngine::in_memory()` | Create an in-memory graph. |
| Read policy | `set_read_mode(ReadMode::Hybrid)` | Combine SQL islands and graph runtime work. |
| SQL read policy | `set_read_mode(ReadMode::SqlOnly)` | Require complete SQL read execution. |
| SQL timeout | `set_sql_timeout(Duration)` | Bound individual DuckDB SQL queries and setup. |
| Mapping | `MappedGraphEngine::new(executor, mapping)` | Use the application's source schemas and graph vocabulary. |
| RDF dataset | `RdfGraphEngine::new(executor, mapping, name)` | Choose a configured quad dataset. |

Keep configuration near engine construction so applications can see the persistence and execution policy in one place.

## Environment variables

| Variable | Purpose |
| --- | --- |
| `ORCHIDDB_INTERPRETER_MAX_STEPS` | Set an operation budget for graph interpreter execution. |
| `GRAPH_PG_URL` | Connection URL for PostgreSQL executor entry points that load configuration from the environment. |
| `RUST_MIN_STACK` | Rust thread stack size; the repository's integration CI uses `16777216`. |

Treat database connection URLs as application secrets. The managed and mapped DuckDB engines are configured through their constructors and setters.

## Build features

The default library build has no database driver. Enable the optional `duckdb` feature for managed execution. It includes the bundled executor and the high-level engine APIs. The `postgres` feature includes the PostgreSQL SQL executor.

```sh
cargo build --locked
cargo build --locked --features postgres
cargo check --locked --no-default-features --lib
```

## Run development checks

```sh
cargo fmt --all -- --check
cargo test --locked --lib --bins
RUST_MIN_STACK=16777216 cargo test --locked --features duckdb \
  --test engine --test mapped_engine --test rdf_engine
```

See the repository's `docs/verification.md` for local validation and conformance reproduction.

## Reproduce an application setup

Keep the Cargo lockfile, schema definitions, graph mapping, ontology mapping, and query text under version control with the application. Parameter values are supplied at execution time. Record the execution policy and relevant timeout settings alongside deployment configuration.

For a standalone mapping artifact, use [TOML mappings](mapping-reference.md#toml-format). For schema evolution, update the registered Arrow schemas when physical column names, types, or nullability change.
