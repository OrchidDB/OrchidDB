# Resumed Gremlin traverser work (G1)

Date: 2026-09-23. Status: **focused continuation complete; broad G1 package incomplete**.

This records the interrupted traverser work resumed from Claude agent
`a332decf3eaf741fc`. It is not a full Gremlin conformance report and does not
change the historical coverage numbers in `duckdb_read_query_completion_plan.md`.
Repeat and query-local side effects belong to the separate G2 continuation.

## Completed scope

The interrupted patch already included relational helpers for per-input child
traversals: occurrence-keyed apply joins, child output ownership, empty count
results, primitive scalar/element mixed output, and element-preserving selection.
Its five existing focused tests passed when the continuation began. Those changes
were retained and revalidated.

The continuation completed these additional fixes:

- `edge_src`/`edge_dst` projections now attach vertex properties through a node
  scan joined on endpoint ID **and label**. Previously
  `g.V().outE().inV().values('name')` returned no rows because the endpoint
  projection carried identity without properties. `inV()` and `outV()` now
  support the tested subsequent property reads, labeled selection, and navigation.
- Expression-based label rebinding removes the previous binding's entire
  physical representation before projecting its replacement. Repeating `as('a')`
  previously produced duplicate `a__id` columns. The tested `Pop.last` query now
  returns the latest bound vertices.
- The existing history capability check is now reached for repeated labels.
  The tested `Pop.first` query fails explicitly with a label-history unsupported
  error. Historical selection is **not implemented** by this change.
- Added assertions that injected null traversers remain productive and that
  `coalesce(identity(), constant('fallback'))` retains a productive null.

Implementation files are `src/ir/rel/gremlin.rs` and minimal hooks in
`src/ir/rel/mod.rs` (`lower_project` and `lower_bind`). Focused tests are in
`tests/gremlin_traverser_duckdb.rs`. Other working-tree edits in shared files
belong to concurrent or earlier work and are not all attributable to G1.

## Validation

Executed against DuckDB, with expected results written independently in the tests:

```sh
RUST_MIN_STACK=16777216 cargo test --release --test gremlin_traverser_duckdb -- --nocapture
```

Result: **8 passed, 0 failed, 1 ignored**. The ignored test is the opt-in
interpreter/DuckDB diagnostic `probe`; it is not a conformance case.

| Test | Evidence |
| --- | --- |
| `scan_view_is_rebuilt_when_columns_change_over_same_content` | Reused scan data receives the current binding column names |
| `by_modulator_is_evaluated_per_traverser` | Group keys and child values are computed per traverser |
| `local_count_keeps_traversers_with_empty_children` | Empty children yield count zero, including filtering and branching |
| `child_output_replaces_current_per_traverser` | Child scalar results replace prior element bindings |
| `choose_mixes_element_and_scalar_traversers` | Both representations survive in one result stream |
| `edge_endpoints_preserve_vertex_properties_and_labels` | `inV`, `outV`, labeled selection, multiplicity, and subsequent navigation |
| `select_preserves_element_shape_and_rejects_missing_label_history` | Element selection, latest repeated binding, explicit unsupported history |
| `null_traversers_are_productive` | Null injection and productive-null coalesce |

A concurrent shared null-safe-join change briefly caused DuckDB SQL generation
failures. Its owner restored the supported equality-or-both-null expression;
the passing result above was obtained afterward. No full corpus sweep was run
for this slice. The stack override follows the project's documented integration
setting and is not evidence that default-stack compilation is safe.

## Regression review

Reviewed the continuation's endpoint and rebinding changes after the focused run:

- Endpoint lookup includes both label and ID, so equal numeric IDs in different
  vertex labels do not match each other. The temporary scan binding is checked
  against existing physical columns before use.
- The join keeps upstream columns and copies only vertex property columns into
  the projected endpoint binding. It does not replace edge-label bindings or
  append a second set of endpoint identity columns.
- No distinct or grouping operation is introduced. Tests retain repeated
  endpoint results from different edges and verify a further traversal step.
- Rebinding excludes only the named binding and its value-tag companion;
  unrelated bindings remain available. Endpoint hydration is dispatched only
  for Gremlin projects.
- Existing focused correlation, grouping, mixed-value, and empty-child tests
  remained passing. `git diff --check` passed for the reviewed source/test paths.

No additional blocking defect was found in this focused review. The tests use
the in-memory modern fixture as the SQL source; mapped endpoint hydration,
large-graph plan cost, and broader language regressions were not independently
established by this run. Endpoint hydration adds a node-scan join per endpoint
projection and needs scale evaluation before making performance claims.

## Remaining G1 gaps

The broader package still requires independent implementation and/or conformance
validation for full traverser schema and bulk contracts; complete repeated-label
history (`Pop.first`, `Pop.all`, and related history-dependent behavior); general
path objects/labels; multi-properties and meta-properties; full property-object,
`valueMap`/`elementMap`, local/global ordering and slicing semantics; and all
remaining branch/group combinations. This list does not assert that every
individual operation is currently unsupported; these families are not completed
or proved by the eight tests above.

Nullable label/map fallback and nonprimitive heterogeneous values retain explicit
limitations in the helpers. There is no full read-manifest denominator, official
expected-result sweep, zero-regression corpus comparison, or complete G1 claim
from this continuation.
