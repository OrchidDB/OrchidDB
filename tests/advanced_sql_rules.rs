//! Semantic and physical-shape checks for the advanced SQL rule batch.
#![cfg(feature = "duckdb")]
use orchiddb::ir::plan::{DistinctBulk, DistinctMode, LabelExpr, Node};
use orchiddb::ir::policy::GraphPlanPolicy;
use orchiddb::ir::rel::{
    RelBackend,
    sql::{DuckDbExecutor, execute_lowered_sql},
};
use orchiddb::ir::{GraphPlan, PropertyGraph};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

fn graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("Person", BTreeMap::new());
    let b = graph.insert_node("Person", BTreeMap::new());
    let c = graph.insert_node("Person", BTreeMap::new());
    graph.insert_edge("KNOWS", &a, &b, BTreeMap::new()).unwrap();
    graph.insert_edge("KNOWS", &a, &c, BTreeMap::new()).unwrap();
    graph
}

async fn run(query: &str) -> (Vec<Vec<String>>, String) {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let mut lowered = RelBackend::new().lower(&plan, &graph()).unwrap();
    lowered.plan = datafusion::prelude::SessionContext::new()
        .state().optimize(&lowered.plan).unwrap();
    let explain = lowered.plan.display_indent().to_string();
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap();
    let rows = (0..result.batch.num_rows())
        .map(|i| {
            result
                .batch
                .columns()
                .iter()
                .map(|a| arrow::util::display::array_value_to_string(a.as_ref(), i).unwrap())
                .collect()
        })
        .collect();
    (rows, explain)
}

#[tokio::test]
async fn count_distinct_keeps_null_group_and_avoids_representative_windows() {
    let (rows, explain) =
        run("UNWIND [1,1,null,null,2] AS x WITH DISTINCT x RETURN count(*) AS n").await;
    assert_eq!(rows, vec![vec!["3"]]);
    assert!(!explain.contains("WindowAggr"), "{explain}");
    assert!(explain.contains("groupBy=[[x]]"), "{explain}");
}

#[tokio::test]
async fn ordered_distinct_still_preserves_first_encounter() {
    let (rows, _) = run("UNWIND [2,1,2] AS x WITH DISTINCT x RETURN x").await;
    assert_eq!(rows, vec![vec!["2"], vec!["1"]]);
}

#[tokio::test]
async fn singleton_unwind_keeps_duplicate_parents_and_cartesian_semantics() {
    let (rows, explain) = run("UNWIND [1,1,2] AS x UNWIND [7] AS y RETURN x,y").await;
    assert_eq!(rows, vec![vec!["1", "7"], vec!["1", "7"], vec!["2", "7"]]);
    assert_eq!(explain.matches("TableScan:").count(), 2, "{explain}");
    let (rows, _) = run("UNWIND [1,2] AS x UNWIND [7,8,9] AS y RETURN x,y").await;
    assert_eq!(rows.len(), 6);
    let (rows, _) = run("UNWIND [1,1] AS x UNWIND [null] AS y RETURN x,y").await;
    assert_eq!(rows, vec![vec!["1", ""], vec!["1", ""]]);
    let (rows, _) = run("UNWIND [] AS x UNWIND [7] AS y RETURN y").await;
    assert!(rows.is_empty());
}

#[tokio::test]
async fn catalog_identity_eliminates_dedup_but_expansion_multiplicity_does_not() {
    let node = Node::GraphDistinct {
        keys: vec!["p".into()],
        mode: DistinctMode::Row,
        bulk: DistinctBulk::NotApplicable,
        input: Box::new(Node::GraphNodeScan {
            graph: "default".into(),
            binding: "p".into(),
            labels: LabelExpr::Any,
        }),
    };
    let plan = GraphPlan::new(GraphPlanPolicy::cypher(), node);
    let lowered = RelBackend::new().lower(&plan, &graph()).unwrap();
    let explain = lowered.plan.display_indent().to_string();
    assert!(!explain.contains("WindowAggr"), "{explain}");
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap();
    assert_eq!(result.batch.num_rows(), 3);
    let (rows, explain) = run("MATCH (p:Person)-[:KNOWS]->(q) WITH DISTINCT p RETURN p").await;
    assert_eq!(rows.len(), 1);
    assert!(explain.contains("WindowAggr"), "{explain}");
}

#[tokio::test]
async fn production_gremlin_distinct_and_existence_keep_bulk_and_duplicates() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine.replace_graph(graph()).unwrap();
    for (query, expected) in [
        ("g.inject(1,1,2).dedup().count()", 2),
        ("g.V().dedup().count()", 3),
        ("g.V().out().dedup().count()", 2),
        ("g.inject(1,1,2).filter(__.dedup()).count()", 3),
        ("g.inject(1,1,2).not(__.dedup()).count()", 0),
    ] {
        let result = engine.gremlin(query).await.unwrap();
        let rows: serde_json::Value = serde_json::from_str(
            &result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
        )
        .unwrap();
        assert_eq!(rows[0][0]["value"], expected, "{query}");
    }
}

#[tokio::test]
async fn optimized_fanout_distinct_count_retains_grouping() {
    // One person has two outgoing edges. Count people once, not edges twice.
    let (rows, explain) = run(
        "MATCH (p:Person)-[:KNOWS]->(q) WITH DISTINCT p RETURN count(*) AS n"
    ).await;
    assert_eq!(rows, vec![vec!["1"]], "{explain}");
    assert!(!explain.contains("WindowAggr"), "{explain}");
    assert!(explain.matches("Aggregate:").count() >= 2, "{explain}");
    let (rows, _) = run(
        "MATCH (p:Person)-[:KNOWS]->(q) RETURN count(*) AS n"
    ).await;
    assert_eq!(rows, vec![vec!["2"]]);
}
