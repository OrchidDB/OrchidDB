# Query cost observations

`QueryResult.stats.cost` contains `QueryCost`: actual SQL island invocations,
SQL output rows and Arrow array memory bytes, native graph-kernel calls and row
visits, demand-driven source requests/rows, final Arrow result rows and elapsed time.
Cached physical subplans count every execution; counters reset per query.
Decode-only kernels are excluded from native row visits. SQL-only mode and RDF
read queries are measured too (`RdfGraphEngine::last_query_stats`).

The version 1 score is:

```
SQL output rows + native input rows + native output rows + native source rows
+ ceil(SQL output Arrow memory bytes / 1024)
+ 100 * (SQL executions + native source requests)
```

`work_per_result_row` divides by `max(1, result_rows)`. Counters and the score use
saturating arithmetic. Elapsed time is recorded separately and does not change
the work score. `report()` serializes measurements, score, amplification and
metric version.

This measures boundary work and native graph-kernel visits, not DuckDB's internal
scan cardinality, all DataFusion physical operators, network traffic, or CPU
instructions. Arrow memory bytes can include allocation overhead. An efficient
aggregate may legitimately have high amplification. Rank candidates using both
absolute work and elapsed time, then inspect their execution plans.
Native unary kernel fusion removes intermediate row encoding and decoding. The
counters measure the resulting physical kernels, not each logical operation
inside a fused kernel; a lower score does not by itself prove lower CPU time.

## Lowering regression checks

Focused regression tests in `tests/cypher_sql_cost.rs`,
`tests/gremlin_sql_joins.rs`, and `tests/recursive_sql_islands.rs` check answers
alongside SQL boundaries and native execution. They cover streaming integer
ranges, nullable element counts, quantifier variable scope, repeated match-label
equality, duplicate group members, path/history observers, and weighted repeats.

The lowering changes keep selective labelled joins and correlated match counts
in SQL, defer match-map construction until filters finish, and compact duplicate
group inputs before native map assembly. Bounded, count-only repeats aggregate
weighted frontiers in materialized SQL CTEs instead of enumerating every path.
Path-sensitive, sack-sensitive, and loop-sensitive traversals retain their
stateful execution. Heterogeneous Cypher collections still use native values;
their evaluators reuse lexical rows and fuse compatible kernels without changing
volatile-expression evaluation order.

Direct query checks against the original grateful fixture measured these work
scores. These are selected-query observations, not a new conformance run:

| Query | Earlier work | After lowering |
| --- | ---: | ---: |
| Garcia/Willie Dixon labelled join (Graph:60) | 590,245 | 108 |
| Eight-hop repeat count (Count:91) | 63,569 | 102 |
| Repeat with labelled suffix (Count:102) | 41,420 | 102 |
| Correlated match counts (Match:548) | 32,668 | 102 |
| Scalar match selection/count (Match:535) | 11,813 | 102 |
| Match with anchored filters (Match:560) | 31,670 | 129 |
| Nested grouping (Group:165) | 291,570 | 51,947 |

The repeat checks also assert the exact large integer answers. Grouping still
uses native map reducers after SQL compaction. Match:560 returns its exact six
maps and transfers only six SQL rows, but its observed elapsed time was about
1.62 seconds versus 0.92 seconds in the earlier evidence. The boundary score
improvement is not a latency improvement; SQL planning/execution remains a
follow-up for that query. These development-build timings are observations,
not controlled benchmarks. Historical published reports keep
their recorded outcomes and timings; helper-cost attribution was corrected from
existing evidence without executing the suites again.

## Conformance reports

Every OrchidDB case receives `query_cost`. Individual invocations retain the query,
phase, measurements and request duration. Successful DAG queries report boundary
work. Syntax-only requests, errors, and RDF update requests without complete work
instrumentation report `elapsed_only` with a reason and a null work score; skipped
cases with no requests report `not_executed`. Missing work is never reported as zero.
Request durations include parsing/planning/transport-side engine setup, and RDF
request durations also include fixture loading. DAG elapsed time excludes that setup.

Cypher fixture construction and side-effect observation requests remain visible
but are excluded from the tested-query ranking. Gremlin records submitted native
traversals alongside its existing transport evidence. Attribution version 2
classifies Gremlin graph initializers and parameter lookups as fixture work, and
assertion-helper requests as observations. Only tested traversals contribute to
the ranking; `helper_query_count` reports the excluded request count. This is
separate from version 1 of the work-unit formula. Reports include
`query_cost_summary`: highest work, elapsed-time rankings, threshold candidates,
and optional comparisons with an earlier run. Cost observations never alter case
status, fixtures, deadlines or assertions.

Cypher's native `cypher-snapshot` requests retain exact before/after side-effect
observations without executing five graph queries per snapshot. They appear as
`observation` entries with `elapsed_only` coverage and `state_snapshot` as the
reason. Their duration contributes to case time but not tested-query work.
Scenarios without side-effect assertions omit these requests. Fixture checkpoints
and engine-owned JVM processes can be reused across different queries; result
caching is not involved.

```
python conformance/upstream/run.py --engine orchiddb --suite opencypher \
  --output target/current-cypher.json \
  --cost-work-threshold 100000 \
  --cost-baseline target/previous-cypher.json
```

Comparisons require matching case definitions, query text/phase sequence, metric
version, attribution version and complete boundary-work coverage in both reports. Reports predating
this metric are not treated as zero-cost baselines. The first complete run creates
a baseline for subsequent constraint/lowering changes.

The published `conformance-report.html#query-cost` displays coverage and the 50
highest-work scenarios per suite, with links to individual cases. Per-scenario
cells show work and coverage. Published results contain compact summaries only;
raw queries, fixture payloads and returned data stay in ignored `target/` files.
Published baseline comparisons include aggregate totals and the 50 largest
improvements and regressions; complete comparison details remain in local reports.
CSV downloads include metric version, coverage, work units and request time.
`query-cost-summary.json` exposes rankings and baseline comparisons. The report
uses committed `conformance/upstream-results/orchiddb-*.json` evidence.

The runner defaults to `target/conformance/`. Publish compact evidence explicitly:

```
python conformance/upstream/compact_report.py target/conformance/orchiddb-opencypher.json \
  conformance/upstream-results/orchiddb-opencypher.json
```

Use full local reports for `--cost-baseline` comparisons; compact published
reports omit the query sequence needed to establish baseline compatibility.
