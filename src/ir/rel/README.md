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
