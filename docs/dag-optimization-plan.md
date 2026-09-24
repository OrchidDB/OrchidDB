# SQL IR DAG optimization plan

## Contract

Keep one Crabgraph execution path: language frontend → Graph IR → SQL IR DAG → DuckDB regions and DataFusion residual operators. Preserve exact values, ordering, bulk, mutation visibility, transaction rollback, callback invocation counts, and error behavior. Run tests locally; GitHub Actions publishes only static documentation and recorded evidence.

## Measurement and acceptance

1. Capture a reproducible baseline from the current commit. Repeated workloads use the same fixtures, query sequence, build profile, and warmup. Record total time and per-query medians, and verify results on every repetition.
2. Instrument lowering, SQL-region preparation, physical planning, and execution separately. Include region/kernel counts. Do not infer engine performance from adapter timing alone.
3. Require all 1,511 pinned upstream Gremlin scenarios to pass on one uninterrupted Crabgraph instance. Run mapped-write regression tests for table destinations, identity, and transaction semantics.
4. The primary performance target is total full-suite runtime with all 1,511 scenarios passing. Track mean and median scenario time as well as the total, so improvements benefit common queries rather than only outliers. Prioritize shared planning, source preparation, transport, and execution overhead. Small benchmarks are diagnostic only. Batch related improvements before running the whole suite; use focused checks during implementation. Retain changes only when full-suite time improves without losing conformance. Record build profile and before/after revisions; do not compare different build profiles as an optimization gain.

## Implementation sequence

1. Measure first. Identify fixed overhead versus row processing and SQL preparation costs.
2. Remove measured overhead with narrow changes. Candidates include repeated session construction, repeated source collection, and redundant row serialization between compatible residual kernels.
3. Add DAG rewrites with explicit eligibility. Apply fusion inside correlated subplans as well as the outer plan; fuse adjacent read kernels and their sources to avoid intermediate Arrow encoding and task scheduling. Reuse immutable transport schemas and serialization buffers. Preserve SQL scope barriers, ordering, side effects, and duplicate evaluation. Run DataFusion logical optimization before region placement; opaque kernels retain their pushdown barriers. Fuse compatible unary residual kernels without reordering evaluation.
4. Establish typed operator contracts before broader predicate movement or cost-based placement. Stateful/JVM/write operators remain boundaries unless their semantics prove a rewrite safe. Full typed-column migration, plan caching, and set-based write batching are separate follow-on changes if measurements justify them.
5. Re-run matched before/after benchmarks and the complete Gremlin suite; commit and push code and evidence. Publish through the existing static workflow.

## Existing leaderboard

For Crabgraph only, add **Passed runtime**, the sum of `elapsed_ms` for records whose status is exactly `pass`. Exclude failed, skipped, unsupported, timeout, and adapter-error cases. Label this as recorded scenario time, including setup/assertion overhead, not pure database execution or suite wall-clock time. Show missing timing explicitly; never treat missing timing as zero. Keep ranking by passed scenarios and retain one Crabgraph result. Other engines do not receive a runtime value.

## Broad per-query overhead batch

- Read existing MemTable Arrow batches directly during SQL source preparation, retaining complete source contents and SQL filters/projections. Other providers still use DataFusion, with one preparation context per call.
- Reuse a single query-scoped DataFusion state snapshot for logical and physical planning.
- Preserve Arrow buffers when the result already consists of one batch.
- Hash Arrow types directly instead of allocating temporary debug strings for scan-cache keys.
- Run background conformance against copied, committed binaries while subsequent source edits continue. Each result belongs to its captured revision; never merge cases across revisions. Avoid compilation during timing-sensitive runs.

## Second logical optimization batch

Implement all five changes before the next full conformance run:

1. **Required bindings:** residual SQL IR operators declare dependencies and writes. Propagate required return bindings through projections, filters, expansion, unwind, ordering, and slicing. Prune inside the physical kernel before encoding Arrow output. Opaque functions, JVM operators, reducers, and writes retain their complete inputs.
2. **Predicate movement:** move total, deterministic predicates across compatible total read operators when their dependencies are not overwritten. Scope replacement, effects, fallible expressions, ordering, and limits remain fences.
3. **Projection simplification:** compose adjacent binding/literal projections; preserve simultaneous alias evaluation, alias shadowing, missing-versus-null behavior, and bulk. Precompute common-expression slots and identity-copy elimination once during planning. Fallible/opaque expressions preserve their evaluation order and count.
4. **Shared SQL-island analysis:** analyze purity and complexity bottom-up once. Memoize lowering for original immutable nodes within a query and Gremlin label scope. Failed parent attempts reuse already-lowered children. Use borrowed nodes and unique source aliases; correlated scopes and temporary rewritten nodes bypass the memo. No graph data or plans survive into another query.
5. **Correlated batching:** distribute eligible pure row-wise subplans over one frontier, with a private identity per input occurrence. Group results back in original input order before applying existing inner/optional/semi/anti/scalar semantics. Reset child bulk to one and apply parent bulk on the join. Reductions, scope replacement, effects, and opaque callbacks keep their existing execution behavior.

Focused checks cover duplicate values and fan-out, non-unit bulk, projection shadowing, null/missing fields, suppressed-error hazards, mutation visibility, rollback, and query-local cache lifetime. After this complete batch, run the full suite locally against a committed build. Compare total, mean, median, and matched scenario improvements against the 1,511-pass bcfad49 baseline (189.31 seconds wall time).

## Advanced SQL assessment — September 24, 2026

Status: source assessment and proposed implementation sequence; the changes below are not implemented or benchmarked. Baseline is the completed `4fbcab3` batch: 1,511 passing scenarios, 177.385 seconds summed scenario time and 178.561 seconds wall time.

### Where boundaries form

`src/ir/rel/dag.rs::partition` already attempts the largest eligible subtree before visiting its children. Nested SELECTs, aliases, and CTE scopes within that statement are not separate database executions. Several scopes in collection and distinct lowering are required for valid SQL name resolution and window evaluation.

The earlier limitation is lowering into opaque residual kernels in `src/ir/rel/runtime.rs`. Those operations no longer expose enough relational meaning for broad rewrites. The region selector also excludes extension nodes and certain functions. Preserve typed relational operations through optimization, then choose DuckDB regions and DataFusion residual execution. Measure actual statement count, repeated sources, transferred rows/bytes, and planning time before attributing cost to SQL nesting.

### SQL opportunities

| Construct | Current source evidence | Proposed change and eligibility |
| --- | --- | --- |
| `SELECT DISTINCT` | `joins.rs::keyed_distinct` generates an ordinal window, a partitioned rank window, and a final sort. | Use ordinary relational distinct when only the distinct values are observable and ordering/representative selection is irrelevant. Project the semantic keys first; hidden path, bulk, and occurrence columns must not accidentally become distinct keys. Remove distinct entirely only when propagated uniqueness proves it redundant. |
| `DISTINCT ON` | Keyed dedup currently retains a representative through windows. | Consider as an alternative for first-per-key selection with explicit required ordering. Restore final output order separately where needed. Compare its physical plan with the window form; syntax alone does not establish a speedup. |
| `UNNEST` | `collection_operators.rs::lower_unwind_dynamic` already lowers typed lists through DataFusion Unnest; outer unwind adjusts empty/null behavior. `sql/mod.rs` repairs output aliases. | Extend typed list representation across residual boundaries. Push independent filters/projections before expansion, and keep expansion plus consumers in one SQL region. Preserve element position when observable. Sequential unwinds can mean a Cartesian product: DuckDB's side-by-side SELECT unnests zip and pad, so they cannot simply be combined. |
| Scalar subqueries | `joins.rs::lower_apply` constructs relational correlation; `apply.rs::guard_scalar_cardinality` uses ranking and an error guard before a left apply. | Keep scalar-subquery semantics available in SQL IR and assess native SQL emission against the current join form. DuckDB can decorrelate subqueries. Preserve zero-row null and multiple-row error behavior; never replace the guard with arbitrary `LIMIT 1`. Eliminate guards only with proven at-most-one cardinality. |
| `EXISTS` / `NOT EXISTS` | `lower_existence_apply` already uses semi/anti joins; some Cypher existence patterns are simplified earlier. | Extend recognition to more pure existence-only children. Avoid producing unused child bindings or multiplicity. Retain separate semantics for null-sensitive `IN` / `NOT IN`; an anti join is not generally equivalent to `NOT IN`. |
| `LATERAL` and correlated aggregation | Apply lowering already absorbs or joins correlated inputs, and residual execution batches eligible row-wise children. | Keep eligible aggregation, top-k, and optional expansion in relational IR. Compare native correlated SQL with explicit grouped joins. Preserve per-occurrence identity, zero-child results, parent bulk, and scalar cardinality. |
| Windows and `QUALIFY` | `joins.rs` and `apply.rs` already use windows for dedup, partitioned limits, and correlation identity. | Share compatible partition/order specifications and replace per-parent execution with partitioned top-k. `QUALIFY` is an emission convenience, not evidence of a faster plan. Preserve frame, null ordering, ties, and occurrence identity. |
| Aggregate `FILTER`, distinct aggregates, ordered collection | `operators.rs` and `collection_operators.rs` already emit aggregate expressions, distinct aggregates, filtered collection, and ordered array aggregation. | Combine compatible aggregates over the same input/grouping; avoid separate scans and collect-then-unwind when intermediate collection is unobservable. Preserve empty-group behavior, null handling, bulk, ordering, and fallible expression evaluation. |
| List/struct expressions | Typed list lowering exists, but some graph values remain opaque. | Retain typed nested values where possible. Assess direct list operations for list-local work rather than expanding and regrouping. Do not move opaque JVM callbacks or change their invocation counts. |
| CTE reuse / materialization | Planner memoization reuses lowering work; that does not prove physical source execution is shared. | Identify repeated pure relational subplans. Compare inlining with query-local shared materialization using reuse count and input size. Avoid forced materialization of every scope, which can inhibit pushdown and add memory/transfer cost. |
| `ASOF JOIN` | Proposed temporal specialization; not established by this assessment as an existing lowering. | Recognize nearest-prior/next temporal lookup only when the query actually requests it. Timestamp metadata alone does not establish temporal join semantics or how ties should be resolved. |

DuckDB documents [distinct and distinct-on selection](https://duckdb.org/docs/current/sql/query_syntax/select), [unnest null/empty and side-by-side behavior](https://duckdb.org/docs/current/sql/query_syntax/unnest), [scalar and quantified subqueries](https://duckdb.org/docs/lts/sql/expressions/subqueries), and its [subquery decorrelation approach](https://duckdb.org/2023/05/26/correlated-subqueries-in-sql). These establish candidate capabilities; validate emission and physical plans against our bundled version before adopting a rewrite.

### Schema properties needed

Extend mapping metadata beyond IDs/endpoints/properties with composite primary and unique keys, nullability, foreign keys, relationship multiplicity, and typed nested values. Separate enforced/validated constraints from estimates. Track timestamp timezone/precision and event-versus-validity meaning; record ordering and tie-breakers only when guaranteed. Statistics should include row counts, distinct counts, null fractions, and useful ranges.

Propagate uniqueness, functional dependencies, cardinality bounds, ordering, and required columns through SQL IR. Projection can drop a key; expansion and one-to-many joins can destroy uniqueness; filtering can preserve it. A table key does not identify duplicate input occurrences. Constraints used to change results must remain valid through mapped writes and external source changes. Estimates may guide placement but cannot justify eliminating rows or error checks. DataFusion provider declarations alone do not enforce primary keys or implement foreign-key rewrites: see [constraint handling](https://datafusion.apache.org/library-user-guide/table-constraints.html).

### Recommended implementation order

1. Add boundary reasons and physical-work counters to explain/diagnostics. Distinguish SQL scope count from executed statements and physical materialization.
2. Add schema properties and propagate the minimal proofs needed for distinct, cardinality, and required-column simplification. Keep conservative behavior when metadata is absent.
3. Implement distinct specialization, broader typed unnest regions, and existence-only rewrites as one broad batch. Prioritize reductions in routine planning and execution overhead.
4. Add native scalar/lateral alternatives and shared aggregate/window rewrites. Preserve these relational forms until backend placement. Compare emitted plans instead of assuming one syntax always wins.
5. Assess shared-subplan materialization and temporal specialization after measuring the broader changes. Keep SQL IR DAG → DuckDB regions plus DataFusion residual execution as the sole execution model.

Use focused equivalence checks for duplicate traversers, ordered representatives, graph identity versus property equality, null/missing values, empty lists/groups, zip versus Cartesian expansion, bulk, scalar errors, effects, and mapped writes. Finish the implementation batch before running the full local suite. Acceptance remains all 1,511 scenarios passing on one instance plus improved full-suite total and broad per-scenario timings; no new leaderboard rows or alternate evidence paths.

### First advanced SQL implementation batch

The initial implementation uses named semantic rules in `src/ir/rel/rules.rs`:

- Propagate catalog-generated node/edge identity keys through DataFusion's functional dependencies. Mapped sources receive no inferred key declarations.
- Eliminate ordered dedup only when a non-null unique determinant proves every key occurs at most once. Keep original input order and payload.
- For count-only consumers, project semantic distinct keys (including correlation identity) and use relational DISTINCT instead of representative-selection windows. RDF terms retain their identity-aware path.
- Remove redundant full-row DISTINCT on the membership side of semi/anti SQL IR joins. Existence-only lowering also avoids building graph dedup windows when the representative is unobservable.
- Fold scalar singleton unwind into a projection, preserving duplicate parents and retaining the existing behavior for shadowing and tagged values. Reuse DataFusion's existing unnest filter-pushdown rule.
- Remove scalar cardinality guards only with a uniqueness proof; nullable unique columns do not suffice. Native correlated scalar SQL emission remains a follow-on alternative.
- Analyze SQL eligibility once per logical node, and report lowering, capability, and SQL-preparation boundary reasons with `CRABGRAPH_EXPLAIN_DAG=1`.

The rules run before SQL region placement; the language → Graph IR → SQL IR DAG → DuckDB/DataFusion contract is unchanged. Broader user-declared mapping constraints, timestamps, shared windows, and cost-based materialization remain follow-on work. This batch must pass the focused rule/mapped-write checks and a complete local run before replacing published results.
