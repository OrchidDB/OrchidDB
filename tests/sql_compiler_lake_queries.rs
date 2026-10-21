//! Regression coverage for lake queries with Orchid runtime helpers available.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn request(query: &str) -> Value {
    json!({"native_values":true,"version":1,"dialect":"duckdb","language":"cypher","query":query,
        "tables":[{"name":"artifacts","columns":[
            {"name":"id","data_type":"int64"},{"name":"kind","data_type":"string"},
            {"name":"bytes","data_type":"int64"},{"name":"run_id","data_type":"int64"}]},
            {"name":"runs","columns":[{"name":"id","data_type":"int64"}]}],
        "nodes":[{"label":"Artifact","table":"artifacts","id":"id","properties":{"id":"id","kind":"kind","bytes":"bytes"}},
            {"label":"Run","table":"runs","id":"id","properties":{"id":"id"}}],
        "edges":[{"label":"RUN","table":"artifacts","id":"id","source_label":"Artifact","source":"id","target_label":"Run","target":"run_id"}]})
}
async fn compile(query: &str) -> Value {
    serde_json::from_str(&compile_json(&request(query).to_string()).await
        .unwrap_or_else(|e| panic!("{query}: {e}"))).unwrap()
}

#[tokio::test]
async fn collections_and_aggregate_filters_compile_with_runtime_helpers() {
    for query in [
        "MATCH (a:Artifact)-[:RUN]->(r:Run) WITH a.kind AS kind, count(a) AS artifacts, sum(a.bytes) AS bytes, count(DISTINCT r.id) AS runs WHERE artifacts > 1 RETURN kind, artifacts, bytes, runs ORDER BY bytes DESC LIMIT 5",
        "MATCH (a:Artifact) WITH a.kind AS kind, count(a) AS n WITH collect({kind: kind, n: n}) AS groups UNWIND groups AS g RETURN g.kind AS kind, g.n AS count ORDER BY count DESC",
        "MATCH (a:Artifact) WITH collect(DISTINCT a.kind) AS kinds UNWIND kinds AS kind RETURN kind ORDER BY kind",
    ] {
        let plan = compile(query).await;
        assert!(plan["sql"].as_str().is_some(), "{plan}");
    }
}

#[tokio::test]
async fn correlated_call_compiles_and_preserves_outer_limit() {
    let plan = compile("MATCH (r:Run) WITH r ORDER BY r.id LIMIT 5 CALL { WITH r MATCH (a:Artifact)-[:RUN]->(r) RETURN count(a) AS artifacts, sum(a.bytes) AS bytes } RETURN r.id AS id, artifacts, bytes ORDER BY id").await;
    assert_eq!(plan["fields"], json!(["id", "artifacts", "bytes"]));
    assert!(plan["sql"].as_str().unwrap().contains("LIMIT 5"));
}

#[tokio::test]
async fn call_scope_parameters_and_comments() {
    for query in [
        "UNWIND [1,1,2] AS x CALL { WITH x RETURN x + 1 AS y } RETURN x,y",
        "CALL { RETURN 1 AS x } RETURN x",
        "WITH 2 AS x CALL { WITH x // keep newline\n RETURN x AS y } RETURN y",
        "WITH 'λ' AS x CALL { WITH x RETURN x AS y } RETURN y",
        "WITH 2 AS x CALL { WITH x CALL { WITH x RETURN x+1 AS y } RETURN y+1 AS z } RETURN z",
    ] { compile(query).await; }
    let mut r = request("WITH $n AS x CALL { WITH x RETURN x+1 AS y } RETURN y");
    r["parameters"] = json!({"n": 2});
    compile_json(&r.to_string()).await.unwrap();
    for query in [
        "WITH 1 AS x CALL { RETURN x AS y } RETURN y",
        "WITH 1 AS x CALL { WITH absent RETURN absent AS y } RETURN y",
        "WITH 1 AS x CALL { RETURN 2 AS x } RETURN x",
        "CALL { WITH 1 AS x RETURN x } RETURN x",
    ] { assert!(compile_json(&request(query).to_string()).await.is_err(), "{query}"); }
}
