#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
use serde_json::{Value, json};

async fn native(engine: &mut GraphEngine, query: &str) -> Value {
    let result = engine.gremlin(query).await.unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"]).unwrap()
}

#[tokio::test]
async fn bags_execute_mutation_once_and_survive_empty_stream() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject('alpha','beta','gamma').addV('item').aggregate('a').by(T.label).aggregate('b').by(T.label).cap('a','b')").await;
    let entries = rows[0][0]["value"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    for entry in entries {
        assert_eq!(entry[1]["type"], "bulkset");
        assert_eq!(entry[1]["value"].as_array().unwrap().len(), 3);
    }
    assert_eq!(native(&mut engine, "g.V().count()").await[0][0]["value"], 3);
    let rows = native(&mut engine, "g.inject(4,5).aggregate('x').filter(__.constant(false).is(true)).cap('x')").await;
    assert_eq!(rows[0][0]["value"], json!([{"type":"int","value":4},{"type":"int","value":5}]));
}

#[tokio::test]
async fn bags_accumulate_across_children_and_repeat_without_replaying_sources() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject(1).repeat(__.constant(2).aggregate('x')).times(3).cap('x')").await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 3);
    let rows = native(&mut engine, "g.inject(1,2).sideEffect(__.aggregate('x')).cap('x')").await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn finalized_group_count_keeps_numeric_and_null_keys() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject(10,20,null,20,10,10).groupCount('x').dedup().as('y').project('a','b').by().by(__.select('x').select(__.select('y')))").await;
    assert_eq!(rows.as_array().unwrap().len(), 3, "{rows}");
    let mut counts: Vec<i64> = rows.as_array().unwrap().iter().map(|row| {
        let entries = row[0]["value"].as_array().unwrap();
        assert!(entries.iter().any(|entry| entry[0]["value"] == "a"), "{row}");
        entries.iter().find(|entry| entry[0]["value"] == "b").unwrap()[1]["value"].as_i64().unwrap()
    }).collect();
    counts.sort();
    assert_eq!(counts, vec![1,2,3]);
}

#[tokio::test]
async fn unproductive_bag_projection_preserves_parent_and_productive_null() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let count = native(&mut engine, "g.inject(1,2,3).aggregate('x').by(__.filter(__.is(2))).count()").await;
    assert_eq!(count[0][0]["value"], 3);
    let rows = native(&mut engine, "g.inject(1,2,3).aggregate('x').by(__.filter(__.is(2))).cap('x')").await;
    assert_eq!(rows[0][0]["value"], json!([{"type":"int","value":2}]));
    let rows = native(&mut engine, "g.inject(null).aggregate('x').cap('x')").await;
    assert_eq!(rows[0][0]["value"], json!([{"type":"null","value":null}]));
}

#[tokio::test]
async fn eager_and_lazy_aggregate_have_distinct_slice_visibility() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let eager = native(&mut engine, "g.inject(1,2,3).aggregate('x').limit(1).cap('x')").await;
    let lazy = native(&mut engine, "g.inject(1,2,3).aggregate(local,'x').limit(1).cap('x')").await;
    assert_eq!(eager[0][0]["value"].as_array().unwrap().len(), 3);
    assert_eq!(lazy[0][0]["value"].as_array().unwrap().len(), 1);
    let rows = native(&mut engine, "g.inject(1,2,3).where(P.without('x')).aggregate('x').cap('x')").await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn hybrid_uses_bulk_frontier_for_deep_repeat() {
    use orchiddb::ir::{catalog::PropertyGraph, value::Value as GValue};
    use std::collections::BTreeMap;
    let graph = PropertyGraph::new();
    let node = graph.insert_node("item", BTreeMap::new());
    graph.insert_edge("next", &node, &node, BTreeMap::new()).unwrap();
    graph.insert_edge("next", &node, &node, BTreeMap::from([("n".into(), GValue::Int(1))])).unwrap();
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.replace_graph(graph).unwrap();
    let rows = native(&mut engine, "g.V().repeat(__.out()).times(50).count()").await;
    assert_eq!(rows[0][0]["value"], 1_u64 << 50);
}

#[tokio::test]
async fn project_distinguishes_productive_null_from_absent_child() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject(null).project('x').by()").await;
    assert_eq!(rows[0][0]["value"], json!([[{"type":"string","value":"x"},{"type":"null","value":null}]]));
    let rows = native(&mut engine, "g.inject(1).project('x').by(__.constant(null))").await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 1);
    let rows = native(&mut engine, "g.inject(1).project('x').by(__.filter(__.is(2)))").await;
    assert!(rows[0][0]["value"].as_array().unwrap().is_empty());
    let rows = native(&mut engine, "g.withStrategies(ProductiveByStrategy).inject(1).project('x').by(__.filter(__.is(2)))").await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn multiple_property_keys_execute_upstream_mutation_and_store_once() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.addV('item').property('a',1).property('b',2).store('x').values('a','b').cap('x').unfold().count()").await;
    assert_eq!(rows[0][0]["value"], 1);
    assert_eq!(native(&mut engine, "g.V().count()").await[0][0]["value"], 1);
}

#[tokio::test]
async fn label_retraction_keeps_dynamic_mutation_endpoint_bindings() {
    let mut engine = GraphEngine::in_memory().unwrap();
    native(&mut engine, "g.inject(1,2).addV('item')").await;
    let rows = native(&mut engine, "g.V().aggregate('x').as('a').select('x').unfold().addE('link').to('a').count()").await;
    assert_eq!(rows[0][0]["value"], 4);
    assert_eq!(native(&mut engine, "g.E().count()").await[0][0]["value"], 4);
}

#[tokio::test]
async fn native_property_children_keep_labels_and_register_forward_side_effects() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.addV('item').as('a').constant('value').as('b').select('a').local(__.property(single,'p',__.select('b'))).values('p')").await;
    assert_eq!(rows[0][0]["value"], "value");
    let rows = native(&mut engine, "g.addV('registered').where(P.without('x')).property(single,'p',__.constant(1).aggregate('x')).cap('x')").await;
    assert_eq!(rows[0][0]["value"], json!([{"type":"int","value":1}]));
    assert_eq!(native(&mut engine, "g.V().hasLabel('registered').values('p')").await[0][0]["value"], 1);
}
