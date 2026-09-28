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
