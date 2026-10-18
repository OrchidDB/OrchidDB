#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, value::Value, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;
async fn rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, graph)
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
    (0..result.batch.num_rows()).map(|row| result.batch.columns().iter().map(|c|
        arrow::util::display::array_value_to_string(c, row).unwrap().replace(", ", ",")).collect::<Vec<_>>().join("|")).collect()
}
#[tokio::test]
async fn logical_label_transform_preserves_duplicates_unicode_and_membership() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("Source", BTreeMap::from([("name".into(), Value::String("foo".into()))]));
    let b = graph.insert_node("Storage", BTreeMap::new());
    graph.set_node_labels(&b, ["FOO".into(), "Foo".into(), "Ä".into()]).unwrap();
    graph.insert_edge("T", &a, &b, BTreeMap::new()).unwrap();
    assert_eq!(rows(&graph, "MATCH (n)-->(b) WHERE n.name IN [x IN labels(b) | toLower(x)] RETURN [x IN labels(b) | toLower(x)]").await, ["[foo,foo,ä]"]);
    assert_eq!(rows(&graph, "MATCH (n)-->(b) RETURN 'missing' IN [x IN labels(b) | toLower(x)],null IN [x IN labels(b) | toLower(x)]").await, ["false|"]);
    graph.set_node_labels(&b, Vec::<String>::new()).unwrap();
    assert_eq!(rows(&graph, "MATCH (n)-->(b) RETURN [x IN labels(b) | toLower(x)],null IN [x IN labels(b) | toLower(x)]").await, ["[]|false"]);
}
#[tokio::test]
async fn optional_label_transform_preserves_null_list() {
    let graph = PropertyGraph::new();
    graph.insert_node("A", BTreeMap::new());
    assert_eq!(rows(&graph, "MATCH (n:A) OPTIONAL MATCH (n)-[:MISSING]->(b) RETURN [x IN labels(b) | toLower(x)],'a' IN [x IN labels(b) | toLower(x)]").await, ["|"]);
}
#[test]
fn arbitrary_label_transform_bodies_remain_fenced() {
    let graph = PropertyGraph::new();
    graph.insert_node("A", BTreeMap::new());
    for query in [
        "MATCH (n) RETURN 'a' IN [x IN labels(n) | toString(rand())]",
        "MATCH (n) RETURN 1 IN [x IN labels(n) | 1 / 0]",
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!(RelBackend::new().with_language_functions(true).lower(&plan, &graph).is_err(), "{query}");
    }
}
