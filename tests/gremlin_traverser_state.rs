#![cfg(feature = "duckdb")]
use orchiddb::{
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

#[tokio::test]
async fn path_projects_native_elements_before_collection_steps() {
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
    let c = graph.insert_node(
        "person",
        BTreeMap::from([("name".into(), GValue::String("c".into()))]),
    );
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    graph.insert_edge("knows", &b, &c, BTreeMap::new()).unwrap();
    e.replace_graph(graph).unwrap();
    let rows = native(
        &mut e,
        "g.union(__.V().out().out()).path().by('name').combine([])",
    )
    .await;
    assert_eq!(
        rows[0][0]["value"],
        json!([{"type":"string","value":"a"},{"type":"string","value":"b"},{"type":"string","value":"c"}])
    );
    let rows = native(&mut e, "g.V().values('name').as('x').select(Pop.all,'x')").await;
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|row| row[0]["type"] == "list" && row[0]["value"].as_array().unwrap().len() == 1)
    );
}
#[tokio::test]
async fn repeat_modulators_require_repeat_body() {
    let mut e = GraphEngine::in_memory().unwrap();
    for q in [
        "g.V().emit()",
        "g.V().until(__.identity())",
        "g.V().times(5)",
    ] {
        assert!(
            e.gremlin(q)
                .await
                .unwrap_err()
                .to_string()
                .contains("The repeat()-traversal was not defined")
        );
    }
}

#[tokio::test]
async fn path_filters_slice_and_project_without_replacing_current() {
    let mut e = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    let a = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), GValue::String("a".into())),
            ("age".into(), GValue::Int(1)),
        ]),
    );
    let b = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), GValue::String("b".into())),
            ("age".into(), GValue::Int(1)),
        ]),
    );
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    e.replace_graph(graph).unwrap();
    assert_eq!(
        native(&mut e, "g.V().out().cyclicPath().by('age').values('name')").await[0][0]["value"],
        "b"
    );
    assert_eq!(
        native(&mut e, "g.V().out().simplePath().by('age')").await,
        json!([])
    );
    assert_eq!(
        native(
            &mut e,
            "g.V().has('name','a').as('a').out().as('b').in().cyclicPath().from('a').to('b')"
        )
        .await,
        json!([])
    );
    let row = native(&mut e, "g.inject(0).V().has('name','a').path()").await;
    assert_eq!(row[0][0]["value"].as_array().unwrap().len(), 2);
    assert_eq!(row[0][0]["value"][0]["value"], 0);
    let row = native(
        &mut e,
        "g.V().has('name','a').as('a').out().as('b').in().path().by('name').from('a').to('b')",
    )
    .await;
    assert_eq!(row[0][0]["value"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn union_barriers_consume_each_complete_arm_stream() {
    let mut e = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut e,
        "g.inject(1,2,3).union(__.count(),__.sum(),__.fold())",
    )
    .await;
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert_eq!(rows[0][0]["value"], 3);
    assert_eq!(rows[1][0]["value"], 6);
    assert_eq!(rows[2][0]["value"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn label_dedup_modulators_drop_unproductive_keys() {
    let mut e = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    let a = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), GValue::String("a".into())),
            ("age".into(), GValue::Int(1)),
        ]),
    );
    let b = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), GValue::String("b".into())),
            ("age".into(), GValue::Int(2)),
        ]),
    );
    let c = graph.insert_node(
        "person",
        BTreeMap::from([("name".into(), GValue::String("c".into()))]),
    );
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    graph.insert_edge("knows", &a, &c, BTreeMap::new()).unwrap();
    e.replace_graph(graph).unwrap();
    let rows = native(
        &mut e,
        "g.V().has('name','a').as('a').out().as('b').dedup('a','b').by('age').values('name')",
    )
    .await;
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0][0]["value"], "b");
    let rows = native(
        &mut e,
        "g.withStrategies(new ProductiveByStrategy()).V().order().by('age').values('name')",
    )
    .await;
    assert_eq!(rows[0][0]["value"], "c");
}
