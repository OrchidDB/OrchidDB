#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, value::Value, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

#[tokio::test]
async fn coalesce_selects_whole_graph_binding_and_preserves_optional_nulls() {
    let graph = PropertyGraph::new();
    let a=graph.insert_node("A",BTreeMap::new());
    let b=graph.insert_node("B",BTreeMap::from([("v".into(),Value::Int(7))]));
    graph.insert_edge("T", &a,&b,BTreeMap::new()).unwrap();
    for (query,expected) in [
        ("MATCH (a:A),(b:B) WITH coalesce(a,b) AS x RETURN x.v,labels(x)", vec!["|[A]"]),
        ("MATCH (a:A),(b:B) WITH coalesce(null,b,a) AS x RETURN x.v,labels(x)", vec!["7|[B]"]),
        ("MATCH (a:A) OPTIONAL MATCH (a)-[:Missing]->(b) WITH coalesce(b,a) AS x MATCH (x)-[:T]->(c) RETURN c.v", vec!["7"]),
        ("MATCH (a:A) OPTIONAL MATCH (a)-[:Missing]->(b) OPTIONAL MATCH (a)-[:Missing]->(c) WITH coalesce(b,c) AS x RETURN x IS NULL,x.v", vec!["true|"]),
        ("UNWIND [1,1] AS n MATCH (a:A) OPTIONAL MATCH (a)-[:Missing]->(b) WITH coalesce(b,a) AS x RETURN count(x)", vec!["2"]),
    ] {
        let ir=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered=RelBackend::new().with_language_functions(true).lower(&ir,&graph).unwrap_or_else(|e|panic!("{query}: {e}"));
        let result=execute_lowered_sql(&mut DuckDbExecutor::default(),&lowered).await.unwrap_or_else(|e|panic!("{query}: {e}"));
        let rows=(0..result.batch.num_rows()).map(|i|result.batch.columns().iter().map(|a|arrow::util::display::array_value_to_string(a,i).unwrap()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
        assert_eq!(rows,expected,"{query}");
    }
}

#[test]
fn mixed_graph_and_scalar_coalesce_remains_fenced() {
    let graph=PropertyGraph::new();
    let a=graph.insert_node("A",BTreeMap::new());
    graph.insert_edge("T", &a,&a,BTreeMap::new()).unwrap();
    for query in ["MATCH (a:A)-[r:T]->() WITH coalesce(a,r) AS x RETURN x", "MATCH (a:A) WITH coalesce(a,42) AS x RETURN x"] {
        let ir=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!(RelBackend::new().lower(&ir,&graph).is_err(),"{query}");
    }
}
