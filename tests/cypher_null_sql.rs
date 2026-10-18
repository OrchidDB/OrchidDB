#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{execute_lowered_sql, DuckDbExecutor}}, value::Value};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

async fn sql_rows(graph: &PropertyGraph, query: &str) -> Vec<Vec<String>> {
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, graph).unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap_or_else(|e| panic!("{query}: {e}"));
    (0..result.batch.num_rows()).map(|row| result.batch.columns().iter().map(|col| arrow::util::display::array_value_to_string(col, row).unwrap()).collect()).collect()
}

#[tokio::test]
async fn null_collection_operations_preserve_null_before_key_type_check() {
    let graph = PropertyGraph::new();
    for query in [
        "WITH null AS l RETURN size(l), size(null)",
        "WITH null AS m RETURN m.missing, m['missing']",
        "WITH null AS m RETURN m.missing IS NULL, m.missing IS NOT NULL",
    ] {
        let expected = if query.contains("IS NULL") { vec!["true", "false"] } else { vec!["", ""] };
        assert_eq!(sql_rows(&graph, query).await, vec![expected], "{query}");
    }
    let query = "WITH [1,2] AS xs RETURN xs['bad']";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    assert!(RelBackend::new().with_language_functions(true).lower(&plan, &graph).is_err());
}

#[tokio::test]
async fn absent_properties_are_null_not_varchar_for_numeric_consumers() {
    for populated in [false, true] {
        let graph = PropertyGraph::new();
        if populated { graph.insert_node("P", BTreeMap::new()); }
        assert_eq!(sql_rows(&graph, "MATCH (n) RETURN avg(n.age), count(n.age), count(*)").await,
            vec![vec!["".to_string(), "0".into(), if populated { "1" } else { "0" }.into()]]);
        assert_eq!(sql_rows(&graph, "MATCH (n) WITH n.age AS age RETURN age, age + count(*)").await.len(), if populated { 1 } else { 0 });
    }
    let graph = PropertyGraph::new();
    graph.insert_node("P", BTreeMap::from([("age".into(), Value::String("oops".into()))]));
    let plan = CypherPlanner::new().plan(&parse_query("MATCH (n) RETURN avg(n.age)").unwrap()).unwrap();
    assert!(RelBackend::new().with_language_functions(true).lower(&plan, &graph).is_err(), "real strings must not be silently numeric-coerced");
}
