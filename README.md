# OrchidDB

An embedded graph query engine written in Rust. Compile Cypher, Gremlin and
SPARQL over mapped tables to SQL, then run it with your own database connection.

[Documentation](https://docs.orchiddb.com/) · [Website](https://orchiddb.com/) ·
[Java client](https://github.com/OrchidDB/OrchidDB-java) ·
[Examples](examples/) · [Conformance](https://docs.orchiddb.com/conformance.html)

## Compile to SQL

```toml
[dependencies]
orchiddb = { git = "https://github.com/OrchidDB/OrchidDB.git" }
```

The default library build has no DuckDB or PostgreSQL driver dependency. Supply
schema metadata and graph mappings to `compiler::compile`, or use the versioned
`compiler::compile_json` interface. The result contains SQL and output field
names. DuckDB and PostgreSQL SQL rendering are supported; unsupported operations
fail before execution.

```sh
cargo run --example compile_sql
```

[The runnable example](examples/compile_sql.rs) maps a `people` table to `Person`
nodes and prints SQL. [The compiler guide](docs/compiler.md) documents requests,
parameters, functions and limitations.

## Bring your own engine

Pass generated SQL directly to your driver, or implement
`execution::SqlSession` for a checked hand-off. Results can be driver-native rows,
Arrow batches or a cursor borrowing the session. Your application owns connection
setup, extensions, UDFs, transactions, schema discovery and caching. OrchidDB does
not install plugins or copy source tables in this path.

[The caller-owned DuckDB example](examples/duckdb-client/) demonstrates a streaming
cursor, a SQL function, and transaction ownership. DuckDB is that application’s
dependency.

Each compilation targets one engine. Multiple connections can be routed by the
application; cross-engine joins and ClickHouse SQL are not implemented.

## Optional managed runtime and CLI

The existing managed engine and CLI remain available with an explicit feature.
This feature bundles DuckDB and supports broader execution than standalone SQL.

```sh
cargo run --features duckdb --example managed_graph
cargo run --features duckdb --bin orchiddb -- --query 'RETURN 1 AS value'
```

See the [managed runtime](docs/runtime.md), [native JVM provider](docs/jvm.md),
and [release packaging guide](scripts/release/README.md).

## Development

```sh
cargo test --locked --test sql_compiler --test execution
cargo check --locked --lib
```

[Architecture](docs/architecture.md) · [Module ownership](docs/code_ownership.md) ·
[Verification](docs/verification.md) · [Roadmap](docs/roadmap.md)

The mdBook stays in [`website/docs`](website/docs/). The landing page and installer
are maintained separately in the private `OrchidDB/OrchidDB-landing` repository.
Pinned corpora, conformance evidence, production JVM modules and vendored parser
sources are intentional repository contents.

## License

See [LICENSE.md](LICENSE.md) for the applicable terms.
