Update mapped table properties with Cypher while preserving your database schema and transaction boundaries.

## Update a node property

Use `MappedGraphEngine::cypher_update` for an assignment over matched rows:

```rust
let changed = graph.cypher_update(
    "MATCH (p:Person) WHERE p.age >= 30 SET p.age = p.age + 1"
).await?;
println!("updated {changed} rows");
```

The engine compiles the match and assigned value into a native DuckDB update. It returns the affected-row count. DuckDB enforces the destination column's type and constraints.

## Bind an update value

```rust
use std::collections::BTreeMap;
use new_graph::ir::Value;

let params = BTreeMap::from([
    ("name".into(), Value::String("alice".into())),
    ("age".into(), Value::Int(31)),
]);
let changed = graph.cypher_update_with_params(
    "MATCH (p:Person) WHERE p.name = $name SET p.age = $age",
    &params,
).await?;
```

The parameter values enter the parsed query as typed data. See [parameters and values](/parameters.html) for the same pattern in read queries.

## Prepare the mapping

Use a table-backed node mapping with a known label and a unique, non-null ID column. Map the property to a writable physical column. Each update statement assigns one property; group several statements in an [executor transaction](/transactions.html#mapped-engine-transactions) when they belong together.

For relationship updates, give the table-backed edge mapping an explicit ID using `.with_id(...)`. Its source and destination columns continue to identify the endpoints.

## Update a relationship property

The tutorial's `ORDERED` mapping exposes `total` and identifies each relationship with `order_id`:

```cypher
MATCH (:Person)-[r:ORDERED]->(:Order)
WHERE r.total > 100.0
SET r.total = r.total + 10.0
```

Execute the statement with `cypher_update`, then run a separate `cypher` query to read the new values.

## Combine graph and relational work

Use `execute_sql` when an operation is naturally expressed as SQL. It executes against the mapped engine's DuckDB connection:

```rust
graph.execute_sql("UPDATE users SET age = 30 WHERE id = 1")?;
let result = graph.cypher(
    "MATCH (p:Person) WHERE p.name = 'alice' RETURN p.age"
).await?;
```

Keep identity and endpoint management in your relational data model. The graph mapping continues to resolve the same columns after property updates.
