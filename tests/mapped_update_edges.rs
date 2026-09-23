#![cfg(feature = "duckdb")]

use arrow::datatypes::{DataType, Field, Schema};
use new_graph::ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping};
use new_graph::ir::rel::sql::DuckDbExecutor;
use new_graph::mapped_engine::MappedGraphEngine;
use std::sync::Arc;

fn engine() -> MappedGraphEngine {
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema(
        "people",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("score", DataType::Int64, true),
        ])),
    );
    mapping.register_table_schema(
        "links",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("src", DataType::Int64, false),
            Field::new("dst", DataType::Int64, false),
            Field::new("weight", DataType::Int64, true),
        ])),
    );
    mapping.map_node(NodeMapping::table("P", "people", "id").property("score", "score"));
    mapping.map_edge(
        EdgeMapping::table("R", "links", "src", "dst", "P", "P")
            .with_id("id")
            .property("weight", "weight")
            .property("source", "src"),
    );
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping));
    engine.execute_sql("CREATE TABLE people(id BIGINT PRIMARY KEY, score BIGINT); INSERT INTO people VALUES(1,10),(2,20),(3,30); CREATE TABLE links(id BIGINT PRIMARY KEY,src BIGINT,dst BIGINT,weight BIGINT); INSERT INTO links VALUES(5,1,2,2),(6,1,3,3)").unwrap();
    engine
}

#[tokio::test]
async fn native_sql_updates_edge_properties_and_rejects_endpoint_changes() {
    let mut engine = engine();
    assert_eq!(
        engine
            .cypher_update("MATCH (:P)-[r:R]->(:P) SET r.weight = r.weight * 2")
            .await
            .unwrap(),
        2
    );
    let weight: i64 = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT sum(weight) FROM links", [], |r| r.get(0))
        .unwrap();
    assert_eq!(weight, 10);
    assert!(
        engine
            .cypher_update("MATCH (:P)-[r:R]->(:P) SET r.source = 3")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn duplicate_match_rows_fail_without_arbitrary_updates() {
    let mut engine = engine();
    assert!(
        engine
            .cypher_update("MATCH (p:P)-[:R]->(:P) SET p.score = p.score + 1")
            .await
            .is_err()
    );
    let score: i64 = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT score FROM people WHERE id=1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(score, 10);
    assert_eq!(
        engine
            .cypher_update("MATCH (p:P) WHERE p.score >= 20 SET p.score = p.score + 1")
            .await
            .unwrap(),
        2
    );
}
