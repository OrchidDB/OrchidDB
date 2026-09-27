#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::GraphEngine,
    ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping},
};
use std::sync::Arc;
fn engine() -> GraphEngine {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE vertices(id VARCHAR PRIMARY KEY); INSERT INTO vertices VALUES ('a'),('b'),('c'); CREATE TABLE edges(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO edges VALUES ('ab','a','b'),('ac','a','c'),('ba','b','a'),('ca','c','a')").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("V", "vertices", "id").property("id", "id"));
    mapping.map_edge(EdgeMapping::table("E", "edges", "src", "dst", "V", "V").with_id("id"));
    GraphEngine::mapped(db, Arc::new(mapping)).unwrap()
}
#[tokio::test]
async fn repeat_count_is_one_weighted_sql_island_preserving_bags() {
    let mut engine = engine();
    for (query, expected) in [
        ("g.V().repeat(__.out()).times(8).count()", "48"),
        (
            "g.withBulk(false).V().repeat(__.out()).times(8).count()",
            "48",
        ),
        ("g.V().repeat(__.out()).times(5).count()", "16"),
        ("g.V().times(0).repeat(__.out()).count()", "3"),
        (
            "g.V().has('id','absent').repeat(__.out()).times(8).count()",
            "0",
        ),
        ("g.V().repeat(__.out()).times(50).count()", "100663296"),
        (
            "g.V().repeat(__.out()).times(5).as('a').out('E').as('b').select('a','b').count()",
            "24",
        ),
        ("g.V().repeat(__.out()).times(0).count()", "4"),
        ("g.V().has('id','a').repeat(__.out()).times(2).count()", "2"),
    ] {
        let result = engine.gremlin(query).await.unwrap();
        let actual =
            arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0)
                .unwrap();
        assert_eq!(actual, expected, "{query}");
        assert_eq!(
            result.stats.cost.sql_executions, 1,
            "{query}: {:?}",
            result.stats
        );
        assert_eq!(
            result.stats.cost.sql_output_rows, 1,
            "{query}: {:?}",
            result.stats
        );
        assert_eq!(
            result.stats.cost.native_kernel_calls, 0,
            "{query}: {:?}",
            result.stats
        );
        assert!(
            result
                .stats
                .sql_queries
                .iter()
                .any(|sql| sql.contains("__w_sql_cte_weighted_repeat_")),
            "{query}: {:?}",
            result.stats
        );
    }
}

#[tokio::test]
async fn repeat_control_and_stateful_bodies_keep_their_semantics() {
    let mut engine = engine();
    for (query, expected) in [
        ("g.withSack(1).V().repeat(__.out()).times(8).count()", "48"),
        ("g.inject(1).repeat(__.loops()).times(2).count()", "1"),
        ("g.V().repeat(__.dedup()).times(2).count()", "0"),
        ("g.V().repeat(__.out()).times(2).emit().count()", "10"),
        (
            "g.V().has('id','a').repeat(__.out()).until(__.loops().is(2)).count()",
            "2",
        ),
        ("g.V().repeat(__.out()).times(2).path().count()", "6"),
    ] {
        let result = engine.gremlin(query).await.unwrap();
        assert_eq!(
            arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0)
                .unwrap(),
            expected,
            "{query}"
        );
        assert!(
            result.stats.cost.native_kernel_calls > 0,
            "{query}: {:?}",
            result.stats.cost
        );
    }
}

#[tokio::test]
async fn weighted_counts_preserve_unsigned_bulk_and_signed_long_results() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE vertices(id VARCHAR PRIMARY KEY); INSERT INTO vertices VALUES ('a'); CREATE TABLE edges(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO edges SELECT 'e'||i,'a','a' FROM range(10) t(i)").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("V", "vertices", "id"));
    mapping.map_edge(EdgeMapping::table("E", "edges", "src", "dst", "V", "V").with_id("id"));
    let mut engine = GraphEngine::mapped(db, Arc::new(mapping)).unwrap();
    let result = engine
        .gremlin("g.V().repeat(__.out()).times(19).count()")
        .await
        .unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(),
        "-8446744073709551616"
    );
    assert_eq!(result.stats.cost.sql_output_rows, 1);
    assert!(
        engine
            .gremlin("g.V().repeat(__.out()).times(20).count()")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn weighted_frontiers_keep_labels_with_overlapping_scalar_ids_and_null_properties() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE a(id VARCHAR PRIMARY KEY, p VARCHAR); INSERT INTO a VALUES ('same',NULL); CREATE TABLE b(id VARCHAR PRIMARY KEY, p VARCHAR); INSERT INTO b VALUES ('same',NULL); CREATE TABLE aa(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO aa VALUES ('e1','same','same'); CREATE TABLE ab(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO ab VALUES ('e2','same','same'); CREATE TABLE ba(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO ba VALUES ('e3','same','same')").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("A", "a", "id").property("p", "p"));
    mapping.map_node(NodeMapping::table("B", "b", "id").property("p", "p"));
    for (table, source, target) in [("aa", "A", "A"), ("ab", "A", "B"), ("ba", "B", "A")] {
        mapping
            .map_edge(EdgeMapping::table(table, table, "src", "dst", source, target).with_id("id"));
    }
    let mut engine = GraphEngine::mapped(db, Arc::new(mapping)).unwrap();
    for (times, count) in [(1, 3), (2, 5), (3, 8)] {
        let result = engine
            .gremlin(&format!("g.V().repeat(__.out()).times({times}).count()"))
            .await
            .unwrap();
        assert_eq!(
            arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0)
                .unwrap(),
            count.to_string()
        );
        assert_eq!(result.stats.cost.sql_output_rows, 1);
        assert_eq!(result.stats.cost.native_kernel_calls, 0);
    }
}
