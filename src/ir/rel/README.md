# Relational lowering ownership

`RelBackend` lowers Graph IR into executable DataFusion plans. The public API
stays in `mod.rs`; private modules group feature implementations so agents can
work on separate features without editing the dispatch file for every helper.

| Feature work | Owning files |
| --- | --- |
| Graph scans, property schemas, Arrow materialization, GraphValues | `scans.rs`; schema mappings remain in `mapping.rs` |
| Scalar expressions, scalar functions, cast/type classification | `expression.rs`; native cast functions remain in `casts.rs` |
| Constant evaluation, casts, language-specific value rendering | `constants.rs` |
| List/map expressions and their constant evaluation | `collection_expr.rs`; native list functions remain in `collections.rs` |
| Join/apply lowering, correlation keys, correlated limits/distinct | `joins.rs`; apply-specific traversal execution remains in `apply.rs` |
| Expansion and fixed paths | `expansion.rs`; variable-length traversal remains in `varlen.rs` |
| Collection-producing operators and quantifiers | `collection_operators.rs` |
| Branching and union/coalesce/choose | `branches.rs` |
| Projection and aggregate lowering | `projection.rs`, `aggregates.rs` |
| Gremlin traversal state and repeat | `gremlin.rs`, `gremlin_state.rs`, `repeat.rs` |
| SPARQL solution algebra | `sparql.rs`, `sparql/joins.rs`, `sparql/solution_ops.rs`, `sparql/aggregate.rs` |
| SPARQL expressions and typed RDF operations | `sparql/expressions.rs`, `sparql/term_ops.rs`; path lowering remains in `rdf_paths.rs` |
| SQL generation | `sql/unparse.rs`, `sql/literals.rs`; dialect and execution API stay in `sql/mod.rs` |
| SQL results and database execution | `sql/rows.rs`, existing `sql/*_exec.rs` |

Shared seams:

- `LoweringContext`, `LoweredNode`, `RelError`, and public backend types stay in
  `mod.rs`. Feature modules add inherent methods to the same context; no new
  runtime layer or public API is introduced.
- `operators.rs` dispatches Graph IR node variants. Add a dispatch arm there
  when introducing a new operator, and implement its feature in its owner.
- `columns.rs` owns the shared physical binding layout and projection helpers.
  Coordinate changes here when multiple languages must observe the same layout.
- `plan_walk.rs` owns Graph IR child traversal and diagnostic node names.
- Parent imports expose existing shared helper names to sibling modules. This
  preserves existing private call sites; helper visibility remains restricted
  to the relational module. New feature-local helpers should remain local.

The split preserves existing function bodies and test locations. Files were
not split merely to enforce a line-count limit; scan materialization and scalar
expression families deliberately remain together despite their size.

### Embedded language functions

Embedded DuckDB engines register pure vectorized Rust language functions and
use their expanded SQL lowering paths **by default**. Call
`GraphEngine::set_language_functions(false)` to opt out.
`DuckDbExecutor::set_language_functions(false)` provides the same control for
`MappedGraphEngine` and `RdfGraphEngine` callers. For separately compiled plans,
use `RelBackend::with_language_functions(true)` with a DuckDB executor; the
engine-neutral relational backend retains its portable default because other
SQL engines do not provide these functions.
Applications owning a raw connection can instead call
`sql::language_functions::register(&connection)` directly.

The current registered functions cover:

- Cypher equivalence keys for grouping, row distinct, and count-distinct over
  supported scalar, list, and plain struct values. Null keys remain SQL NULL,
  so count-distinct excludes them; nested nulls are part of the equivalence key.
- Gremlin case conversion, trim, reverse, UTF-16 length/substring, replace,
  concat, split, and conjoin, including supported local list operations.
  Arguments retain their types and nulls inside a struct. Simple terminal
  scalar pipelines can stay in DuckDB; partial islands preserve traverser state.
- SPARQL's existing RDF scalar contracts, including regex/replacement flags,
  IRI resolution, URI encoding, temporal comparison, and decimal division.
  This consolidates their registration; those operations already had DuckDB
  implementations before this option was added.

All implementations reuse the existing language kernels. Unsupported value
shapes, path/history consumers, distinct collections, and Cypher union-distinct
retain their existing execution boundaries. Other SQL engines are not assumed
to have these functions. Disabling the option stops emitting the calls; it does
not remove functions from caller-owned DuckDB catalogs. The internal
`__orchiddb_lang_*` names are reserved. No extension binary, network install,
new scheduler, or transaction model is introduced.

Run `cargo test --features duckdb --test language_functions` for result and plan
comparisons. Run `cargo run --features duckdb --example language_functions_bench`
for a five-trial, 10,000-row comparison that also asserts result parity.

The recorded development-build comparison is in
[`docs/language-functions-benchmark.json`](../../../docs/language-functions-benchmark.json).
Five-trial medians with DuckDB using one thread: Cypher grouping 279.5 → 113.6 ms,
Cypher count-distinct 270.2 → 111.2 ms, and Gremlin uppercase/trim 692.1 → 179.8 ms.
Those workloads contain 10,000 source rows and remove all residual language
kernel calls (the final result decoder remains). The two-row SPARQL control,
already fully pushed down, measured 45.9 → 49.7 ms. These are local unoptimized
measurements, not release-build performance guarantees. Every timed result is
compared with the disabled-option result.

Validation includes enabled/disabled result and SQL-plan comparisons, nested
keys, numeric precision, Unicode, null arguments, empty groups, transaction
rollback/error recovery, independent sessions, unsupported-shape fallback,
and a build/test run without DuckDB. The existing
`duckdb_udf_mappings::mapped_engine_owns_function_scope_across_async_queries`
test fails with “engine aggregate functions require relational execution”;
that same failure was reproduced on clean commit `e90dd92b` with the new option
absent. Its fix is outside this change.
