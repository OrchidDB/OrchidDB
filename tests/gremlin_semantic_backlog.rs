#![cfg(feature = "duckdb")]

use new_graph::engine::GraphEngine;
use serde_json::{Value, json};

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

fn entry<'a>(map: &'a Value, key: Value) -> &'a Value {
    &map["value"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry[0] == key)
        .unwrap_or_else(|| panic!("missing {key} in {map}"))[1]
}

#[tokio::test]
async fn project_cycles_modulators_and_resets_for_each_traverser() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.inject(1,2).project('a','b','c').by(__.constant(7))",
    )
    .await;
    for row in rows.as_array().unwrap() {
        for key in ["a", "b", "c"] {
            assert_eq!(
                entry(&row[0], json!({"type":"string","value":key})),
                &json!({"type":"int","value":7})
            );
        }
    }
    let rows = native(
        &mut engine,
        "g.inject(1,2).project('a','b','c').by().by(__.constant(9))",
    )
    .await;
    for (index, row) in rows.as_array().unwrap().iter().enumerate() {
        for key in ["a", "c"] {
            assert_eq!(
                entry(&row[0], json!({"type":"string","value":key}))["value"],
                index + 1
            );
        }
        assert_eq!(
            entry(&row[0], json!({"type":"string","value":"b"}))["value"],
            9
        );
    }
    // Extra modulators belong to project even if this row has fewer keys.
    let rows = native(
        &mut engine,
        "g.inject(1).project('a').by(__.constant(7)).by(__.constant(9))",
    )
    .await;
    assert_eq!(
        entry(&rows[0][0], json!({"type":"string","value":"a"}))["value"],
        7
    );
}

#[tokio::test]
async fn cyclic_project_preserves_null_and_unproductive_children() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.inject(1).project('a','b','c','d').by(__.constant(null)).by(__.filter(__.is(2)))",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 2);
    for key in ["a", "c"] {
        assert_eq!(
            entry(&rows[0][0], json!({"type":"string","value":key})),
            &json!({"type":"null","value":null})
        );
    }
}

#[tokio::test]
async fn seeded_group_count_retains_typed_keys_and_long_values() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.withSideEffect('x',[(1):10L,(9):4L]).inject(1,1,2).groupCount('x').cap('x')",
    )
    .await;
    for (key, count) in [(1, 12), (2, 1), (9, 4)] {
        assert_eq!(
            entry(&rows[0][0], json!({"type":"int","value":key})),
            &json!({"type":"long","value":count})
        );
    }
    let rows = native(
        &mut engine,
        "g.withSideEffect('x',['old':3L]).inject('new').groupCount('x').cap('x')",
    )
    .await;
    assert_eq!(
        entry(&rows[0][0], json!({"type":"string","value":"old"}))["value"],
        3
    );
    assert_eq!(
        entry(&rows[0][0], json!({"type":"string","value":"new"}))["value"],
        1
    );
    let rows = native(
        &mut engine,
        "g.withSideEffect('x',[(1):10L]).inject(1).groupCount('x').select('x')",
    )
    .await;
    assert_eq!(
        entry(&rows[0][0], json!({"type":"int","value":1}))["value"],
        11
    );
}

#[tokio::test]
async fn seeded_group_count_handles_empty_input_and_repeated_publication() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.withSideEffect('x',[(1):10L]).inject(1).filter(__.is(2)).groupCount('x').cap('x')",
    )
    .await;
    assert_eq!(
        entry(&rows[0][0], json!({"type":"int","value":1}))["value"],
        10
    );
    let rows = native(&mut engine, "g.withSideEffect('x',[(1):10L]).inject(1).repeat(__.groupCount('x').sideEffect(__.cap('x'))).times(3).cap('x').cap('x')").await;
    assert_eq!(
        entry(&rows[0][0], json!({"type":"int","value":1})),
        &json!({"type":"long","value":13})
    );
}

#[tokio::test]
async fn lazy_aggregate_consumes_through_filters_projections_and_ranges() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        (
            "g.inject(1,2,3,4,5).aggregate(local,'x').is(P.gt(2)).constant(9).limit(1).cap('x')",
            vec![1, 2, 3, 4],
        ),
        (
            "g.inject(1,2,3,4,5).store('x').filter(__.is(P.gt(1))).range(1,2).cap('x')",
            vec![1, 2, 3, 4],
        ),
        (
            "g.inject(1,2,3).aggregate(local,'x').flatMap(__.union(__.identity(),__.identity())).limit(3).cap('x')",
            vec![1, 2],
        ),
        (
            "g.inject(1,2,3,4).store('x').choose(__.is(1),__.constant(9),__.identity()).limit(1).cap('x')",
            vec![1, 2],
        ),
        (
            "g.inject(1,2,3).aggregate(local,'x').is(P.gt(5)).limit(1).cap('x')",
            vec![1, 2, 3],
        ),
        (
            "g.inject(1,2,3).aggregate(local,'x').limit(0).cap('x')",
            vec![],
        ),
    ] {
        let rows = native(&mut engine, query).await;
        let actual: Vec<i64> = rows[0][0]["value"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["value"].as_i64().unwrap())
            .collect();
        assert_eq!(actual, expected, "{query}");
    }
}

#[tokio::test]
async fn lazy_pipeline_preserves_barriers_and_executes_only_consumed_writes() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let rows = native(
        &mut engine,
        "g.inject(1,2,3).aggregate('x').is(P.gt(1)).limit(1).cap('x')",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 3);
    let rows = native(
        &mut engine,
        "g.inject(3,2,1).aggregate(local,'x').order().limit(1).cap('x')",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 3);
    let rows = native(
        &mut engine,
        "g.inject(1,2,3).aggregate(local,'x').addV('consumed').limit(1).cap('x')",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 1);
    assert_eq!(
        native(&mut engine, "g.V().hasLabel('consumed').count()").await[0][0]["value"],
        1
    );
    let rows = native(
        &mut engine,
        "g.inject(1).local(__.inject(2,3,4).store('x').is(P.gt(2)).limit(1)).cap('x')",
    )
    .await;
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 4);
}
