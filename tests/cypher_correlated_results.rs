#![cfg(feature = "duckdb")]
use orchiddb::engine::{GraphEngine, QueryResult};
use serde_json::{Value, json};
fn values(result: &QueryResult) -> Vec<Value> {
    fn value(v: &Value) -> Value {
        match v["type"].as_str().unwrap() {
            "list" => json!(
                v["value"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(value)
                    .collect::<Vec<_>>()
            ),
            "null" => Value::Null,
            _ => v["value"].clone(),
        }
    }
    let native: Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"],
    )
    .unwrap();
    let mut rows = native
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            json!(
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(value)
                    .collect::<Vec<_>>()
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(Value::to_string);
    rows
}
#[tokio::test]
async fn pattern_comprehension_count_preserves_apply_output_and_duplicates() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (x:X), (x)-[:T]->(), (x)-[:T]->(), (x)-[:T]->(), (x)-[:OTHER]->()")
        .await
        .unwrap();
    for (query, expected) in [
        (
            "MATCH (a:X) RETURN size([(a)-->() | 1]) AS length",
            vec![json!([4])],
        ),
        (
            "MATCH (a:X) RETURN size([(a)-[:T]->() | 1]) AS length",
            vec![json!([3])],
        ),
        (
            "MATCH (a:X) RETURN size([(a)-[:T|OTHER]->() | 1]) AS length",
            vec![json!([4])],
        ),
        (
            "UNWIND [1,1] AS i MATCH (a:X) RETURN i,size([(a)-[:T]->() | 1]) AS length",
            vec![json!([1, 3]), json!([1, 3])],
        ),
    ] {
        let result = engine.cypher(query).await.unwrap();
        assert_eq!(
            values(&result),
            expected,
            "{query}\n{}",
            result.stats.physical_plan
        );
    }
}
#[tokio::test]
async fn correlated_node_and_edge_collection_keep_null_elements_and_empty_lists() {
    for (setup, query) in [
        (
            "CREATE (a), (b {name:'val'}), (c), (a)-[:T]->(b), (b)-[:T]->(c)",
            "MATCH(n) RETURN [(n)-[:T]->(b)|b.name] AS list",
        ),
        (
            "CREATE (a), (b), (c), (a)-[:T {name:'val'}]->(b), (b)-[:T]->(c)",
            "MATCH(n) RETURN [(n)-[r:T]->()|r.name] AS list",
        ),
    ] {
        let mut engine = GraphEngine::in_memory().unwrap();
        engine.cypher(setup).await.unwrap();
        let result = engine.cypher(query).await.unwrap();
        let mut expected = vec![json!([["val"]]), json!([[null]]), json!([[]])];
        expected.sort_by_key(Value::to_string);
        assert_eq!(values(&result), expected, "{query}");
    }
}
#[tokio::test]
async fn correlated_comprehension_boolean_keeps_the_empty_input_row() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (a:X {num:42}),(:X {num:43}),(a)-[:T]->()")
        .await
        .unwrap();
    let result = engine
        .cypher("MATCH(n:X) RETURN n.num,size([(n)--()|1])>0 AS connected")
        .await
        .unwrap();
    assert_eq!(values(&result), vec![json!([42, true]), json!([43, false])]);
}
#[tokio::test]
async fn correlated_degree_does_not_hide_invalid_aggregate_argument() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("UNWIND range(0,10) AS i CREATE(s:S) WITH s,i UNWIND range(0,i) AS j CREATE(s)-[:REL]->()").await.unwrap();
    let error=engine.cypher("MATCH(n:S) WITH n,size([(n)-->()|1]) AS deg WHERE deg>2 WITH deg LIMIT 100 RETURN percentileDisc(0.90,deg),deg").await.err().expect("invalid percentile must be evaluated for surviving degree rows");
    assert!(error.to_ascii_lowercase().contains("percentile"), "{error}");
}
