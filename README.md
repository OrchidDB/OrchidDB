# Crabgraph

Crabgraph (`new-graph`) is an embedded graph query engine written in Rust. It
accepts Cypher, Gremlin, and SPARQL, lowers them into a shared Graph IR, and
executes relational query regions in DuckDB. A graph runtime handles supported
operations that cannot yet be lowered to SQL. Query results use Apache Arrow.

The project is under active development. Language coverage is partial, and
managed graphs are reconstructed in memory. See [current limitations](#current-limitations)
before choosing an execution mode.

## Quick start

Install a Rust toolchain with edition 2024 support and a native C/C++ build
toolchain. The default build compiles bundled DuckDB, so the first build can take
time. Cypher and Gremlin parser bindings are committed; normal builds do not
require Java or ANTLR generation.

From the repository root:

```sh
cargo build --locked

# Create a graph in a persistent DuckDB file.
cargo run --locked --bin crabgraph -- --database social.duckdb --query \
  "CREATE (:Person {name:'Alice'})-[:KNOWS]->(:Person {name:'Bob'})"

# Query the same graph in a separate process.
cargo run --locked --bin crabgraph -- --database social.duckdb --query \
  "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name"
```

The read returns Alice and Bob. Running the `CREATE` command again adds another
pair of nodes and a relationship.

## Command-line usage

The `crabgraph` binary executes one Cypher or Gremlin query per invocation. It
prints tab-separated headers and rows to stdout and the execution backend to
stderr. Errors produce a nonzero exit status.

```sh
# Query a managed graph with Gremlin.
cargo run --bin crabgraph -- --database social.duckdb --language gremlin \
  --query "g.V().hasLabel('Person').values('name')"

# Print a Graph IR plan without executing the query.
cargo run --bin crabgraph -- --explain \
  --query "MATCH (p:Person) RETURN p.name"

# Require a read to execute entirely in DuckDB.
cargo run --bin crabgraph -- --database social.duckdb --sql-only \
  --query "MATCH (p:Person) RETURN p.name"

# Read a query from a file or standard input.
cargo run --bin crabgraph -- --database social.duckdb --file query.cypher
printf 'RETURN 1 AS value' | cargo run --bin crabgraph
```

| Option | Behavior |
| --- | --- |
| `--database PATH` | Open a persistent DuckDB file; omitted means a fresh in-memory graph. |
| `--language LANG` | `cypher` (default) or `gremlin`. |
| `--query TEXT` | Supply query text directly. |
| `--file PATH` | Read query text from a file. |
| `--sql-only` | Reject reads that require graph runtime execution. |
| `--explain` | Print the Graph IR plan without executing. |
| `-h`, `--help` | Show CLI help. |

Query input precedence is `--query`, then `--file`, then stdin. SPARQL is available
through the Rust APIs; the CLI currently accepts Cypher and Gremlin.

## Rust API

The crate is named `new-graph` in Cargo and imported as `new_graph` in Rust.
Choose an engine based on where your data lives:

| API | Data source | Supported use |
| --- | --- | --- |
| `engine::GraphEngine` | A managed graph in memory or a DuckDB file | Cypher, Gremlin, and ontology-mapped SPARQL; managed mutations and transactions. |
| `mapped_engine::MappedGraphEngine` | Existing DuckDB tables or views with a `GraphMapping` | SQL-only graph reads and an explicit API for limited Cypher property updates. |
| `rdf_engine::RdfGraphEngine` | Existing DuckDB quad tables with an `RdfDatasetMapping` | Read-only SPARQL over mapped IRI quads. |

All three engine APIs require the default `duckdb` feature.

### Managed graph example

```rust
use std::collections::BTreeMap;
use new_graph::engine::GraphEngine;
use new_graph::ir::Value;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut graph = GraphEngine::in_memory()?;
    // Use GraphEngine::open("social.duckdb")? for persistent storage.

    graph.begin()?;
    graph.cypher_with_params(
        "CREATE (:Person {name:$name})-[:KNOWS]->(:Person {name:'Bob'})",
        &BTreeMap::from([("name".into(), Value::String("Alice".into()))]),
    ).await?;
    graph.commit()?;

    let result = graph.cypher(
        "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name",
    ).await?;

    println!("backend: {:?}", result.backend);
    println!("rows: {}", result.returned.batch.num_rows());
    Ok(())
}
```

Run the [complete example](examples/managed_graph.rs), which also prints the
returned cells:

```sh
cargo run --locked --example managed_graph
```

Cypher parameters are bound as typed values. Managed writes outside an explicit
transaction commit atomically; use `begin`, `commit`, and `rollback` to group
statements. Query results expose the Arrow batch, field names, backend, and
execution statistics.

For table mappings, native property updates, RDF datasets, persistence details,
and transaction behavior, see the [engine guide](docs/engine.md) and the
[mapped graph](tests/mapped_engine.rs), [mapped update](tests/mapped_updates.rs),
and [RDF engine](tests/rdf_engine.rs) integration examples.

## How it works

```text
Cypher / Gremlin / SPARQL
          |
    Parse and lower
          |
     Shared Graph IR
          |
  Relational lowering and planning
          |
   +------+----------------+
   |                       |
DuckDB SQL regions    Graph runtime
   |                       |
   +----------+------------+
              |
         Arrow results
```

The Graph IR preserves graph operations and language semantics through planning,
including expansion, paths, multiplicity, and traverser behavior. Relational
lowering identifies regions that can execute as SQL. DataFusion provides
relational planning integration.

Managed reads default to hybrid execution. `ReadMode::SqlOnly` requires complete
SQL lowering and returns an error when that is unavailable. Mapped graph and RDF
engine reads use SQL-only execution. Managed mutations execute in the graph
runtime and persist changed records transactionally in DuckDB.

## Current limitations

- Cypher, Gremlin, and SPARQL support is incomplete. Passing integration tests
  does not imply full language conformance.
- Managed graphs still require whole-graph reconstruction in memory when
  committed data changes. Incremental persistence does not provide disk-backed
  traversal or direct SQL execution of managed mutations.
- Mapped property updates support one `MATCH ... SET binding.property = expression`
  assignment with mapping restrictions. That API does not support `CREATE`,
  `MERGE`, or `DELETE`.
- The RDF quad adapter currently supports IRI terms. Typed literals, blank nodes,
  property paths, and federation require additional execution work.
- Streaming results, consistent cancellation across execution backends, and
  stable public errors remain release work.

See the [engine guide](docs/engine.md),
[language coverage review](docs/language_coverage_review.md), and
[read-query completion plan](docs/duckdb_read_query_completion_plan.md) for detail.
Coverage documents and corpus counts describe particular development snapshots.

## Development

See the [code ownership guide](docs/code_ownership.md) for module boundaries and parallel feature work.

```sh
# Formatting and unit tests.
cargo fmt --all -- --check
cargo test --locked --lib --bins

# Focused integration tests, using the larger thread stack configured in CI.
RUST_MIN_STACK=16777216 cargo test --locked \
  --test engine --test mapped_engine --test rdf_engine --test planner_integration

# Full test suite, including corpus tests.
RUST_MIN_STACK=16777216 cargo test --locked

# Check the library without the default DuckDB integration.
cargo check --locked --no-default-features --lib
```

The `duckdb` feature is enabled by default and required by the CLI. The optional
`postgres` feature enables a PostgreSQL SQL executor; it does not replace the
DuckDB-backed engine APIs. The [CI workflow](.github/workflows/ci.yml) lists the
integration tests run on pushes and pull requests.

## Repository layout

| Path | Contents |
| --- | --- |
| `src/bin/` | `crabgraph` command-line interface. |
| `src/engine.rs`, `src/storage.rs` | Managed graph execution and persistence. |
| `src/mapped_engine.rs`, `src/rdf_engine.rs` | Engines for existing relational and RDF data. |
| `src/language/`, `src/planner/` | Language frontends and planner APIs. |
| `src/grammar/` | Committed generated parser bindings. |
| `src/ir/` | Graph IR, interpreter, relational lowering, and SQL executors. |
| `examples/` | Runnable Rust examples. |
| `tests/`, `cases/` | Integration tests and imported language corpora. |
| `docs/` | Architecture, coverage reports, and development plans. |
| `languages/`, `specs/` | Grammars and reference material. |
| `website/` | Project website; see its [README](website/README.md). |

## License

Copyright (c) 2026 Daniel Henneberger. The project uses a custom commercial
license. See [LICENSE.md](LICENSE.md) for fees, permitted use, and restrictions.
