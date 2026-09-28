# Execution and plans

Inspect Graph IR and generated SQL, choose a read policy, and understand execution statistics.

## Managed read policies

`GraphEngine` defaults to `ReadMode::Hybrid`. It lowers Graph IR to a DataFusion relational DAG, executes eligible SQL regions in DuckDB, and schedules native kernels through DataFusion. To require a complete SQL read, set `ReadMode::SqlOnly`:

```rust
use orchiddb::engine::ReadMode;

graph.set_read_mode(ReadMode::SqlOnly);
let result = graph.cypher(
    "MATCH (p:Person) RETURN p.name"
).await?;
```

`SqlOnly` applies to managed-storage reads. Mapped execution currently uses the shared DAG path regardless of this setting; it lowers source reads to SQL islands and schedules remaining operators through DataFusion. RDF dataset queries use the same mapped execution path.

## Inspect Graph IR

The legacy core CLI's `--explain` flag parses and plans a query, then prints Graph IR:

```sh
cargo run --features duckdb --bin orchiddb -- --explain --query \
  "MATCH (p:Person)-[:KNOWS]->(friend) RETURN p.name, friend.name"
```

Use this to inspect scans, expansions, filters, and projections. Explain mode does not execute the query or open a database.

From Rust, build a plan through the language frontend and format it:

```rust
use orchiddb::language::cypher;

let parsed = cypher::parse_query(
    "MATCH (p:Person) RETURN p.name"
).map_err(|error| error.to_string())?;
let plan = cypher::CypherPlanner::new().plan(&parsed)
    .map_err(|error| error.to_string())?;
println!("{}", orchiddb::ir::explain(&plan));
```

## Inspect generated SQL

For SQL inspection without execution, use `compiler::compile` or `compile_json` and inspect `CompiledSql.sql` and `logical_plan`. The existing `MappedGraphEngine` compatibility facade also offers `explain_cypher` (the following snippet uses that facade):

```rust
let sql = graph.explain_cypher(
    "MATCH (p:Person)-[:ORDERED]->(o:Order) \
     WHERE o.total > 100.0 RETURN p.name, o.total"
).await?;
println!("{sql}");
```

Graph property names resolve to the physical columns in your mapping. Use this output when reviewing source joins, filters, and query-backed definitions.

## Execution metadata

`GraphEngine` query results identify their backend for managed or mapped storage:

| Backend | Execution |
| --- | --- |
| `ExecutionBackend::DuckDb` | The query's execution used DuckDB. |
| `ExecutionBackend::Hybrid` | DuckDB SQL regions and DataFusion operators were combined. |
| `ExecutionBackend::DataFusion` | The relational DAG executed through DataFusion. |

`ExecStats` records `datafusion_ops`, `islands`, `island_rows`, `sql_queries`,
`native_source_queries`, `native_source_rows`, and `cost`. It also exposes
`logical_plan`, `physical_plan`, `constraint_proofs`, and `layout_selections`.
Use `fully_pushed_down()` to inspect whether all recorded query operators were
delegated to SQL. Backend labels alone do not establish that.

Logical plans show selected physical tables and collection `Unnest` operators.
Layout estimates describe parent-table bytes/files; they are not measured I/O or
expanded collection cardinalities. SQL island rows are intermediate rows; use
the result batch for the number of returned rows.

For RDF, `sparql_dataset` returns these statistics in `QueryResult.stats`.
`sparql_query` decodes typed terms and does not return the stats wrapper.
`sparql_sql(query, dataset)` produces SQL without executing the query.

## SQL timeout

```rust
use std::time::Duration;
graph.set_sql_timeout(Duration::from_secs(10));
```

This setting applies to individual DuckDB queries and setup work. Treat the SQL timeout as one part of your application's overall request budget.

## Planner and executor boundaries

Language planners produce `GraphPlan` values. `RelBackend` handles relational lowering; SQL preparation turns the lowered plan into a dialect-specific program. `SqlExecutor` supplies the SQL execution boundary. Managed execution compiles to a DataFusion relational DAG with explicit DuckDB regions and native kernels.

Graph IR is retained for planning and inspection. It has no standalone recursive interpreter, and execution errors do not trigger an interpreter fallback. SPARQL `SERVICE`, including `SERVICE SILENT`, is unsupported; no remote SPARQL HTTP calls are made.

Use the engine APIs for application workflows. Use these lower-level types when integrating a custom planner or execution target. See the [Rust API](rust-api.md#planning-and-execution-apis) for the module map.
