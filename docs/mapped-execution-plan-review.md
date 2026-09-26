# Reading the mapped execution plans

The [captured plans](mapped-execution-plans.md) come from actual executions against DuckDB, not standalone SQL compilation. Reproduce them with:

```sh
RUST_MIN_STACK=16777216 cargo run --features duckdb --example mapped_execution_plans > docs/mapped-execution-plans.md
```

The fixture contains 10,000 people, 9,999 directed links in a chain, and a mapped unrelated view that raises an error if scanned. Source keys are strings. All nine queries pass asserted results and require at least one SQL island, fewer than 20 native source rows and requests, and no source SQL referencing the unrelated view or unmapped column; the update is additionally checked directly through another connection to the same DuckDB database.

`DuckDbExec` is the SQL island. Read the physical DAG from the leaves upward: operators above it run through the DataFusion scheduler. `DecodeTraversers` translates SQL result bindings into runtime graph values; it alone is not meaningful evidence of residual query processing. `Pruned` indicates a runtime kernel with binding pruning, not a DuckDB operator. `CooperativeExec` is a scheduler wrapper.

| Query | SQL work | Work outside SQL | Native source rows |
| --- | --- | --- | ---: |
| Selective lookup | Filter and projection | Binding decoding | 0 |
| One-hop join | Filter, edge/node joins, projection | Binding decoding | 0 |
| Aggregate | Filter and count | Binding decoding | 0 |
| Bounded variable-length traversal | Filtered seed, recursive CTE, endpoint join, ordering | Binding decoding | 0 |
| Named path and path length | Select the starting node | Path expansion, length projection, sorting | 7 |
| List comprehension after joined aggregation | Filtered seed and recursive traversal | Aggregate, collection, list expression and projections | 4 |
| Correlated optional match and list comprehension | Select two starting nodes | Lateral optional expansion, aggregation, list expression, sorting | 4 |
| SQL inside a lateral path subplan | UNWIND plus selective seed SQL on each of two invocations | Named-path expansion, lateral combination, projection, sorting | 5 |
| Mapped update | Select affected node | Property mutation and projection; persistence writes source table | 1 |

The substantive mixed plans are worth inspecting closely:

- **Named path and path length:** the SQL island selects only `p42`. Native expansion fetches three adjacent links and their reached nodes, then projects path lengths and sorts. Seven native requests return seven rows in total. The path remains a typed graph value outside SQL.
- **SQL inside a lateral path subplan:** the outer SQL island produces the two UNWIND values. Each lateral invocation executes the selective seed SQL, then native path expansion. The capture records three SQL island executions, including the repeated seed SQL, and five source rows fetched through four native requests. This demonstrates SQL inside an executed subplan; the repeated independent seed is still avoidable work.
- **List comprehension after joined aggregation:** the recursive SQL island returns the three reachable endpoint bindings. DataFusion schedules the native aggregate and fused projections that build and transform the list. A single batched source lookup fetches four people for graph-value hydration. This is bounded by reached bindings, but repeats properties already present in island output.
- **Correlated optional match and list comprehension:** SQL selects `p42` and `p9999`. `LateralApplyExec` performs the optional expansion, including preserving the unmatched `p9999`. Native aggregation collects names; projections filter and uppercase the list; native sorting orders the result. Four source statements fetch four total rows. The two adjacency lookups are separate correlated requests, so this case does not demonstrate batching across all optional-match inputs.

The simple cases execute their query computation within SQL. The generated one-hop joins have identity equality predicates. The lateral example has an intentional cross join between a single-row seed and the two UNWIND values; it is not a cross product of graph tables. The recursive seed contains the selective predicate. No successful query references the unrelated view or unmapped `unused` column in either SQL islands or native source requests.

## Issues exposed by inspection

The initial inspection found a serious named-path defect: the path ran inside a native-only lateral subplan and fetched every mapped node. With the diagnostic unrelated view made readable, this issued 10,005 source requests and fetched 20,004 rows. The strict throwing view caught it. The corrected plan removes the unnecessary uncorrelated lateral wrapper and permits eligible SQL inside nested subplans. The same strict query now passes with one selective SQL island and seven source requests fetching seven rows. The current capture contains only the corrected execution, and the unrelated throwing view remains in the fixture.

Remaining observed inefficiencies should not be mistaken for ideal plans:

- `count(p)` currently renders the graph node as a string inside SQL and references `name`, although counting a non-null node identity is sufficient.
- SQL output and native source hydration overlap in the complex list example.
- Correlated optional expansion issues separate adjacency requests for the two starting nodes.
- The lateral UNWIND example reruns identical seed SQL for each outer row, although the seed is independent of that row.
- The named path fetches reached node properties through separate requests rather than one combined final hydration request.

The capture is not a full query profiler. `sql_queries` contains emitted island SQL before DuckDB optimization; it does not show DuckDB's chosen scan/index/join operators. `native_source_rows` counts fetched records, not DuckDB rows inspected internally. Native source SQL uses placeholders for Arrow-backed key batches, and this instrumentation does not print their bound contents. Per-island boundary row counts are not currently recorded. Executed nested plans are appended to the top-level physical plan; the current formatting can include an empty `Executed subplan:` heading before the nested tree. SQL island counts include repeated executions of the same subplan. Persistence DML, schema discovery, and transaction statements are not included in the query SQL arrays. The update's direct source verification establishes persistence, but is not a DML trace. These are instrumentation limits, not evidence of zero additional work.
