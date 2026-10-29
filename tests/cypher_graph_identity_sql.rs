#![cfg(feature = "duckdb")]

use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{DuckDbExecutor, SqlDialect, execute_lowered_sql, unparse}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

/// Compile with native-value support disabled: passing only through the engine
/// scheduler would not prove that the predicates use ordinary SQL.
#[tokio::test]
async fn structural_graph_predicates_use_identity_and_preserve_optional_nulls() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("A", BTreeMap::new());
    let b = graph.insert_node("B", BTreeMap::new());
    graph.insert_edge("T", &a, &b, BTreeMap::new()).unwrap();
    // A and B deliberately have the same per-label offset; label is part of
    // identity. An edge can also share that offset without equaling a node.
    for (query, expected) in [
        ("MATCH (a:A),(b:B) RETURN a=a,a=b,a<>b,a IS NULL,b IS NOT NULL", "true|false|true|false|true"),
        ("MATCH (a:A)-[r:T]->(b:B) RETURN type(r),r=r,a=r,r<>a", "T|true|false|true"),
        ("MATCH (a:A) WITH a AS x RETURN x=x,x IS NOT NULL", "true|true"),
        ("MATCH (b:B) OPTIONAL MATCH (b)-[r:T]->(n) RETURN n IS NULL,n IS NOT NULL,type(r),n=n,n<>b", "true|false|||"),
        ("MATCH (b:B) OPTIONAL MATCH (b)-[r:T]->(n) RETURN count(n),count(DISTINCT n),count(r),count(*)", "0|0|0|1"),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        assert!(!sql.contains("__orchiddb_value"), "{query}: {sql}");
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        assert_eq!(result.batch.num_rows(), 1, "{query}");
        let row = result.batch.columns().iter().map(|array|
            arrow::util::display::array_value_to_string(array, 0).unwrap()
        ).collect::<Vec<_>>().join("|");
        assert_eq!(row, expected, "{query}\n{sql}");
    }
}

#[tokio::test]
async fn invalid_type_arguments_are_not_reinterpreted_as_graph_bindings() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:A)").await.unwrap();
    for query in ["RETURN type(42)", "MATCH (n:A) RETURN type(n)"] {
        assert!(engine.cypher(query).await.is_err(), "{query}");
    }
    let result = engine.cypher("MATCH (n:A) WITH 42 AS n RETURN n IS NULL,n IS NOT NULL,n=n")
        .await.unwrap();
    let row = result.returned.batch.columns().iter().map(|array|
        arrow::util::display::array_value_to_string(array, 0).unwrap()
    ).collect::<Vec<_>>().join("|");
    assert_eq!(row, "false|true|true");
}

#[tokio::test]
async fn terminal_graph_results_keep_logical_fields_and_flat_identity_columns() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("A", BTreeMap::from([("k".into(), orchiddb::ir::value::Value::Int(1))]));
    let b = graph.insert_node("B", BTreeMap::new());
    graph.insert_edge("T", &a, &b, BTreeMap::new()).unwrap();
    for (query, fields, rows) in [
        ("MATCH (a:A)-[r:T]->(b:B) RETURN b,r,a", vec!["b", "r", "a"], 1),
        ("MATCH (a:A) WITH a AS copy RETURN copy", vec!["copy"], 1),
        ("MATCH (b:B) OPTIONAL MATCH (b)-[r:T]->(n) RETURN n,r,b", vec!["n", "r", "b"], 1),
        ("MATCH (a:A) WHERE false RETURN a", vec!["a"], 0),
        ("MATCH (n) RETURN n ORDER BY n.k LIMIT 1", vec!["n"], 1),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(lowered.fields, fields, "{query}");
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        assert!(!sql.contains("__orchiddb_value"), "{query}: {sql}");
        let returned = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        assert_eq!(returned.fields, fields, "{query}");
        assert_eq!(returned.batch.num_rows(), rows, "{query}");
        for field in fields {
            assert!(returned.batch.schema().field_with_name(&format!("{field}__id")).is_ok(), "{query}");
            assert!(returned.batch.schema().field_with_name(&format!("{field}__label")).is_ok(), "{query}");
        }
    }
}

#[tokio::test]
async fn terminal_graph_transport_restores_public_typed_results_and_nulls() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (a:A:B {k:1}),(b:C {k:2}),(a)-[:T {weight:3}]->(b)").await.unwrap();
    let result = engine.cypher("MATCH (a:A)-[r:T]->(b:C) RETURN b AS last,r,a AS first").await.unwrap();
    assert_eq!(result.returned.fields, ["last", "r", "first"]);
    let typed: serde_json::Value = serde_json::from_str(&result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"]).unwrap();
    assert_eq!(typed[0][0]["type"], "vertex");
    assert_eq!(typed[0][0]["properties"]["k"]["value"], 2);
    assert_eq!(typed[0][1]["type"], "edge");
    assert_eq!(typed[0][1]["label"], "T");
    assert_eq!(typed[0][1]["properties"]["weight"]["value"], 3);
    assert_eq!(typed[0][2]["labels"], serde_json::json!(["A", "B"]));
    assert_eq!(typed[0][2]["properties"]["k"]["value"], 1);
    assert_eq!(typed[0][1]["outV"], typed[0][2]["id"]);
    assert_eq!(typed[0][1]["inV"], typed[0][0]["id"]);
    assert_eq!(result.stats.cost.native_relational_kernel_calls, 0);

    let result = engine.cypher("MATCH (b:C) OPTIONAL MATCH (b)-[r:T]->(n) RETURN n,r,b").await.unwrap();
    let typed: serde_json::Value = serde_json::from_str(&result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"]).unwrap();
    assert_eq!(typed[0][0]["type"], "null");
    assert_eq!(typed[0][1]["type"], "null");
    assert_eq!(typed[0][2]["type"], "vertex");
    assert_eq!(result.stats.cost.native_relational_kernel_calls, 0);
}
