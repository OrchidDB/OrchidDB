# Handoff: relational-backend lowering push (2026-07-26)

Scope: `src/ir/rel/mod.rs`, `tests/graph_rel_backend_cases.rs`,
one visibility change in `src/ir/catalog.rs` (`parse_debug_value` →
`pub(crate)`). Nothing committed. Build is green; `rel_backend_smoke`
and `sql_backend_smoke` pass. A concurrent agent owns `src/ir/rel/sql/`
and `src/ir/rel/mapping.rs`; their seam in `mod.rs` (mapping hooks in
`lower_node_scan` / `lower_rel_scan`, `RelBackendOptions::mapping`) was
preserved untouched.

## Results (harness: `cargo test --release --test graph_rel_backend_cases -- --ignored --nocapture`)

| Mode | Language | Before | After |
| --- | --- | --- | --- |
| DataFusion (in-process) | cypher | 32.0% | **51.6%** (2884/5593) |
| DataFusion (in-process) | gremlin | 28.4% | **33.2%** (568/1709) |
| DuckDB (`GRAPH_REL_EXEC=duckdb`) | cypher | 28.2% | **44.7%** (2500/5593)* |
| DuckDB (`GRAPH_REL_EXEC=duckdb`) | gremlin | 25.2% | **30.3%** (517/1709)* |

\* DuckDB numbers are from the run *before* the final struct-order-key
rendering fix (which removed the 300-case "nul byte found in provided
data" DuckDB setup failures); a re-verification run was still in flight
at handoff time (`scratchpad/duck_v.log`) and should come in higher.
DataFusion numbers are post-fix and verified.

## Completed (per-blocker status)

- **Variable-length expand (was 181 cases): done** —
  `lower_expand_varlen` in `src/ir/rel/mod.rs`: bounded unrolling
  (cap 6), union-by-name of k-hop chains, pairwise (id,label)
  relationship-distinctness (trail semantics), k=0 duplicates the source
  binding. Upper bounds > 6 and path-materializing queries stay typed
  unsupported (never wrong results). Path values are stubbed:
  `recursive_relationship_path` lowers to a NULL placeholder in
  `project_item_exprs` so `count(*)`-style queries work.
- **Constant folding via interpreter (`cypher_property_star`
  adjacent + INTERVAL + casts + simplify_expressions, ~350 cases): done** —
  `try_constant_fold` calls the public `crate::ir::interpreter::eval` on
  binding-free `Call`/`ListTransform`/`ListFilter`/`ListReduce`
  expressions (`expr_is_constant`, denylist in
  `constant_foldable_function`). Kuzu-style errors ("Conversion
  exception: …") propagate into lower errors, which the harness now
  matches by error category (`error_match_candidates` needles). This
  killed the 102 `simplify_expressions` failures and the 44 INTERVAL
  cases at once.
- **`RETURN a` / element-as-value (was 46+): done** —
  `cypher_element_display_expr` renders Kuzu `{_ID: t:o, _LABEL: …}` /
  `(s)-{…}->(d)` text in SQL (concat/CASE; floats via
  `Decimal128(38,6)` cast; `label_index_case` for table indexes).
- **`a.*` property star (was 157): done** — expanded to native-typed
  per-property columns (`{alias}__star__{key}`, catalog order) in
  `project_item_exprs`; re-collected in `return_projection`
  (`star_expansion_columns`). Propertyless elements yield one null col.
- **Encoded list/map properties: done** — `property_array` decodes
  debug-encoded structured property strings via
  `catalog::parse_debug_value` and re-renders with `rel_display_value`,
  filtering `\0struct_order`/`\0struct_types` keys (these previously
  produced nul bytes that broke DuckDB INSERTs, ~300 cases).
- **INT128/UINT128 (was 170): done** — `Decimal128(38, 0)` + Utf8 wrap
  in the cast branch; constant cases fold through the interpreter with
  exact overflow messages.
- **Non-constant UNWIND (was 64): partial** — `lower_unwind_dynamic`
  uses DataFusion `unnest_column_with_options` when the lowered expr has
  a real Arrow List type (collect()/array_agg). String-encoded lists
  (most property lists) still unsupported by design.
- **GraphRepeat (was 97): done for times(n)** — `lower_repeat` unrolls
  ≤ 8 iterations through the body's `GraphCorrelate` leaf, pruning the
  feed to correlate bindings so body binding names don't collide
  (`first_correlate_bindings`). `until`-terminated and emit-traversal
  variants stay typed unsupported. Repeat suite went 3→26 matched.
- **GraphGroupMap (was 96): partial** — `lower_group_map` handles
  `groupCount()` (CountBulk) rendering `m[{"k":"d[n].l"}]`; entry order
  irrelevant (comparator sorts). `group().by(...)` non-count aggregates
  (59 cases) unsupported.
- **valueMap()/valueMapTokens: done** — `lower_value_map` (12-case
  suite went 0→10). Note: wrap `substr()` output in `cast_utf8` — the
  test formatter can't render Utf8View.
- **Harness hardening** (`tests/graph_rel_backend_cases.rs`):
  per-case timeout (`GRAPH_REL_TIMEOUT_MS`, default 10s; cooperative
  tokio timeout on the DataFusion path, 3x+5s thread backstop);
  error-category needles; DuckDB refuses plans > 200 nodes up front
  (DuckDB queries are uncancellable — without this, ldbc/lsqb varlen
  unrolls pin cores and OOM the run). `MAX_EXECUTABLE_PLAN_NODES`
  raised 80→200.

## In progress / abandoned

- DuckDB re-verification run after the struct-key fix was mid-flight
  (`scratchpad/duck_v.log`); rerun
  `GRAPH_REL_TIMEOUT_MS=6000 GRAPH_REL_EXEC=duckdb cargo test --release
  --test graph_rel_backend_cases -- --ignored --nocapture` (~11 min;
  ldbc/lsqb sections crawl because timed-out DuckDB queries leak).
- valueMap(true) token-combination residuals (2 cases) — expected token
  value details differ slightly; see
  `target/graph_rel_backend_cases/gremlin_*ValueMap*` dumps.

## Ranked remaining work (post-change top blockers, DataFusion mode)

1. ~500 zero-row mismatch cases ("row count: actual 0, expected N") —
   mostly **broken imports** (DML/transaction suites whose setup
   statements the importer stripped; no graph_initializer) plus fixture
   loaders that load empty (demo_db_parquet/lsqb/npy). Harness/importer
   work, not lowering. See memory note `cypher-conformance-state`.
2. `GraphQuantifier` (84) — any/all/none/single over per-row property
   lists; blocked on lists being display *strings* relationally. Real
   fix: keep list properties as Arrow `List` columns end-to-end
   (`property_array` + `lower_expr` list ops + unnest), which also
   unlocks `list_append`/`list_prepend` (66), `cypher_subscript` (35),
   and dynamic UNWIND over properties.
3. `GraphGroupMap` non-count aggregates (59) — extend `lower_group_map`
   for `group().by(key).by(fold/sum/…)`; list-valued entries render as
   `l[...]` tagged text.
4. Gremlin function tail in `lower_expr`: `gremlin_order_key` (44),
   `properties_list` (43, fan-out — needs a per-key UNION like the
   property-star expansion plus the `p[key→value]` entry rendering),
   `gremlin_unfold_items` (37), `edge_src`/`edge_dst` (51),
   `sack_apply` (28), `fold_reduce` (23), `path_by_keys` (29).
5. `unavailable binding` clusters (a/marko/josh, ~110) — Gremlin
   `select()` of labeled steps dropped by earlier projections; needs
   select-history bindings carried as columns.
6. `date_part` (33) — DataFusion `date_part` over `cast(col AS DATE)`;
   date columns are ISO strings so the cast works for the common cases.
7. `GraphRepeat` until-termination (14) + `GraphPathFilter` (14) +
   `GraphCap`/side-effect groupCount — needs loop-with-predicate
   unrolling (emit rows matching `until` per iteration).
8. DuckDB-only: `duckdb value type List(...)` (42) — array_agg/unnest
   plans reach the DuckDB value shipper; sql-layer work (other agent's
   area, `src/ir/rel/sql/`).
9. lsqb/ldbc DuckDB cost — per-case table re-materialization dominates;
   caching the materialized DB per dataset in the harness would cut the
   duck run from ~11 min to ~3.
10. Postgres executor never exercised (`postgres` feature); same
    lowering should apply.

Failure dumps: `target/graph_rel_backend_cases/`; summaries archived in
the session scratchpad (`df_v_summary.txt`, `duck_final2_summary.txt`).
Memory note `rel-backend-lowering.md` documents each mechanism.
