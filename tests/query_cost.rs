#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::{GraphEngine, ReadMode},
    ir::{
        QueryCost,
        rel::mapping::{EdgeMapping, GraphMapping, NodeMapping},
    },
};
use std::sync::Arc;
fn engine() -> GraphEngine {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id VARCHAR PRIMARY KEY,name VARCHAR); INSERT INTO people SELECT 'p'||i,'name'||i FROM range(100) t(i); CREATE TABLE links(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO links SELECT 'e'||i,'p'||i,'p'||(i+1) FROM range(99) t(i)").unwrap();
    let mut m = GraphMapping::new();
    m.map_node(
        NodeMapping::table("Person", "people", "id")
            .property("id", "id")
            .property("name", "name"),
    );
    m.map_edge(EdgeMapping::table("LINK", "links", "src", "dst", "Person", "Person").with_id("id"));
    GraphEngine::mapped(db, Arc::new(m)).unwrap()
}
#[tokio::test]
async fn costs_rank_boundary_work_and_reset_between_queries() {
    let mut e = engine();
    let query = "MATCH (p:Person {id:'p1'}) RETURN p.name";
    let first = e.cypher(query).await.unwrap().stats.cost;
    assert_eq!(first.sql_executions, 1);
    assert_eq!(first.sql_output_rows, 1);
    assert_eq!(first.result_rows, 1);
    assert_eq!(first.native_kernel_calls, 0);
    assert!(first.sql_output_bytes > 0);
    let broad = e
        .cypher("MATCH (p:Person) RETURN p.name")
        .await
        .unwrap()
        .stats
        .cost;
    assert_eq!(broad.result_rows, 100);
    assert!(broad.work_units() > first.work_units());
    let again = e.cypher(query).await.unwrap().stats.cost;
    assert_eq!(first.work_units(), again.work_units());
    assert_eq!(again.sql_executions, 1);
}
#[tokio::test]
async fn repeated_lateral_execution_counts_actual_calls_and_native_work() {
    let mut e = engine();
    let result=e.cypher("UNWIND ['p1','p2'] AS wanted MATCH path=(p:Person {id:wanted})-[:LINK]->(q:Person) RETURN path").await.unwrap();
    let c = &result.stats.cost;
    assert_eq!(c.result_rows, 2);
    assert!(c.sql_executions >= 3, "{c:?}");
    assert!(c.native_kernel_calls > 0);
    assert!(c.native_input_rows > 0);
    assert!(c.native_output_rows > 0);
    assert_eq!(
        c.native_source_queries,
        result.stats.native_source_queries.len() as u64
    );
    assert_eq!(c.native_source_rows, result.stats.native_source_rows as u64);
}
#[tokio::test]
async fn strict_sql_mode_is_measured_too() {
    let mut e = GraphEngine::in_memory().unwrap();
    e.set_read_mode(ReadMode::SqlOnly);
    let r = e.cypher("RETURN 1 AS n").await.unwrap();
    assert_eq!(r.stats.cost.sql_executions, 1);
    assert_eq!(r.stats.cost.result_rows, 1);
}
#[test]
fn scoring_is_versioned_saturating_and_independent_of_elapsed_time() {
    let mut c = QueryCost {
        sql_output_rows: 5,
        sql_executions: 2,
        result_rows: 0,
        ..Default::default()
    };
    assert_eq!(c.work_units(), 205);
    assert_eq!(c.work_per_result_row(), 205.0);
    c.elapsed_micros = u64::MAX;
    assert_eq!(c.work_units(), 205);
    assert_eq!(c.report()["metric_version"], 1);
    c.native_input_rows = u64::MAX;
    assert_eq!(c.work_units(), u64::MAX);
}
