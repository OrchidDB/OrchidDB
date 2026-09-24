#![cfg(feature = "duckdb")]
use new_graph::{
    engine::GraphEngine,
    ir::{catalog::PropertyGraph, value::Value as GValue},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
async fn native(engine: &mut GraphEngine, q: &str) -> Value {
    let result = engine
        .gremlin(q)
        .await
        .unwrap_or_else(|e| panic!("{q}: {e}"));
    serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap()
}
#[tokio::test]
async fn label_history_survives_rebinding_and_projection() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (pop, expected) in [
        ("first", json!({"type":"string","value":"a"})),
        (
            "all",
            json!({"type":"list","value":[{"type":"string","value":"a"},{"type":"string","value":"ax"}]}),
        ),
    ] {
        let q = format!("g.inject('a').as('x').concat('x').as('x').select(Pop.{pop},'x')");
        assert_eq!(native(&mut e, &q).await[0][0], expected);
    }
}
#[tokio::test]
async fn scoped_aggregate_and_store_keep_type_and_reducer() {
    let mut e = GraphEngine::in_memory().unwrap();
    for method in [
        "store('a')",
        "aggregate(Scope.local,'a')",
        "aggregate(local,'a')",
    ] {
        let value = native(&mut e, &format!("g.inject(1,2,2).{method}.cap('a')")).await;
        assert_eq!(value[0][0]["type"], "bulkset");
        assert_eq!(value[0][0]["value"].as_array().unwrap().len(), 3);
    }
    let value = native(
        &mut e,
        "g.withSideEffect('a',1,Operator.sum).inject(2,3).aggregate(Scope.local,'a').cap('a')",
    )
    .await;
    assert_eq!(value[0][0]["value"], 6);
}
#[tokio::test]
async fn optional_preserves_extended_path() {
    let mut e = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    let a = graph.insert_node(
        "person",
        BTreeMap::from([("name".into(), GValue::String("a".into()))]),
    );
    let b = graph.insert_node(
        "person",
        BTreeMap::from([("name".into(), GValue::String("b".into()))]),
    );
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    e.replace_graph(graph).unwrap();
    let rows = native(
        &mut e,
        "g.V().has('name','a').optional(__.out('knows')).path()",
    )
    .await;
    assert_eq!(rows[0][0]["type"], "path");
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 2);
    let rows = native(
        &mut e,
        "g.V().has('name','a').order().by(__.out('knows').values('name')).path()",
    )
    .await;
    assert_eq!(
        rows[0][0]["value"].as_array().unwrap().len(),
        1,
        "by modulator must not extend the outer traverser path"
    );
}
#[tokio::test]
async fn group_missing_values_keep_empty_group_not_null_member() {
    let mut e = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), GValue::String("a".into())),
            ("age".into(), GValue::Int(29)),
        ]),
    );
    graph.insert_node(
        "software",
        BTreeMap::from([("name".into(), GValue::String("b".into()))]),
    );
    e.replace_graph(graph).unwrap();
    let rows = native(&mut e, "g.V().group().by('name').by('age')").await;
    let entries = rows[0][0]["value"].as_array().unwrap();
    let missing = entries
        .iter()
        .find(|entry| entry[0]["value"] == "b")
        .unwrap();
    assert_eq!(missing[1], json!({"type":"list","value":[]}));
}
#[tokio::test]
async fn group_traversal_by_keeps_its_reducer_semantics() {
    let mut e = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    for name in ["a", "b"] {
        graph.insert_node(
            "person",
            BTreeMap::from([("name".into(), GValue::String(name.into()))]),
        );
    }
    e.replace_graph(graph).unwrap();
    let scalar = native(&mut e, "g.V().group().by(T.label).by(__.values('name'))").await;
    assert_eq!(
        scalar[0][0]["value"][0][1],
        json!({"type":"string","value":"b"})
    );
    let list = native(&mut e, "g.V().group().by(T.label).by('name')").await;
    assert_eq!(list[0][0]["value"][0][1]["type"], "list");
    assert_eq!(
        list[0][0]["value"][0][1]["value"].as_array().unwrap().len(),
        2
    );
}

#[tokio::test]
async fn match_repeated_labels_remain_join_constraints() {
    let mut e = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut e,
        "g.inject(1).match(__.as('a').constant(2).as('c'),__.as('a').constant(3).as('c'))",
    )
    .await;
    assert_eq!(rows, json!([]));
    let rows = native(
        &mut e,
        "g.inject(1).match(__.as('a').constant(2).as('c'),__.as('a').constant(2).as('c'))",
    )
    .await;
    assert_eq!(rows.as_array().unwrap().len(), 1);
}
