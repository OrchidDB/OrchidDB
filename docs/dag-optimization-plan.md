# SQL IR DAG optimization plan

## Contract

Keep one Crabgraph execution path: language frontend → Graph IR → SQL IR DAG → DuckDB regions and DataFusion residual operators. Preserve exact values, ordering, bulk, mutation visibility, transaction rollback, callback invocation counts, and error behavior. Run tests locally; GitHub Actions publishes only static documentation and recorded evidence.

## Measurement and acceptance

1. Capture a reproducible baseline from the current commit. Repeated workloads use the same fixtures, query sequence, build profile, and warmup. Record total time and per-query medians, and verify results on every repetition.
2. Instrument lowering, SQL-region preparation, physical planning, and execution separately. Include region/kernel counts. Do not infer engine performance from adapter timing alone.
3. Require all 1,511 pinned upstream Gremlin scenarios to pass on one uninterrupted Crabgraph instance. Run mapped-write regression tests for table destinations, identity, and transaction semantics.
4. Retain an optimization only if the repeated benchmark improves without incorrect results. Record build profile and before/after revisions; do not compare different build profiles as an optimization gain.

## Implementation sequence

1. Measure first. Identify fixed overhead versus row processing and SQL preparation costs.
2. Remove measured overhead with narrow changes. Candidates include repeated session construction, repeated source collection, and redundant row serialization between compatible residual kernels.
3. Add conservative DAG rewrites with explicit eligibility. Preserve SQL scope barriers, ordering, side effects, and duplicate evaluation. Run DataFusion logical optimization before region placement; opaque kernels retain their pushdown barriers. Fuse compatible unary residual kernels without reordering evaluation.
4. Establish typed operator contracts before broader predicate movement or cost-based placement. Stateful/JVM/write operators remain boundaries unless their semantics prove a rewrite safe. Full typed-column migration, plan caching, and set-based write batching are separate follow-on changes if measurements justify them.
5. Re-run matched before/after benchmarks and the complete Gremlin suite; commit and push code and evidence. Publish through the existing static workflow.

## Existing leaderboard

For Crabgraph only, add **Passed runtime**, the sum of `elapsed_ms` for records whose status is exactly `pass`. Exclude failed, skipped, unsupported, timeout, and adapter-error cases. Label this as recorded scenario time, including setup/assertion overhead, not pure database execution or suite wall-clock time. Show missing timing explicitly; never treat missing timing as zero. Keep ranking by passed scenarios and retain one Crabgraph result. Other engines do not receive a runtime value.
