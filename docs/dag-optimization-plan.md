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
