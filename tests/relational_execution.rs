//! Relational executor regression tests. The reference dispatcher provides an
//! independent oracle; production execution must report a DataFusion plan.
use new_graph::ir::jvm::JvmExecution;
use new_graph::ir::rel::runtime::execute_rows_with_jvm;
use new_graph::ir::{GraphPlan, PropertyGraph, Value};
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};
use std::collections::BTreeMap;

fn plan(query: &str) -> GraphPlan {
    GremlinPlanner::new()
        .plan(&parse_traversal(query).unwrap())
        .unwrap()
}
fn graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    let a = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), Value::String("a".into())),
            ("age".into(), Value::Int(29)),
        ]),
    );
    let b = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), Value::String("b".into())),
            ("age".into(), Value::Int(35)),
        ]),
    );
    let c = graph.insert_node(
        "software",
        BTreeMap::from([("name".into(), Value::String("c".into()))]),
    );
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    graph
        .insert_edge("created", &a, &c, BTreeMap::new())
        .unwrap();
    graph
}
#[tokio::test]
async fn typed_traversers_and_correlated_state_match_reference() {
    for query in [
        "g.V().groupCount().by('age')",
        "g.V().group().by(__.label()).by(__.count())",
        "g.V().choose(__.has('name','a'),__.values('name'),__.identity())",
        "g.V().outE().inV().values('name')",
        "g.V().local(__.out().count())",
        "g.V().has('name','a').as('a').out().as('a').select(Pop.first,'a')",
        "g.inject(null).coalesce(__.identity(),__.constant('fallback'))",
        "g.inject(1,2,3).store('x').limit(1).cap('x')",
        "g.inject(1).repeat(__.groupCount('m')).times(3).cap('m')",
    ] {
        let plan = plan(query);
        let graph = graph();
        let mut expected = new_graph::ir::interpreter::execute_rows(&plan, &graph.clone()).unwrap();
        let (mut actual, stats) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert!(!stats.physical_plan.is_empty(), "{query}");
        assert!(
            !stats.physical_plan.contains("GraphJvm"),
            "{}",
            stats.physical_plan
        );
        // These traversals do not request a result order. Compare the row
        // multiset while retaining exact types, hidden bindings, and bulk.
        actual.sort_by_key(|r| format!("{:?}:{}", r.bindings, r.bulk));
        expected.sort_by_key(|r| format!("{:?}:{}", r.bindings, r.bulk));
        assert_eq!(actual.len(), expected.len(), "{query}");
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.bulk, expected.bulk, "{query}");
            assert_eq!(actual.bindings, expected.bindings, "{query}");
        }
    }
}
#[tokio::test]
#[cfg(feature = "duckdb")]
async fn branch_scans_observe_prior_writes_and_outer_rollback() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    let result = engine
        .gremlin("g.inject(1).union(__.addV('person'),__.V()).count()")
        .await
        .unwrap();
    assert_eq!(result.stats.interpreted_ops, 0);
    assert!(result.stats.datafusion_ops > 0);
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 2);
    engine.begin().unwrap();
    engine.gremlin("g.addV('pending')").await.unwrap();
    engine.rollback().unwrap();
    let result = engine
        .gremlin("g.V().hasLabel('pending').count()")
        .await
        .unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 0);
}
#[test]
fn graph_checkpoints_isolate_writes_in_both_directions() {
    let graph = graph();
    let checkpoint = graph.clone();
    graph.insert_node("later", BTreeMap::new());
    assert!(checkpoint.node_ids("later").is_err());
    checkpoint.insert_node("checkpoint", BTreeMap::new());
    assert!(graph.node_ids("checkpoint").is_err());
}
