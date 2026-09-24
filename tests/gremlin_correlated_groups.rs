#![cfg(feature = "duckdb")]
use new_graph::{
    engine::GraphEngine,
    ir::{catalog::PropertyGraph, value::Value as GValue},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

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
async fn correlated_group_dedup_and_fold_consume_whole_group() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject('a','b','a').as('member').group().by(__.constant('key')).by(__.select('member').dedup().order().fold())").await;
    assert_eq!(
        rows[0][0]["value"][0][1],
        json!({"type":"list","value":[{"type":"string","value":"a"},{"type":"string","value":"b"}]})
    );
}

#[tokio::test]
async fn nested_correlated_group_returns_map_and_keeps_labels() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject('a','b','a').as('member').group().by(__.constant('outer')).by(__.group().by(__.select('member')).by(__.count()))").await;
    let nested = &rows[0][0]["value"][0][1];
    assert_eq!(nested["type"], "map");
    let entries = nested["value"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry[0]["value"] == "a")
            .unwrap()[1],
        json!({"type":"long","value":2})
    );
}

#[tokio::test]
async fn match_reduction_keeps_start_binding_and_unifies_scalar_end() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.inject(1).match(__.as('a').count().as('b'),__.as('a').count().as('b')).select('a','b')",
    )
    .await;
    assert_eq!(rows.as_array().unwrap().len(), 1);
    let rows = native(&mut engine, "g.inject(1).match(__.as('a').count().as('b'),__.as('a').union(__.identity(),__.identity()).count().as('b'))").await;
    assert_eq!(rows, json!([]));
}

#[tokio::test]
async fn match_scalar_mean_is_visible_in_nested_where() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    let target = graph.insert_node(
        "song",
        BTreeMap::from([("name".into(), GValue::String("target".into()))]),
    );
    for weight in [1, 2, 6] {
        let source = graph.insert_node("song", BTreeMap::new());
        graph
            .insert_edge(
                "followedBy",
                &source,
                &target,
                BTreeMap::from([("weight".into(), GValue::Int(weight))]),
            )
            .unwrap();
    }
    engine.replace_graph(graph).unwrap();
    let rows = native(&mut engine, "g.V().match(__.as('a').has('name','target'),__.as('a').map(__.inE('followedBy').values('weight').mean()).as('b'),__.as('a').inE('followedBy').as('c'),__.as('c').filter(__.values('weight').where(P.gte('b'))).outV().as('d')).select('c').values('weight')").await;
    assert_eq!(rows.as_array().unwrap().len(), 1, "{rows}");
    assert_eq!(rows[0][0]["value"], 6);
}

#[tokio::test]
async fn group_sum_preserves_long_width_and_omits_unproductive_groups() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    for age in [29, 27] {
        graph.insert_node("person", BTreeMap::from([("age".into(), GValue::Int(age))]));
    }
    graph.insert_node("software", BTreeMap::new());
    engine.replace_graph(graph).unwrap();
    let rows = native(
        &mut engine,
        "g.V().group().by(T.label).by(__.values('age').sum())",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 1);
    assert_eq!(rows[0][0]["value"][0][1], json!({"type":"long","value":56}));
}

#[tokio::test]
async fn named_group_accumulates_across_repeat_and_survives_empty_output() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject('a','b').repeat(__.group('m').by(__.constant('key')).by(__.fold())).times(2).limit(0).cap('m')").await;
    assert_eq!(
        rows[0][0]["value"][0][1]["value"].as_array().unwrap().len(),
        4
    );
}

#[tokio::test]
async fn named_group_value_prefix_writes_once_across_repeated_reads() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(&mut engine, "g.inject('a','b').group('m').by(__.constant('key')).by(__.sideEffect(__.store('seen')).fold()).select('m').select('m').cap('seen')").await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 2, "{rows}");
}

#[tokio::test]
async fn named_group_unproductive_key_preserves_parent_traversers() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.inject(1,2).group('m').by(__.filter(__.is(1))).count()",
    )
    .await;
    assert_eq!(rows[0][0]["value"], 2);
    let rows = native(
        &mut engine,
        "g.inject(1,2).group('m').by(__.filter(__.is(1))).cap('m')",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn named_group_count_unproductive_key_preserves_parent_traversers() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.inject(1,2).groupCount('m').by(__.filter(__.is(1))).count()",
    )
    .await;
    assert_eq!(rows[0][0]["value"], 2);
    let rows = native(
        &mut engine,
        "g.inject(1,2).groupCount('m').by(__.filter(__.is(1))).cap('m')",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 1);
    assert_eq!(rows[0][0]["value"][0][1], json!({"type":"long","value":1}));
}

#[tokio::test]
async fn current_only_nested_group_reductions_preserve_duplicate_multiplicity() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    let a = graph.insert_node("person", BTreeMap::new());
    let b = graph.insert_node("software", BTreeMap::new());
    graph
        .insert_edge(
            "created",
            &a,
            &b,
            BTreeMap::from([("weight".into(), GValue::Int(3))]),
        )
        .unwrap();
    engine.replace_graph(graph).unwrap();
    for prefix in ["g", "g.withBulk(false)"] {
        let rows = native(&mut engine, &format!("{prefix}.inject(1,2,3).V().hasLabel('person').group().by(T.label).by(__.bothE().group().by(T.label).by(__.values('weight').sum()))")).await;
        assert_eq!(
            rows[0][0]["value"][0][1]["value"][0][1],
            json!({"type":"long","value":9})
        );
    }
    let named = native(&mut engine, "g.inject(1,2,3).V().hasLabel('person').group('m').by(T.label).by(__.bothE().group().by(T.label).by(__.values('weight').sum())).cap('m')").await;
    assert_eq!(
        named[0][0]["value"][0][1]["value"][0][0]["value"],
        "created"
    );
    assert_eq!(named[0][0]["value"][0][1]["value"][0][1]["value"], 9);
    let rows = native(
        &mut engine,
        "g.inject(2,1,2).group().by(__.constant('key')).by(__.fold())",
    )
    .await;
    assert_eq!(
        rows[0][0]["value"][0][1]["value"],
        json!([{"type":"int","value":2},{"type":"int","value":1},{"type":"int","value":2}])
    );
}

#[tokio::test]
async fn named_group_count_after_short_repeat_preserves_walk_bulk() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    let a = graph.insert_node(
        "song",
        BTreeMap::from([("kind".into(), GValue::String("a".into()))]),
    );
    let b = graph.insert_node(
        "song",
        BTreeMap::from([("kind".into(), GValue::String("b".into()))]),
    );
    for _ in 0..2 {
        graph.insert_edge("next", &a, &b, BTreeMap::new()).unwrap();
    }
    engine.replace_graph(graph).unwrap();
    for prefix in ["g", "g.withBulk(false)"] {
        let rows = native(&mut engine, &format!("{prefix}.V().repeat(__.both('next')).times(2).group('m').by('kind').by(__.count()).cap('m')")).await;
        let entries = rows[0][0]["value"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        for entry in entries {
            assert_eq!(entry[1], json!({"type":"long","value":4}));
        }
    }
}
