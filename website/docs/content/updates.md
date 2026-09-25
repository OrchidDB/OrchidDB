# Update properties

Create, update, and delete graph elements in the physical tables declared by your mapping. Cypher and Gremlin use the same labels, relationship types, identity columns, and property columns for reads and writes.

## Create mapped rows

```rust
graph.cypher(
    "CREATE (p:Person {name:'dave', age:29}) RETURN p.name"
).await?;
```

With `Person` mapped to `users`, this inserts into `users`. Creating a relationship inserts into its mapped relationship table and fills its source and destination columns from the bound nodes. A statement may write several mapped tables in one transaction.

Graph property names resolve to their configured physical columns. Omitted properties use the table's SQL defaults. Supply an integer ID through a property mapped to the identity column, or let the engine allocate the next integer within the transaction. DuckDB enforces column types and constraints.

## Update and return values

```rust
let result = graph.cypher(
    "MATCH (p:Person) WHERE p.name='dave' \
     SET p.age=p.age+1, p.name='David' RETURN p.name,p.age"
).await?;
```

Use `SET p += {...}` to assign several properties or `SET p = {...}` to replace the mapped property values. Replacement clears omitted writable property columns to SQL `NULL`; identity and endpoint columns are preserved.

## Delete mapped rows

```cypher
MATCH (:Person)-[r:FOLLOWS]->(:Person)
DELETE r
```

Node deletion checks mapped incident relationships. Use `DETACH DELETE p` to remove those relationships with the node. Relationship rows are deleted before node rows, and SQL constraints remain in force.

## Write with Gremlin

```rust
graph.gremlin("g.addV('Person').property('name','dave').property('age',29)").await?;
graph.gremlin("g.V().hasLabel('Person').has('name','dave').property('age',30)").await?;
graph.gremlin("g.V().hasLabel('Person').has('name','dave').drop()").await?;
```

Mapped properties address scalar columns. Relationship creation uses the configured endpoint labels and columns. Every write target must have a table-backed mapping; relationship creation, property updates, and direct relationship deletion use its explicit ID column.

## Transactions

Each mutation statement starts and commits a transaction when the connection has no active transaction. Within an explicit executor transaction, successful statements leave that transaction open for the caller. An error during mutation execution rolls back the active transaction, including earlier changes in that transaction. See [transactions](transactions.md#mapped-engine-transactions).

Identity columns must be unique, non-null integers. Writes preserve identity and relationship endpoint columns; property changes address only declared writable columns.

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
use orchiddb::ir::Value;

let params = BTreeMap::from([
    ("name".into(), Value::String("alice".into())),
    ("age".into(), Value::Int(31)),
]);
let changed = graph.cypher_update_with_params(
    "MATCH (p:Person) WHERE p.name = $name SET p.age = $age",
    &params,
).await?;
```

The parameter values enter the parsed query as typed data. See [parameters and values](parameters.md) for the same pattern in read queries.

## Prepare the mapping

Use a table-backed node mapping with a known label and a unique, non-null ID column. Map the property to a writable physical column. The affected-row-count `cypher_update` API assigns one property per statement. Use `cypher` for multiple assignments and returned values.

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
