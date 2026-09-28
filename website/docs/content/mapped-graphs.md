# Map existing tables

Expose existing DuckDB tables as a graph, then query them with Cypher and Gremlin.

## The relational dataset

This tutorial models three people, their orders, and a social relationship. The `users` table supplies `Person` nodes. The `orders` table supplies both `Order` nodes and `ORDERED` relationships. The `follows` table supplies `FOLLOWS` relationships.

```sql
CREATE TABLE users (id BIGINT, name VARCHAR, age BIGINT);
INSERT INTO users VALUES
  (1, 'alice', 30), (2, 'bob', 28), (3, 'carol', 41);
CREATE TABLE orders (order_id BIGINT, user_id BIGINT, total DOUBLE);
INSERT INTO orders VALUES
  (100, 1, 50.0), (101, 1, 120.0), (102, 2, 80.0), (103, 3, 500.0);
CREATE TABLE follows (src BIGINT, dst BIGINT);
INSERT INTO follows VALUES (1, 2), (1, 3), (2, 3);
```

Save this as `schema.sql` beside your application's `Cargo.toml`, or download the [sample schema](/downloads/schema.sql).

## Register schemas and mappings

Use the dependencies from [installation](installation.md#use-the-rust-library). This complete `src/main.rs` sets up the tutorial database and queries it:

```rust
use std::sync::Arc;
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::ir::rel::mapping::{EdgeMapping, ForeignKeyEndpoint, GraphMapping, NodeMapping};
use orchiddb::engine::GraphEngine;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema("users", Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, true),
        Field::new("age", DataType::Int64, true),
    ])));
    mapping.register_table_schema("orders", Arc::new(Schema::new(vec![
        Field::new("order_id", DataType::Int64, false),
        Field::new("user_id", DataType::Int64, false),
        Field::new("total", DataType::Float64, true),
    ])));
    mapping.register_table_schema("follows", Arc::new(Schema::new(vec![
        Field::new("src", DataType::Int64, false),
        Field::new("dst", DataType::Int64, false),
    ])));
    mapping.map_node(NodeMapping::table("Person", "users", "id")
        .property("id", "id").property("name", "name").property("age", "age"));
    mapping.map_node(NodeMapping::table("Order", "orders", "order_id")
        .property("total", "total"));
    mapping.map_edge(EdgeMapping::table(
        "ORDERED", "orders", "user_id", "order_id", "Person", "Order"
    ).foreign_key(ForeignKeyEndpoint::Destination));
    mapping.map_edge(EdgeMapping::table(
        "FOLLOWS", "follows", "src", "dst", "Person", "Person"
    ));

    let connection = duckdb::Connection::open_in_memory().map_err(|e| e.to_string())?;
    connection.execute_batch(include_str!("../schema.sql")).map_err(|e| e.to_string())?;
    let mut graph = GraphEngine::mapped(connection, Arc::new(mapping))?;
    let result = graph.cypher(
        "MATCH (p:Person)-[:ORDERED]->(o:Order) WHERE o.total > 100.0 \
         RETURN p.name, o.total ORDER BY o.total"
    ).await?;
    for row in 0..result.returned.batch.num_rows() {
        let values = result.returned.batch.columns().iter().map(|column|
            arrow::util::display::array_value_to_string(column, row)
                .map_err(|error| error.to_string())
        ).collect::<Result<Vec<_>, _>>()?;
        println!("{}", values.join(" | "));
    }
    Ok(())
}
```

Expected rows:

```text
alice | 120.0
carol | 500.0
```

Download the [complete Rust example](/downloads/mapped_graph.rs) to use with the sample schema.

## Open an existing database

For a database that already contains these tables, open its connection and pass it to the engine. Register the same schemas and mappings, then query immediately:

```rust
let connection = duckdb::Connection::open("warehouse.duckdb")
    .map_err(|error| error.to_string())?;
let mut graph = GraphEngine::mapped(connection, Arc::new(mapping))?;
```

The mapping describes the source schema; the engine owns the connection to its rows. Use matching table names and Arrow types on both sides.

## Traverse with Gremlin

With the same mapped engine:

```rust
let friends = graph.gremlin(
    "g.V().hasLabel('Person').has('name','alice').out('FOLLOWS').values('name')"
).await?;
```

Alice follows bob and carol.

## Write through the mapping

Use `cypher` and `gremlin` to insert, update, and delete rows in the same mapped tables. Node labels select node tables; relationship types select relationship tables and endpoint columns. See [mapped writes](updates.md) for examples, identity rules, and transaction behavior.

## Share the engine with RDF

Add `RdfMapping` rules to the same `GraphMapping` before constructing the engine.
Use `sparql_query` for typed RDF results or `sparql_dataset` for Arrow results and
statistics. All languages share the connection and transaction methods. See
[RDF over application tables](rdf.md). `MappedGraphEngine` remains a compatibility
facade; new applications use `GraphEngine::mapped`.

## Optional generated statistics

[Generate statistics once](statistics.md) to collect bounded source summaries and
automatically use the cached snapshot for source selection, cardinality estimates
and supported filter ordering in Cypher, Gremlin and SPARQL. The guide covers
client/engine ownership, plan diagnostics and partial coverage.
