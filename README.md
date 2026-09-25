# OrchidDB

[Website](https://orchiddb.com/) · [Documentation](https://docs.orchiddb.com/) · [Conformance](https://docs.orchiddb.com/conformance.html)

OrchidDB is an embedded graph engine in Rust built on DuckDB and
DataFusion. Cypher, Gremlin, and SPARQL frontends produce a shared Graph IR.
Relational query regions execute in DuckDB, with a graph runtime for operations
that do not yet lower to SQL.

`GraphEngine` provides managed graph persistence, transactions, typed Cypher
parameters, and Arrow results. `MappedGraphEngine` runs SQL-only graph reads
over existing DuckDB tables and views, plus explicit native SQL property updates.
`RdfGraphEngine` runs read-only SPARQL queries over mapped RDF quad tables in
DuckDB. Its initial quad mapping supports IRI terms, variable predicates, and
default or named graph selection; other RDF term kinds remain explicit errors.
Language coverage is partial. Managed writes use the graph runtime and persist
changed records transactionally in DuckDB, with checkpoints for compaction. See [the engine API and its current limits](docs/engine.md).

For caller-owned SQL engines, [`compiler`](docs/compiler.md) generates SQL from
schema metadata without connecting to or bundling DuckDB. Build this API with
`default-features = false`; client bindings own connection setup, execution,
transactions, and results. DuckDB and PostgreSQL SQL rendering are available;
cross-engine federation is not implemented.

The Rust package and CLI are named `orchiddb`. Build from this repository;
no crates.io release has been published. See [installation](https://docs.orchiddb.com/installation.html).

```sh
cargo run --bin orchiddb -- --database social.duckdb --query \
  "CREATE (:Person {name:'Alice'})-[:KNOWS]->(:Person {name:'Bob'})"
cargo run --bin orchiddb -- --database social.duckdb --query \
  "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name"
cargo run --example managed_graph
```

The project is currently implemented in Rust. It contains parsers, ASTs,
semantic lowering, a Graph IR, planner facades, a small interpreter, and adapter
code that represents Graph IR operators as relational extension nodes. The
design target is Calcite-style planning: keep graph semantics explicit while
allowing relational optimization and SQL island generation where graph operators
can be safely expressed as SQL.

## What This Project Does

`orchiddb` is exploring this pipeline:

```text
Cypher / Gremlin frontend
  -> parser and language AST
  -> semantic lowering
  -> Graph IR logical plan
  -> graph rewrites and relational optimizer integration
  -> SQL islands
  -> SQL engine execution
```

The main idea is that graph languages should not be translated directly into one
large SQL string. Instead, the planner preserves graph operations such as node
scans, edge scans, expands, path semantics, traverser semantics, filters,
projects, aggregation, optional matches, repeats, and apply-style subqueries in
a graph-specific IR. Later passes can then decide which regions are relational
enough to lower into SQL islands and which regions need graph-native execution.

## Current Status

Implemented pieces include:

- Cypher and Gremlin language modules with AST, parser, semantic, and planner
  code.
- Committed ANTLR-generated parser bindings for Cypher and Gremlin, so normal
  Rust builds do not require Java or ANTLR generation.
- A shared Graph IR under `src/ir` with graph operators, expressions, catalog
  types, execution policy, and plan formatting.
- Public planner facades under `src/planner` for lowering Cypher and Gremlin
  planner-input ASTs into Graph IR.
- Managed execution lowers Graph IR to a SQL IR DAG. DuckDB runs eligible SQL
  regions; DataFusion runs residual operators and JVM compute kernels.
- Arrow-backed graph operations preserve element identities, paths, and typed
  values across execution boundaries.
- Transform rules under `src/transform` for graph plan normalization.
- Case runners and integration tests for Gremlin/TinkerPop and Cypher/Ladybug
  corpora.

Full SQL lowering, direct SQL managed mutations, disk-backed execution,
and complete language conformance remain active development areas. Execution
metadata distinguishes DuckDB execution from graph runtime work; SQL-only
mode fails explicitly when a read cannot be lowered.

## Build

Install a recent Rust toolchain with edition 2024 support, then build the crate:

```bash
cargo build
```

Run the test suite:

```bash
cargo test
```

Run narrower test targets while iterating:

```bash
cargo test --test planner_integration
cargo test --test hep_optimization
cargo test --test gremlin_planner
```

The repository commits generated parser modules under `src/grammar/generated`,
so a normal build should only need Cargo and the Rust toolchain.

## Release packages

The [release workflow](.github/workflows/release.yml) builds CLI archives for
Linux, macOS (Intel and Apple Silicon), and Windows when a version tag is pushed.
It attaches checksummed archives to a draft GitHub release after native builds
and CLI smoke checks succeed. See [release instructions](scripts/release/README.md)
for tagging, manual runs, local packaging, and the proposed crates.io name.

## Repository Layout

```text
src/
  grammar/      Committed ANTLR parser bindings.
  syntax/       Parser entry points and syntax errors.
  language/     Cypher and Gremlin AST, parser, semantics, and planner modules.
  ir/           Graph IR nodes, expressions, catalog, policies, interpreter,
                bridge lowering, and relational adapter.
  planner/      Public CypherPlanner and GremlinPlanner facades.
  transform/    Graph IR rewrite pipeline and rules.

docs/           Graph IR design examples and planning notes.
tests/          Integration tests and corpus case runners.
cases/          Imported Cypher and Gremlin test cases.
languages/      Source grammars and language reference material.
specs/          Specification PDFs and research references.
```

## Design Notes

The Graph IR is meant to be the contract between language frontends and later
optimization/lowering stages. It tracks language-specific semantics that are
easy to lose in direct SQL translation, including result shape, multiplicity,
missing-property behavior, path mode, match mode, and traverser behavior.

The SQL-island direction is to identify subplans that can be represented as
relational SQL, lower those subplans, and leave graph-specific operators intact
where SQL would be incorrect or too awkward. This is the layer that is expected
to integrate with Calcite-style planner rules as the project matures.

For examples of the intended logical operators and semantic policy model, see
`docs/graph_ir_language_examples_v0_2_draft.md`.

For the read-only Cypher, SPARQL, and Gremlin completion roadmap, architecture
changes, and subagent work packages, see the
[DuckDB read-query completion plan](docs/duckdb_read_query_completion_plan.md).

For module boundaries and parallel feature work, see the
[code ownership guide](docs/code_ownership.md).

## License

`orchiddb` is licensed by Daniel Henneberger under a custom GPL-3.0-only license.
See `LICENSE.md` for the full terms.

License fees are based on organization size:

| Organization size | License fee |
| --- | ---: |
| 0-5 people | $100 |
| 5-50 people | $10,000 |
| More than 50 people | Requires a separate contract. Contact Daniel Henneberger. |
