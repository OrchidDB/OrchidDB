# Slow conformance queries

Measured September 23, 2026, using the release relational conformance runner
with DuckDB, the `all` profile, all 5,593 Cypher and 1,709 Gremlin cases, serially.
The baseline completed in 96.38 seconds. There were **46 cases at or above
500 ms**, all Cypher; none of the Gremlin cases reached that threshold.

The complete ranked list, including query text, case path, outcome and phase
timings, is [slow-conformance-queries-before.tsv](slow-conformance-queries-before.tsv).
Total case time includes fixture construction/cloning, planning, lowering,
database setup, execution and result comparison. Blank phase cells mean no
timing was logged (the trace threshold was 100 ms), not zero. Failed and
timed-out queries remain in the list. The harness passing does not mean all
queries matched: baseline matches were 3,499 Cypher and 767 Gremlin.

| Case / query family | Total ms | Dataset ms | Query pipeline ms | Baseline outcome |
| --- | ---: | ---: | ---: | --- |
| SNAP Amazon count | 5,093 | 4,801 | <100 | matched |
| DDL sequence: `SUM(c.propx)` | 3,451 | 3,292 | <100 | lower error |
| LDBC list grouping | 2,951 | 2,542 | 259 | mismatch |
| LSQB parquet q7: optional likes/replies | 2,626 | 909 | 1,587 | timeout |
| SNAP Twitter count | 2,437 | 2,285 | <100 | matched |
| LSQB parquet q1: long join chain | 2,255 | 422 | 1,631 | matched |
| LSQB CSV q7: optional likes/replies | 1,962 | 408 | 1,437 | timeout |
| LSQB CSV q1: long join chain | 1,940 | 398 | 1,387 | matched |
| LSQB parquet q4: tag/creator/likes/replies | 1,920 | 585 | 1,178 | matched |
| LSQB parquet q2: knows/comment/reply | 1,839 | 930 | 776 | matched |

## First fix

`PropertyGraph::clone` previously copied the outgoing adjacency, incoming
adjacency and edge-location maps, including per-edge strings and vectors.
The fixture cache cloned the graph for every case, so even warm datasets
spent hundreds of milliseconds copying indexes. The same cost applied to
graph/session snapshots using `clone`.

These three immutable base indexes now use `Arc` storage. `add_edges` obtains
copy-on-write access once per table, preserving independent graph mutations.
Snapshot decoding builds fresh indexes. Overlay state still clones independently.
A regression test verifies that clones initially share indexes and that adding
edges to a clone leaves the original adjacency and edge IDs unchanged.

Validation: `cargo test --release --lib ir::catalog` passed all 22 catalog and
snapshot tests, including the new clone-isolation test.

The post-change full run also completed all 7,302 cases, with **zero transitions
from matched to another outcome**. The [post-change slow list](slow-conformance-queries-after.tsv)
contains 40 cases at or above 500 ms. All 22 LDBC and 18 LSQB dataset phases
fell below the 100 ms trace threshold; the same phases previously accounted
for 6,233 ms and 8,930 ms respectively.

The full after-run took 168.95 seconds versus 96.38 seconds before: heavy
concurrent compiler/linker activity makes the total unsuitable as a speedup
measurement. Other developers changed the working tree between builds; the
additional 24 Cypher and 18 Gremlin matches cannot be attributed to this fix.
Post-change logs: `/tmp/graph-conformance-after.log`; outcomes:
`target/coverage/slow-after`.

A subsequent serial LSQB comparison (18 cases, same binaries, budgets and
`GRAPH_REL_SLOW_MS=0`) completed in **27.08 s before / 17.22 s after**, about
36% less wall time, with identical case outcomes. Warm fixture cloning across
the remaining 17 cases had a median of **394 ms before / <1 ms after** (maximum
446 ms / <1 ms). Total dataset time, including the first cold load, fell from
9,482 ms to 2,059 ms. This is a single-run comparison, not a statistical benchmark.
Reproduce by adding `GRAPH_REL_SUITE=lsqb` and `GRAPH_REL_SLOW_MS=0` to the
command below. Logs: `/tmp/graph-lsqb-before.log` and `/tmp/graph-lsqb-after.log`;
outcomes: `target/coverage/slow-lsqb-before` and `target/coverage/slow-lsqb-after`.

## Remaining priorities

1. LSQB q3, q6, q7 and q9 hit the one-second query deadline. Removing fixture
   cloning overhead does not solve their join cardinality/SQL execution cost.
2. Cold SNAP and LDBC fixture loading remains expensive. Sharing indexes helps
   subsequent copies, but CSV/parquet decoding and initial index construction
   still occur once per dataset.
3. Large embedding-array lowering and existing result mismatches need separate
   investigation; elapsed time alone does not establish a query-engine bug.

## Reproduce

```sh
cargo test --release --test graph_rel_backend_cases --no-run
GRAPH_REL_LANG=all GRAPH_REL_EXEC=duckdb \
GRAPH_REL_OUTPUT_DIR=target/coverage/slow-run \
GRAPH_REL_TIMEOUT_MS=1000 GRAPH_REL_SETUP_TIMEOUT_MS=1000 \
GRAPH_REL_SLOW_MS=100 \
cargo test --release --test graph_rel_backend_cases -- --ignored --nocapture \
  > target/slow-conformance.log 2>&1
python3 scripts/slow-conformance-queries.py target/slow-conformance.log \
  > target/slow-conformance.tsv
```

The initial baseline used the existing release binary copied to
`/tmp/graph-conformance-before`; logs are at `/tmp/graph-conformance-before.log`,
and case outcomes at `target/coverage/slow-before`. Other development and
compilation were running concurrently, so these wall-clock numbers are
diagnostic measurements, not a controlled performance benchmark.
