//! SQL-only boundary regression tests: run without the duckdb feature.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};
fn request(query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
        "tables":[{"name":"\"Graph Data\".\"People\"","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"\"Graph Data\".\"People\"","id":"id","properties":{"name":"name"}}]})
}
async fn compile(r: Value) -> Result<Value, String> {
    Ok(serde_json::from_str(&compile_json(&r.to_string()).await?).unwrap())
}
#[tokio::test]
async fn compiles_qualified_external_table_without_materializing() {
    let r = compile(request("MATCH (p:Person) RETURN p.name AS name"))
        .await
        .unwrap();
    assert!(
        r["sql"]
            .as_str()
            .unwrap()
            .contains("\"Graph Data\".\"People\"")
    );
    assert_eq!(r["fields"], json!(["name"]));
}
#[tokio::test]
async fn constant_query_needs_no_internal_table() {
    let r = compile(request("RETURN 42 AS answer")).await.unwrap();
    assert!(r["sql"].as_str().unwrap().contains("42"));
    assert!(!r["sql"].as_str().unwrap().contains("one_row_"));
}
#[tokio::test]
async fn validates_protocol_dialect_and_schema() {
    let mut r = request("RETURN 1");
    r["version"] = json!(99);
    assert!(compile(r).await.unwrap_err().contains("version"));
    let mut r = request("RETURN 1");
    r["dialect"] = json!("clickhouse");
    assert!(compile(r).await.unwrap_err().contains("dialect"));
    let mut r = request("RETURN 1");
    r["nodes"][0]["id"] = json!("missing");
    assert!(compile(r).await.unwrap_err().contains("missing column"));
    let mut r = request("RETURN 1");
    r["tables"][0]["columns"][0]["data_type"] = json!("string");
    assert!(compile(r).await.unwrap_err().contains("signed integer"));
}
#[tokio::test]
async fn rejects_mutations_and_missing_bindings() {
    let e = compile(request("MATCH (p:Person) DELETE p"))
        .await
        .unwrap_err();
    assert!(e.contains("mutation"), "{e}");
    assert!(
        compile(request("MATCH (p:Person) WHERE p.name=$name RETURN p.name"))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn parameters_are_literals_and_oversized_integers_fail() {
    let mut r = request("MATCH (p:Person) WHERE p.name=$name RETURN p.name");
    r["parameters"] = json!({"name":"O'Reilly; DROP TABLE people; --"});
    let sql = compile(r).await.unwrap()["sql"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(sql.contains("O''Reilly; DROP TABLE people; --"), "{sql}");
    let mut r = request("RETURN $n");
    r["parameters"] = json!({"n":u64::MAX});
    assert!(compile(r).await.unwrap_err().contains("64-bit"));
}

#[tokio::test]
async fn membership_never_duplicates_a_computed_left_operand() {
    let mut r =
        request("MATCH (p:Person) WHERE externalName(p.name) IN ['Ada', 'Grace'] RETURN p.name");
    r["functions"] = json!([{"name":"externalName","target":"external_name","parameters":["string"],"returns":"string"}]);
    let error = compile(r).await.unwrap_err();
    assert!(error.contains("single-evaluation"), "{error}");
}

#[tokio::test]
async fn mapped_gremlin_ids_and_scalar_order_need_no_runtime_functions() {
    for query in ["g.V(1).values('name')", "g.V().order().by('name').values('name')"] {
        let mut r = request(query);
        r["language"] = json!("gremlin");
        let compiled = compile(r).await.unwrap();
        let sql = compiled["sql"].as_str().unwrap();
        assert!(!sql.contains("gremlin_id"), "{sql}");
        assert!(!sql.contains("gremlin_order_key"), "{sql}");
    }
}
