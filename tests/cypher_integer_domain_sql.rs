#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, value::Value, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

async fn rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, graph)
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
    (0..result.batch.num_rows()).map(|row| arrow::util::display::array_value_to_string(result.batch.column(0), row).unwrap()).collect()
}

#[tokio::test]
async fn finite_string_integer_conversion_preserves_reference_grammar_and_precision() {
    let query = "UNWIND ['0x2a','-0x2a','1_2','2.9','-2.9','1_2.9','9007199254740993','9223372036854775808','-9223372036854775808','1e999','NaN','Infinity','bad','',null] AS x RETURN toInteger(x)";
    assert_eq!(rows(&PropertyGraph::new(), query).await, ["42", "-42", "12", "2", "-2", "", "9007199254740993", "9223372036854775807", "-9223372036854775808", "", "", "", "", "", ""]);
}

#[tokio::test]
async fn immutable_property_domains_survive_join_alias_and_optional_null_extension() {
    let graph = PropertyGraph::new();
    graph.insert_node("Person", BTreeMap::from([("name".into(), Value::String("42".into()))]));
    graph.insert_node("Person", BTreeMap::from([("name".into(), Value::String("2.9".into()))]));
    assert_eq!(rows(&graph, "MATCH (p:Person {name:'42'}) WITH * MATCH (n:Person) WITH n.name AS x RETURN toInteger(x) AS x ORDER BY x").await, ["2", "42"]);
    assert_eq!(rows(&graph, "MATCH (p:Person) OPTIONAL MATCH (p)-[:MISSING]->(n:Person) RETURN toInteger(n.name)").await, ["", ""]);
    graph.insert_node("Person", BTreeMap::from([("name".into(), Value::String("0x10".into()))]));
    assert_eq!(rows(&graph, "MATCH (n:Person) RETURN toInteger(n.name) AS x ORDER BY x").await, ["2", "16", "42"]);
}

#[test]
fn unbounded_or_volatile_string_conversions_remain_fenced() {
    let graph = PropertyGraph::new();
    for query in ["RETURN toInteger(toString(rand()))", "UNWIND ['1','2'] AS x RETURN toInteger(x + toString(rand()))", "UNWIND [0] AS n WITH CASE WHEN 1/n = 0 THEN '1' ELSE null END AS x RETURN toInteger(x)"] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!(RelBackend::new().with_language_functions(true).lower(&plan, &graph).is_err(), "{query}");
    }
}

#[tokio::test]
async fn all_null_snapshot_string_domain_is_preserved() {
    let graph = PropertyGraph::new();
    graph.insert_node("Present", BTreeMap::from([("name".into(), Value::String("42".into()))]));
    graph.insert_node("Other", BTreeMap::new());
    assert_eq!(rows(&graph, "MATCH (n:Other) OPTIONAL MATCH (n)-[:MISSING]->(p:Present) WITH p.name AS x RETURN toInteger(x)").await, [""]);
}
