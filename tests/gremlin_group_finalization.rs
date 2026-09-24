#![cfg(feature = "duckdb")]

use new_graph::engine::GraphEngine;
use serde_json::Value;

async fn native(engine: &mut GraphEngine, query: &str) -> Value {
    let result = engine
        .gremlin(query)
        .await
        .unwrap_or_else(|error| panic!("{query}: {error}"));
    serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap()
}

#[tokio::test]
async fn only_explicit_cap_executes_post_reduction_writers() {
    // Pinned TinkerPop 3.7.4 GroupSideEffectStep + SideEffectCapStep:
    // select reads the raw barrier map; each explicit cap finalizes it once.
    for (tail, writes) in [
        ("select('m')", 0),
        ("cap('m')", 1),
        ("cap('m').cap('m')", 2),
        ("select('m').cap('m').select('m')", 1),
    ] {
        let mut engine = GraphEngine::in_memory().unwrap();
        let query = format!(
            "g.inject(1,2).group('m').by(__.constant('k')).by(__.count().sideEffect(__.addV('done'))).{tail}"
        );
        let rows = native(&mut engine, &query).await;
        assert_eq!(rows[0][0]["value"][0][1]["value"], 2, "{query}: {rows}");
        assert_eq!(
            native(&mut engine, "g.V().count()").await[0][0]["value"],
            writes,
            "{query}"
        );
    }
}

#[tokio::test]
async fn finalizers_run_per_key_and_do_not_replay_prefix_mutations() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject(1,2,3).group('m').by(__.choose(__.is(1),__.constant('a'),__.constant('b'))).by(__.sideEffect(__.addV('prefix')).count().sideEffect(__.addV('suffix'))).cap('m').cap('m')").await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 2);
    assert_eq!(
        native(&mut engine, "g.V().hasLabel('prefix').count()").await[0][0]["value"],
        3
    );
    assert_eq!(
        native(&mut engine, "g.V().hasLabel('suffix').count()").await[0][0]["value"],
        4
    );
}

#[tokio::test]
async fn new_contributions_reduce_into_the_published_value() {
    let mut engine = GraphEngine::in_memory().unwrap();
    // Each cap changes the published count to seven. The next occurrence
    // therefore increments seven, rather than recomputing the original rows.
    let rows = native(&mut engine, "g.inject(1).repeat(__.group('m').by(__.constant('k')).by(__.count().sideEffect(__.addV('done')).constant(7L)).sideEffect(__.cap('m'))).times(2).group('m').by(__.constant('k')).by(__.count().sideEffect(__.addV('done')).constant(7L)).select('m')").await;
    assert_eq!(rows[0][0]["value"][0][1]["value"], 8);
    assert_eq!(native(&mut engine, "g.V().count()").await[0][0]["value"], 2);
}

#[tokio::test]
async fn fold_finalization_appends_new_members_without_replaying_writes_on_reads() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject(1).repeat(__.group('m').by(__.constant('k')).by(__.fold().sideEffect(__.addV('done'))).sideEffect(__.cap('m')).sideEffect(__.select('m'))).times(3).select('m')").await;
    assert_eq!(
        rows[0][0]["value"][0][1]["value"].as_array().unwrap().len(),
        3
    );
    assert_eq!(native(&mut engine, "g.V().count()").await[0][0]["value"], 3);
}

#[tokio::test]
async fn finalizer_can_publish_native_elements_and_errors_roll_back_writes() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject(1,2).group('m').by(__.constant('k')).by(__.count().addV('result')).cap('m').cap('m').select('k').label()").await;
    assert_eq!(rows[0][0]["value"], "result");
    assert_eq!(native(&mut engine, "g.V().count()").await[0][0]["value"], 2);
    let result = engine.gremlin("g.inject(1).repeat(__.group('m').by(__.constant('k')).by(__.count().addV('rollback')).sideEffect(__.cap('m'))).times(2).cap('m')").await;
    assert!(
        result.is_err(),
        "a published vertex cannot be merged with a long count"
    );
    assert_eq!(
        native(&mut engine, "g.V().hasLabel('rollback').count()").await[0][0]["value"],
        0
    );
}
