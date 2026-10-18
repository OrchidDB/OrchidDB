#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, value::Value, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

#[tokio::test]
async fn constructor_selection_retains_structural_graph_values() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("A", BTreeMap::from([("existing".into(), Value::Int(42))]));
    let b = graph.insert_node("B", BTreeMap::new());
    graph.insert_edge("T", &a, &b, BTreeMap::from([("existing".into(), Value::Int(24))])).unwrap();
    for (query, expected) in [
        ("MATCH (a:A) WITH [a,1] AS xs RETURN labels(xs[0])", "[A]"),
        ("MATCH (a:A) WITH {node:a,ignored:1} AS m RETURN labels(m.node)", "[A]"),
        ("MATCH (a:A)-[r:T]->() WITH [r,1] AS xs RETURN type(xs[0])", "T"),
        ("MATCH (a:A) WITH [123,a] AS xs RETURN xs[1].missing,xs[1].existing", "|42"),
        ("MATCH ()-[r:T]->() WITH [123,r] AS xs RETURN xs[1].missing,xs[1].existing", "|24"),
        ("MATCH (a:A) WITH [123,a] AS xs RETURN labels(xs[-1]),xs[7] IS NULL", "[A]|true"),
        ("MATCH (a:A) WITH [a,1] AS xs,a RETURN xs[0]=a", "true"),
        ("MATCH (a:A) WITH [a,1] AS xs WITH xs AS ys RETURN labels(ys[0])", "[A]"),
        ("MATCH (a:B) OPTIONAL MATCH (a)-[r:T]->() WITH [r,1] AS xs RETURN type(xs[0])", ""),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
        assert_eq!(result.batch.num_rows(), 1, "{query}");
        let row = result.batch.columns().iter().map(|array|
            arrow::util::display::array_value_to_string(array, 0).unwrap()).collect::<Vec<_>>().join("|");
        assert_eq!(row, expected, "{query}");
    }
}
