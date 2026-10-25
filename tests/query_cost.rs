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
async fn batched_path_execution_counts_one_island_and_source_work() {
    let mut e = engine();
    let result=e.cypher("UNWIND ['p1','p2'] AS wanted MATCH path=(p:Person {id:wanted})-[:LINK]->(q:Person) RETURN path").await.unwrap();
    let c = &result.stats.cost;
    assert_eq!(c.result_rows, 2);
    assert_eq!(c.sql_executions, 1, "{c:?}");
    assert_eq!(c.native_kernel_calls, 0);
    assert_eq!(c.native_input_rows, 0);
    assert_eq!(c.native_output_rows, 0);
    assert!(c.source_engine.statements >= c.native_source_queries);
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
    assert_eq!(c.report()["metric_version"], 2);
    c.native_input_rows = u64::MAX;
    assert_eq!(c.work_units(), u64::MAX);
}

#[test]
fn engine_and_orchid_work_are_additive_with_explicit_partial_coverage() {
    let mut cost=QueryCost {sql_executions:1,sql_output_rows:2,..Default::default()};
    cost.engine.rows_scanned=10;
    cost.engine.operator_rows=20;
    assert_eq!(cost.work_units(),132);
    assert_eq!(cost.report()["unprofiled_sql_executions"],1);
    cost.engine.profiled_statements=1;
    assert_eq!(cost.report()["coverage"],"engine_and_boundary_work");
    cost.source_engine.statements=2;
    cost.source_engine.profiled_statements=1;
    cost.source_engine.operator_rows=3;
    assert_eq!(cost.orchid_work_units(),302);
    assert_eq!(cost.work_units(),335);
    assert_eq!(cost.report()["coverage"],"partial_engine_and_boundary_work");
    cost.engine.cpu_nanos=u64::MAX;
    cost.engine.latency_nanos=u64::MAX;
    assert_eq!(cost.work_units(),335);
}

#[tokio::test]
async fn read_only_queries_skip_insert_defaults_but_writes_keep_them() {
    let db=duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE SEQUENCE ids; CREATE TABLE people(id BIGINT PRIMARY KEY DEFAULT nextval('ids'),name VARCHAR DEFAULT 'anonymous')").unwrap();
    let mut mapping=GraphMapping::new();
    mapping.map_node(NodeMapping::table("Person","people","id").property("name","name"));
    let mut engine=GraphEngine::mapped(db,Arc::new(mapping)).unwrap();
    let read=engine.cypher("MATCH (p:Person) RETURN p.name").await.unwrap();
    assert_eq!(read.stats.cost.source_engine.statements,1,"one physical schema bind suffices for an empty read");
    let write=engine.cypher("CREATE (p:Person) RETURN p.name").await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(write.returned.batch.column(0).as_ref(),0).unwrap(),"anonymous");
    let read=engine.cypher("MATCH (p:Person) RETURN p.name").await.unwrap();
    assert_eq!(read.stats.cost.source_engine.statements,1);
    assert_eq!(arrow::util::display::array_value_to_string(read.returned.batch.column(0).as_ref(),0).unwrap(),"anonymous");
}
