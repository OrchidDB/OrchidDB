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

#[tokio::test]
async fn string_steps_preserve_nulls_and_reject_non_strings() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("g.inject('ripple').substring(1,-1)", "ippl"),
        ("g.inject('ripple').substring(-4,2)", ""),
        ("g.inject('marko').substring(-3)", "rko"),
        ("g.inject('abc').substring(1,0)", ""),
        ("g.inject(null).concat('')", ""),
        ("g.inject('a').concat(__.inject('c'))", "aa"),
        ("g.inject('a').concat(__.inject(['b','c']))", "aa"),
        ("g.inject([3,'three']).conjoin(';')", "3;three"),
        ("g.inject('　abc　').lTrim()", "abc　"),
        ("g.inject('　abc　').rTrim()", "　abc"),
        (
            "g.inject('marko').as('a').constant('Mr.').concat(__.select('a'))",
            "Mr.marko",
        ),
    ] {
        assert_eq!(
            native(&mut engine, query).await[0][0]["value"],
            json!(expected),
            "{query}"
        );
    }
    for query in [
        "g.inject(null).concat()",
        "g.inject(null).toLower(Scope.local)",
    ] {
        assert_eq!(
            native(&mut engine, query).await[0][0]["type"],
            "null",
            "{query}"
        );
    }
    let values = native(
        &mut engine,
        "g.inject(['FEATURE','tESt',null]).toLower(Scope.local)",
    )
    .await;
    assert_eq!(values[0][0]["value"][0]["value"], "feature");
    assert_eq!(values[0][0]["value"][2]["type"], "null");
    for (query, error) in [
        (
            "g.inject(['a','b']).concat('x')",
            "String concat() can only take string as argument",
        ),
        (
            "g.inject(['a','b']).substring(1,2)",
            "substring() step can only take string",
        ),
        (
            "g.inject([1,2]).trim(Scope.local)",
            "trim(local) step can only take string or list of strings",
        ),
        (
            "g.inject(null).conjoin('x')",
            "Incoming traverser for conjoin step can't be null",
        ),
    ] {
        let actual = engine
            .gremlin(query)
            .await
            .err()
            .expect("invalid input must fail");
        assert!(actual.contains(error), "{query}: {actual}");
    }
}

#[tokio::test]
async fn date_and_iterable_arguments_preserve_types_and_errors() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "g.inject('2023-09-06T16:28:27Z').asDate()",
        "g.inject(1694017707000L).asDate()",
    ] {
        let value = native(&mut engine, query).await;
        assert_eq!(value[0][0]["type"], "datetime", "{query}");
        assert!(
            value[0][0]["value"]
                .as_str()
                .unwrap()
                .starts_with("2023-09-06T16:28:27")
        );
    }
    for query in [
        "g.inject(1B).asDate()",
        "g.inject(1S).asDate()",
        "g.inject(1N).asDate()",
    ] {
        let value = native(&mut engine, query).await;
        assert_eq!(value[0][0]["type"], "datetime");
    }
    for query in [
        "g.inject(null).asDate()",
        "g.inject(1694017709000.1D).asDate()",
        "g.inject('wrong').asDate()",
    ] {
        assert!(
            engine
                .gremlin(query)
                .await
                .err()
                .unwrap()
                .contains("Can't parse")
        );
    }
    for op in [
        "combine",
        "difference",
        "disjunct",
        "intersect",
        "product",
        "merge",
    ] {
        let query = format!("g.inject(null).{op}(__.inject(1))");
        let error = engine.gremlin(&query).await.err().unwrap();
        assert!(
            error.contains(&format!("Incoming traverser for {op} step can't be null")),
            "{error}"
        );

        let query = format!("g.inject([1,2]).{op}(__.constant(null))");
        let error = engine.gremlin(&query).await.err().unwrap();
        assert!(
            error.contains(&format!(
                "traversal argument for {op} step must yield an iterable type, not null"
            )),
            "{error}"
        );
    }
}

#[tokio::test]
async fn local_ranges_use_requested_width_and_keep_map_shape() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "g.inject([1,2,3]).tail(Scope.local,1)",
        "g.inject([1,2,3]).range(Scope.local,2,3)",
    ] {
        assert_eq!(
            native(&mut engine, query).await[0][0]["value"],
            3,
            "{query}"
        );
    }
    for query in [
        "g.inject([]).tail(Scope.local,1)",
        "g.inject([1,2,3]).range(Scope.local,3,4)",
    ] {
        let result = engine.gremlin(query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 0, "{query}");
    }
    for query in [
        "g.inject([1]).tail(Scope.local,2)",
        "g.inject([1]).range(Scope.local,0,2)",
    ] {
        let value = native(&mut engine, query).await;
        assert_eq!(value[0][0]["type"], "list");
        assert_eq!(value[0][0]["value"].as_array().unwrap().len(), 1);
    }
    let value = native(&mut engine, "g.inject(['a':1,'b':2]).tail(Scope.local,1)").await;
    assert_eq!(value[0][0]["type"], "map");
    assert_eq!(value[0][0]["value"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn collection_merge_and_reductions_follow_native_types() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "g.inject(null,10I,5I,null).sum()",
        "g.inject([10I,5I]).unfold().sum()",
    ] {
        let value = native(&mut engine, query).await;
        assert_eq!(value[0][0]["type"], "long", "{query}: {value}");
        assert_eq!(value[0][0]["value"], 15);
    }
    assert_eq!(
        native(&mut engine, "g.inject(null,null).sum()").await[0][0]["type"],
        "null"
    );
    assert_eq!(
        engine
            .gremlin("g.inject([]).unfold().sum()")
            .await
            .unwrap()
            .returned
            .batch
            .num_rows(),
        0
    );
    let value = native(&mut engine, "g.inject(29I).mean(Scope.local)").await;
    assert_eq!(value[0][0]["type"], "double");
    assert_eq!(value[0][0]["value"].as_str().unwrap().parse::<f64>().unwrap(), 29.0);
    let value = native(
        &mut engine,
        "g.inject([29,27,32,35]).intersect([27,29,32,35]).order(Scope.local)",
    )
    .await;
    assert_eq!(value[0][0]["value"][0]["value"], 27, "{value}");
    let entry = native(&mut engine, "g.inject(['a':1]).unfold().asString()").await;
    assert_eq!(entry[0][0]["value"], "a=1");
    let property = native(
        &mut engine,
        "g.addV('person').property('age',29).properties('age').asString()",
    )
    .await;
    assert_eq!(property[0][0]["value"], "vp[age->29]");
    let value = native(&mut engine, "g.inject([1,null]).asString(Scope.local)").await;
    assert_eq!(value[0][0]["value"][0]["value"], "1");
    assert_eq!(value[0][0]["value"][1]["type"], "null");
    let value = native(
        &mut engine,
        "g.inject(datetime('2023-08-02T00:00:00Z')).dateDiff(datetime('2023-08-09T00:00:00Z'))",
    )
    .await;
    assert_eq!(value[0][0]["value"], -604800);
    for (query, expected) in [
        (
            "g.inject(['a':1]).merge(__.constant('bad'))",
            "merge step expected provided argument to evaluate to a Map",
        ),
        (
            "g.inject([1]).merge(__.constant(['a':1]))",
            "traversal argument for merge step must yield an iterable type",
        ),
        (
            "g.inject([1]).merge(['a':1])",
            "step type mismatch: expected argument to be Iterable but got Map",
        ),
    ] {
        let error = engine.gremlin(query).await.err().unwrap();
        assert!(error.contains(expected), "{query}: {error}");
    }
    let value = native(&mut engine, "g.inject(['a':1,'b':2]).merge(['b':3,'c':4])").await;
    assert_eq!(value[0][0]["type"], "map");
    assert_eq!(value[0][0]["value"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn local_map_order_retains_entry_keys_and_values() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let value = native(&mut engine, "g.inject(['b':2,'a':1]).order(Scope.local).by(Column.keys).unfold()").await;
    assert_eq!(value.as_array().unwrap().len(),2,"{value}");
    assert_eq!(value[0][0]["type"],"entry", "{value}");
    assert_eq!(value[0][0]["value"][0]["value"],"a", "{value}");
    assert_eq!(value[0][0]["value"][1]["value"],1,"{value}");
}
