# OrchidDB

An embedded graph query engine written in Rust. Compile Cypher, Gremlin and
SPARQL over mapped tables to SQL, then run it with your own database connection.

[Documentation](https://docs.orchiddb.com/) · [Website](https://orchiddb.com/) ·
[Java client](https://github.com/OrchidDB/OrchidDB-java) ·
[Examples](examples/) · [Conformance](https://docs.orchiddb.com/conformance.html)

## Install the CLI

Version 0.1.0 is released. Install the macOS ARM64 CLI with bundled DuckDB:

```sh
curl -fsSL https://install.orchiddb.com | bash
export PATH="$HOME/.local/bin:$PATH"
orchiddb --version
```

The installer verifies release checksums. Follow the
[quickstart](https://docs.orchiddb.com/quickstart.html) to query tables without a
Rust toolchain or source checkout. See [installation](https://docs.orchiddb.com/installation.html)
for pinned versions, source builds, and published Rust, Python, JavaScript,
Java, Elixir, and C++ packages.

## Compile to SQL

```toml
[dependencies]
orchiddb = { version = "=0.1.0", default-features = false }
```

The default library build has no DuckDB or PostgreSQL driver dependency. Supply
schema metadata and graph mappings to `compiler::compile`, or use the versioned
`compiler::compile_json` interface. The result contains SQL and output field
names. DuckDB and PostgreSQL SQL rendering are supported; unsupported operations
fail before execution. SPARQL `SERVICE`, including `SERVICE SILENT`, is
unsupported; OrchidDB does not make remote SPARQL HTTP requests.

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

## Shared graph execution

Enable `duckdb` to use `GraphEngine` with either managed graph records or
application tables via `GraphEngine::mapped`. Cypher, Gremlin, and RDF queries
and updates share the mapping catalog, connection, and transaction owner.
`MappedGraphEngine` and `RdfGraphEngine` remain compatibility facades.
Graph IR is lowered to a DataFusion relational execution plan, with eligible SQL
regions running in DuckDB. There is no standalone Graph IR interpreter or
interpreter fallback.

```sh
cargo run --features duckdb --example managed_graph
cargo run --features duckdb --bin orchiddb -- --query 'RETURN 1 AS value'
```

See [RDF mappings](website/docs/content/rdf.md),
[logical tables and collections](website/docs/content/mapping-reference.md),
the [query runtime](docs/runtime.md), [native JVM provider](docs/jvm.md),
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
