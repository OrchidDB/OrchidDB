# Working on independent features

The module boundaries follow likely change ownership, not file-size targets.
Use this map to locate a feature and its tests. Most work should stay
inside that feature's implementation files; shared facades keep existing public
paths working and contain the contracts that need coordination.

## Ownership map

| Task | Primary owner | Shared seam / validation |
| --- | --- | --- |
| Gremlin syntax and parse-tree lowering | `src/language/gremlin/parser/`: source, predicates, filters, collections, control, projection, literals | `parser.rs` keeps public entry points and shared visitor state; `visitor.rs` contains shared callback implementations and dispatch, so coordinate overlapping callback edits. Parser and Gremlin planner tests. |
| Cypher syntax normalization | `src/language/cypher/parser/normalization.rs` and `normalization/` | The coordinator owns rewrite ordering and COUNT-subquery normalization. Parser entry points and existing `parser/lowering/` own AST construction. `cypher_parser_lowering`. |
| Cypher expression/projection semantics | `src/language/cypher/planner/lowering/project/`: scalar `expression`, grouping `aggregate`, correlated queries/comprehensions `materialize`, binding `scope`, rewriting `aliases`; `semantics/` owns validation and type analysis | Scope/context types and clause dispatch remain shared. Planner and list-comprehension tests. |
| SPARQL frontend algebra, terms, ontology | `src/language/sparql/`: `algebra`, `basic_patterns`, `ontology_lowering`, `planner_expression` (aggregates/EXISTS), `expression` (scalar expressions) | Planner context and algebra dispatch; SPARQL/RDF/mapped-engine tests. |
| Runtime scalar functions and casts | `src/ir/runtime/scalar/` | Runtime dispatch and alias registry are the routing seam; typed value/cast/helper families own implementations. Runtime tests and language execution tests. |
| Engine function catalogs, UDF mappings and dialect rewrites | `src/ir/functions/`, `src/ir/rel/sql/functions.rs` | [Function registration guide](architecture.md); mapped-engine configuration and scalar/aggregate/UDF regression tests. |
| Executable relational lowering | [Relational ownership map](../src/ir/rel/README.md) | `LoweringContext`, physical binding layout and operator dispatch; relational, SQL and language-specific DuckDB tests. |
| DataFusion extension adapter / optimizer interface | `src/ir/df/extensions.rs`, `conversion.rs`, `schema.rs` | `df.rs` preserves the public API. Conversion directions stay together; schema inference can change independently. HEP, planner and IR round-trip tests. |
| Catalog mutation semantics | `src/ir/catalog/mutations.rs` | Graph and overlay data structures stay in `catalog.rs`; read paths remain there. Catalog, DML, mutation-visibility tests. |
| Arrow property decoding / fixture table construction | `src/ir/catalog/values.rs`, `builders.rs` | Existing public catalog entry points remain reexported. Catalog and typed-property tests. |
| Snapshot format and persistence | `src/ir/catalog/snapshot/` | `binary.rs` owns value tags and codecs; `sections.rs` keeps graph-section encoders/decoders together. Envelope version and graph reconstruction stay in `snapshot.rs`. Snapshot and storage-integrity tests. |
| Explain formatting | `src/ir/plan/explain.rs` | Node definitions stay together in `plan.rs`; planner/explain tests. |

## Assigning work

- Give one agent both sides of a representation change: e.g. snapshot encode and
  decode, or Graph IR conversion and recovery. Keeping these together prevents
  two independently successful edits from producing incompatible formats.
- For a new language feature, assign its frontend, execution implementation and
  expected-result tests together. A directory boundary is not a reason to split
  one semantic change between agents.
- Existing operator/function implementations belong in their feature modules.
  New routing entries may require a small edit in the dispatcher; coordinate
  that seam instead of moving substantial implementations back into it.
- Keep new imports and private helpers with their feature. Facades retain some
  existing shared names for compatibility; they are not a destination for every
  new helper. Changes to shared context, IR types or physical binding layout
  should name the affected consumers explicitly.
- Preserve test names, fixtures, assertions and corpus accounting when doing
  structural work. Run the affected suites and the shared integration gate;
  unchanged function bodies alone do not replace compilation and execution.

## Deliberately kept together

Generated ANTLR grammar code is unchanged. The Graph IR node/type catalog,
cohesive pattern/repeat algorithms, and exhaustive language dispatch functions
remain large where splitting them would distribute one contract across files.
Conformance scoring and denominator logic remain with their harnesses. No tests
were removed or weakened to accommodate these extractions.

## Refactor validation (2026-09-23)

The final non-corpus test run passed **448 tests** across 44 targets, with four
existing diagnostic/progress tests ignored and the two full corpus sweeps
excluded. The library's 115 test identities match the pre-refactor baseline.
SPARQL property paths also passed 8/8 in an isolated run using normal parallel
test execution after the shared workspace stopped rebuilding executables.

```sh
CARGO_BUILD_JOBS=2 RUST_MIN_STACK=16777216 cargo test --tests --no-fail-fast -- \
  --test-threads=1
```

Source audits compared 1,403 original function/method definitions with the
refactored tree, allowing import, visibility and formatting adjustments; no
function bodies were changed. Existing test bodies and expectations were
preserved. All 13 generated grammar files match their original hashes.
`cargo check --no-default-features --lib` and `git diff --check` also pass.
This is regression evidence, not a newly measured full-corpus coverage score.
