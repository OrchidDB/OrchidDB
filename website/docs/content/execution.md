# Execution and plans

Inspect Graph IR and generated SQL, choose a read policy, and understand execution statistics.

## Managed read policies

`GraphEngine` defaults to `ReadMode::Hybrid`. This combines DuckDB SQL islands with graph runtime operators according to the query plan. To require a complete SQL read, set `ReadMode::SqlOnly`:

```rust
use new_graph::engine::ReadMode;

graph.set_read_mode(ReadMode::SqlOnly);
let result = graph.cypher(
    "MATCH (p:Person) RETURN p.name"
).await?;
```

SQL-only mode enforces the chosen execution contract. Mapped property-graph and RDF reads execute against their DuckDB source tables through SQL lowering.

## Inspect Graph IR

The CLI's `--explain` flag parses and plans a query, then prints Graph IR:

```sh
crabgraph --explain --query \
  "MATCH (p:Person)-[:KNOWS]->(friend) RETURN p.name, friend.name"
```

Use this to inspect scans, expansions, filters, and projections. Explain mode does not execute the query or open a database.

From Rust, build a plan through the language frontend and format it:

```rust
use new_graph::language::cypher;

let parsed = cypher::parse_query(
    "MATCH (p:Person) RETURN p.name"
).map_err(|error| error.to_string())?;
let plan = cypher::CypherPlanner::new().plan(&parsed)
    .map_err(|error| error.to_string())?;
println!("{}", new_graph::ir::explain(&plan));
```

## Inspect generated SQL

For a mapped engine, `explain_cypher` returns the DuckDB read SQL:

```rust
let sql = graph.explain_cypher(
    "MATCH (p:Person)-[:ORDERED]->(o:Order) \
     WHERE o.total > 100.0 RETURN p.name, o.total"
).await?;
println!("{sql}");
```

Graph property names resolve to the physical columns in your mapping. Use this output when reviewing source joins, filters, and query-backed definitions.

## Execution metadata

Managed query results identify their backend:

| Backend | Execution |
| --- | --- |
| `ExecutionBackend::DuckDb` | The query's execution used DuckDB. |
| `ExecutionBackend::Hybrid` | SQL islands and graph runtime work were combined. |
| `ExecutionBackend::GraphRuntime` | Execution used the graph runtime. |

`ExecStats` records `islands`, `island_rows`, `residual_ops`, `interpreted_ops`, and `declined`. Log these alongside an application query name when analyzing execution. SQL island rows are intermediate execution rows; use the result batch for the number of returned rows.

## SQL timeout

```rust
use std::time::Duration;
graph.set_sql_timeout(Duration::from_secs(10));
```

This setting applies to individual DuckDB queries and setup work. Treat the SQL timeout as one part of your application's overall request budget.

## Planner and executor boundaries

Language planners produce `GraphPlan` values. `RelBackend` handles relational lowering; SQL preparation turns the lowered plan into a dialect-specific program. `SqlExecutor` provides the execution boundary, and `IslandTarget` determines where a SQL island runs.

Use the engine APIs for application workflows. Use these lower-level types when integrating a custom planner or execution target. See the [Rust API](rust-api.md#planning-and-execution-apis) for the module map.
