# Language coverage review

Snapshot: 2026-08-31

This review separates language-result accuracy from SQL pushdown and SQL-to-interpreter agreement. Those measurements answer different questions and must not share a denominator.

## Verified change

Run the Gremlin expected-output corpus with:

```sh
cargo test --release --test gremlin_tinkerpop_cases -- --nocapture
```

| Measure | Before | After | Change |
|---|---:|---:|---:|
| Accurate runnable cases | 1,579 / 1,667 (94.7%) | 1,584 / 1,667 (95.0%) | +5 net cases |
| Incorrect results | 88 | 83 | -5 net cases |
| Parse, plan, or runtime errors | 0 | 0 | none |
| Skipped for missing harness data | 42 | 42 | none |

The changes make four cast cases and two `valueMap().asString()` cases pass. The full sweep gained five net cases because one repeat result also changed:

- Scalar casts preserve a null result as a productive traverser.
- Gremlin integer casts truncate fractional values toward zero.
- Gremlin date casts retain a valid timestamp's time and offset.
- Gremlin value maps use `key=value` rendering, while typed SQL structs retain `key: value` rendering.

The planner emits Gremlin-specific calls for ambiguous integer and date conversions. Shared SQL and Cypher conversions keep their existing semantics.

The report uses the five-case net change. It does not claim the six targeted fixes as the coverage increase. Repeat execution must be deterministic before individual repeat-case movement is attributed to an unrelated semantic change.

## Current shape

Gremlin has complete parser-to-runtime reach across the runnable corpus. Its 83 remaining runnable misses are result mismatches, not unsupported syntax or failed planning. The largest clusters are repeat control flow, graph algorithms, sack and side-effect behavior, orderability, path history, and match constraints.

Cypher has broad read-language support. Its DuckDB runner now uses a real
database interrupt across fixture setup and query execution, resets the
session after interruption, and distributes small ordered chunks across
persistent workers. The complete 5,593-case denominator finished in 58.09
seconds on the 2026-08-31 development machine:

| Measure | Verified result |
|---|---:|
| Total cases | 5,593 |
| Runnable cases | 5,446 |
| Matched expected output | 3,238 (57.9% of total) |
| Parsed | 5,427 |
| Planned | 5,399 |
| Lowered to relational work | 4,610 |
| Executed in DuckDB | 4,443 |
| Result mismatches | 1,205 |
| Missing harness data | 147 |

Reproduce the merged report with:

```sh
scripts/run-rel-coverage.sh cypher 4
```

The command writes per-worker logs and artifacts plus merged `metrics.tsv`
and `blockers.tsv` files under
`target/coverage-reports/cypher-duckdb-sharded/`. The default eight-case
chunks retain nearby fixture reuse without concentrating all expensive LDBC
or LSQB work in one process. `GRAPH_REL_SHARD_CHUNK_SIZE` and the worker count
are configurable.

The current report includes 50 fixture setups that exceeded the deliberately
short one-second DuckDB operation budget. They are reported separately as
setup timeouts, not mistaken for slow generated queries. Several focused
suites also include imported cases whose scenario data or compile-time schema
information is absent:

| Focused Cypher slice | Result | Review finding |
|---|---:|---|
| `function/range` | 29 / 36 (80.6%) | All seven misses depend on the empty `CSV tck` fixture. Constant-only range cases pass. |
| `function/boolean` | 10 / 13 (76.9%) | Boolean values are correct. Three misses differ only in provider-specific ordering of numeric IDs. |
| `function/utility/coalesce` | 30 / 31 (96.8%) | The remaining expected binder error requires schema types that the imported fixture does not expose at planning time. |

These are useful diagnostic slices, but they should not replace the documented Cypher SQL agreement or pushdown benchmarks.

## Configuration boundary

Language-sensitive choices belong in `GraphPlanPolicy` or in explicit calls selected by a frontend. They should not be inferred late from an untyped value. The existing language policy already distinguishes property-missing behavior, optional results, path mode, match mode, multiplicity, and result form.

Extend that boundary when a real conflict appears. The next candidates are:

- numeric coercion and overflow;
- null ordering and cross-type ordering;
- null values versus unproductive Gremlin traversers;
- path uniqueness and repeated-edge rules;
- provider-defined IDs and result formatting;
- strict versus permissive schema binding.

Provider configuration should refine a language policy, not replace it. For example, a Cypher frontend can select Cypher semantics first, then a DuckDB or Kuzu compatibility profile can resolve the remaining provider-defined behavior.

## Recommended work order

1. Import scenario initializers and schema types for `CSV tck`. Exclude neither expected rows nor expected binder errors merely because the shared fixture is empty.
2. Represent Gremlin productivity separately from `Value::Null`. This is required to preserve injected nulls through `fold()` without retaining results from unproductive traversals.
3. Address repeated Gremlin clusters in this order: repeat state, sack and side effects, path history, match constraints, then graph-computer algorithms.
4. Build the mapped SPARQL DuckDB execution denominator. Language accuracy, SQL agreement, and full pushdown should remain separate columns in reports and on the landing page.

## Regression rule

Every semantic change must run the complete applicable corpus before its coverage number changes. A focused test can diagnose a defect, but only the unchanged full denominator can establish an improvement. Any regression in another suite blocks the claimed increase.
