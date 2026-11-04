#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, execute_lowered_sql},
    },
    value::Value,
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

fn graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("A", BTreeMap::from([("age".into(), Value::Int(20))]));
    let b = graph.insert_node("B", BTreeMap::from([("age".into(), Value::Int(30))]));
    graph.insert_edge("R", &a, &b, BTreeMap::new()).unwrap();
    graph.insert_edge("R", &a, &b, BTreeMap::new()).unwrap();
    graph.insert_edge("R", &a, &a, BTreeMap::new()).unwrap();
    graph
}

#[tokio::test]
async fn dead_paths_and_fixed_lengths_preserve_bags_and_trails_in_plain_sql() {
    let graph = graph();
    for (query, expected) in [
        ("MATCH p=(a)-[:R]->(b) RETURN count(*)", "3"),
        ("MATCH p=(a)-[:R]->(b) RETURN count(p)", "3"),
        ("MATCH p=(a)-[:R]->(b)-[:R]->(c) RETURN count(p)", "2"),
        ("MATCH p=(a)-[:R]->(b) WITH p AS q RETURN count(q)", "3"),
        ("MATCH p=(a)-[:R]->(b) WHERE false RETURN count(p)", "0"),
        ("MATCH p=(a)-[:R]->(b) WITH null AS p RETURN count(p)", "0"),
        ("MATCH p=(a)-[:R]->(b) RETURN avg(a.age)", "20.0"),
        (
            "MATCH p=(a)-[:R]->(b) WHERE length(p)=1 RETURN count(*)",
            "3",
        ),
        (
            "MATCH p=(a)-[:R]->(b) WHERE length(p)=2 RETURN count(*)",
            "0",
        ),
        ("MATCH p=(a)-[:R]->(b) RETURN sum(length(p))", "3"),
        ("MATCH p=(a)-[:R]-(b) RETURN count(*)", "5"),
        ("MATCH p=(a)-[:R]->(b)-[:R]->(c) RETURN count(*)", "2"),
    ] {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(result.batch.num_rows(), 1, "{query}");
        let actual =
            arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap();
        assert_eq!(actual, expected, "{query}");
    }
}

#[tokio::test]
async fn observed_paths_optional_lengths_and_distinct_paths_execute_in_text_mode() {
    let graph = graph();
    for (query, expected) in [
        ("MATCH p=(a)-[:R]->(b) RETURN count(DISTINCT p)", vec!["3"]),
        ("MATCH (a:A) OPTIONAL MATCH p=(a)-[:Missing]->(b) RETURN count(p)", vec!["0"]),
        ("MATCH (a:A) OPTIONAL MATCH p=(a)-[:Missing]->(b) RETURN length(p)", vec![""]),
        ("MATCH p=(a)-[:R]->(b)-[:R]->(c) RETURN length(p)", vec!["2", "2"]),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true)
            .lower(&plan, &graph).unwrap_or_else(|error| panic!("{query}: {error}"));
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await.unwrap_or_else(|error| panic!("{query}: {error}"));
        let actual: Vec<_> = (0..result.batch.num_rows()).map(|row|
            arrow::util::display::array_value_to_string(result.batch.column(0), row).unwrap()
        ).collect();
        assert_eq!(actual, expected, "{query}");
    }
    let plan = CypherPlanner::new().plan(
        &parse_query("MATCH p=(a)-[:R]->(b) RETURN p").unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
    let rows = orchiddb::ir::rel::native_values::decode_rows(&result.batch, &lowered.fields, &graph).unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        assert!(matches!(row.get("p"), Value::Path(items) if items.len() == 3));
    }
}

#[tokio::test]
async fn impossible_ranges_keep_fresh_endpoint_schema_and_optional_nulls() {
    let graph = graph();
    for range in ["2..1", "1..0", "..0"] {
        for optional in [false, true] {
            let query = format!(
                "MATCH (a:A) {}MATCH (a)-[:R*{range}]->(c) RETURN c.age",
                if optional { "OPTIONAL " } else { "" }
            );
            let plan = CypherPlanner::new()
                .plan(&parse_query(&query).unwrap())
                .unwrap();
            let lowered = RelBackend::new()
                .with_language_functions(true)
                .lower(&plan, &graph)
                .unwrap_or_else(|e| panic!("{query}: {e}"));
            let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
                .await
                .unwrap_or_else(|e| panic!("{query}: {e}"));
            assert_eq!(result.batch.num_rows(), usize::from(optional), "{query}");
            if optional {
                assert!(result.batch.column(0).is_null(0), "{query}");
            }
        }
    }
}

#[tokio::test]
async fn unobserved_recursive_paths_omit_display_payload_and_keep_trail_multiplicity() {
    let graph = graph();
    // Two parallel a->b edges and one a->a loop: two zero-hop paths,
    // three one-hop paths, and two two-hop trails (loop then either edge).
    let query = "MATCH p=(a)-[:R*0..2]->(b) RETURN count(p) AS c";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
    let sql = orchiddb::ir::rel::sql::unparse(&lowered, orchiddb::ir::rel::sql::SqlDialect::DuckDb).unwrap();
    assert!(!sql.contains("__w_nodes"), "{sql}");
    assert!(!sql.contains("__w_rels"), "{sql}");
    assert!(sql.contains("__w_trail"), "{sql}");
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(), "7");
}

#[test]
fn materialized_multihop_paths_allocate_distinct_anonymous_relationship_bindings() {
    let graph = graph();
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH p=(a)-[:R]->(b)-[:R]->(c) RETURN p").unwrap())
        .unwrap();
    let lowered = RelBackend::with_options(orchiddb::ir::rel::RelBackendOptions {
        native_values: true,
        language_functions: true,
        ..Default::default()
    }).lower(&plan, &graph).unwrap();
    assert!(lowered.islands.is_complete());
}
