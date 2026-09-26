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
traversals alongside its existing transport evidence. Reports include
`query_cost_summary`: highest work, elapsed-time rankings, threshold candidates,
and optional comparisons with an earlier run. Cost observations never alter case
status, fixtures, deadlines or assertions.

```
python conformance/upstream/run.py --engine orchiddb --suite opencypher \
  --output target/current-cypher.json \
  --cost-work-threshold 100000 \
  --cost-baseline target/previous-cypher.json
```

Comparisons require matching case definitions, query text/phase sequence, metric
version and complete boundary-work coverage in both reports. Reports predating
this metric are not treated as zero-cost baselines. The first complete run creates
a baseline for subsequent constraint/lowering changes.

The published `conformance-report.html#query-cost` displays coverage and the 50
highest-work scenarios per suite, with links to individual cases. Per-scenario
cells show work and coverage; expanded evidence contains query text and counters.
CSV downloads include metric version, coverage, work units and request time.
`query-cost-summary.json` exposes rankings and baseline comparisons. The report
uses committed `conformance/upstream-results/orchiddb-*.json` evidence.
