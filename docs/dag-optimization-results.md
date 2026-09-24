# SQL IR DAG optimization results

## Changes

- Run DataFusion logical optimization before partitioning the SQL IR DAG into DuckDB regions.
- Fuse adjacent read kernels (Bind, Filter, Project, CurrentProject, Return, Expand, and PathFilter) and their scan/value/correlated sources, retaining evaluation order, bulk, cancellation checks, and work-budget charges. Apply this rewrite to correlated subplans as well as the outer DAG. SQL, JVM, write, branch, and aggregate operators remain distinct boundaries.
- Reuse DataFusion session and DuckDB execution resources within one GraphEngine. Every query still compiles its current graph snapshot. DuckDB's existing content-addressed scan cache distinguishes changed data. Separate engines own separate sessions; execution errors discard the session.
- Reuse immutable Arrow transport schemas and per-batch serialization buffers.
- Add opt-in `CRABGRAPH_PROFILE_DAG=1` diagnostics for preparation, physical planning, and execution time.
- Show summed passed-scenario runtime for Crabgraph only on the existing leaderboard. Other statuses do not contribute.

## Earlier diagnostic benchmark

These measurements cover the first optimization batch, before correlated-subplan fusion and transport allocation improvements. The full-suite result is the acceptance metric.

The [raw measurements](performance/dag-before-after.json) record three alternating baseline/optimized rounds, six query shapes over the same 256-node/256-edge fixture, three warmups per query per round, and 20 measured repetitions. All 360 measured results per binary, plus warmups, were compared for exact equality. Fixture setup is excluded. Timings include the local request/response, query parsing, planning, execution, and result encoding.

Both binaries use the same dev profile with debug symbols disabled. These are workstation measurements, not release throughput claims. Baseline is the pre-optimization binary used for the 1,511-pass run on source 7729f95. Binary SHA-256 hashes and every timing sample are recorded in the JSON.

| Query shape | Baseline median ms | Optimized median ms |
| --- | ---: | ---: |
| Vertex count | 33.94 | 10.10 |
| Filtered count | 38.88 | 13.55 |
| Edge expansion count | 41.18 | 15.10 |
| Property sum | 39.08 | 15.54 |
| Filtered ordered properties | 44.83 | 20.95 |
| Correlated local expansion/count | 68.56 | 45.60 |

Total measured query time: **16,734.895 ms → 7,943.940 ms**, a **2.11× speedup / 52.5% reduction**. Resource reuse alone was the largest improvement. The complete logical optimizer adds planning overhead on these small queries compared with reuse alone; its benefit is not separately established by this benchmark.

Reproduce with `scripts/benchmark-dag.py --binary baseline=PATH --binary optimized=PATH --output result.json` using binaries built with identical profiles.

## Verification

- 38 mapped-write, mapped-read, and relational-execution tests pass.
- Three leaderboard aggregation tests pass.
- Full upstream Gremlin run: pending final verification on the committed optimized source.
