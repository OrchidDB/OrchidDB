//! PostgreSQL text-search domains stay in the source; no completion engine.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn request(query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","engine":"none","language":"cypher","query":query,
        "sources":{"db":{"adapter":"postgres"}},
        "tables":[{"name":"documents","source":"db","columns":[
            {"name":"id","data_type":"int64"},{"name":"body","data_type":"string"},
            {"name":"vector","data_type":"domain:tsvector:string"}]}],
        "nodes":[{"label":"Document","table":"documents","id":"id","properties":{"id":"id","body":"body","vector":"vector"}}]})
}
async fn compile(r: Value) -> Result<Value, String> {
    Ok(serde_json::from_str(&compile_json(&r.to_string()).await?).unwrap())
}
fn source_sql(plan: &Value) -> &str {
    let nodes = plan["execution_plan"]["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1, "{plan}");
    assert_eq!(nodes[0]["kind"], "source_sql");
    assert_eq!(nodes[0]["dialect"], "postgres");
    nodes[0]["sql"].as_str().unwrap()
}
#[tokio::test]
async fn native_vector_rank_pushes_to_one_postgres_statement() {
    let plan = compile(request("MATCH (d:Document), (q:Document) WHERE q.id=1 RETURN d.id AS id, text.rank(d.vector,text.queryFromVector(q.vector),32) AS score ORDER BY score DESC LIMIT 5")).await.unwrap();
    let sql = source_sql(&plan).to_lowercase();
    assert!(sql.contains("ts_rank("), "{sql}");
    assert!(sql.contains("tsvector_to_array("), "{sql}");
    assert!(sql.contains("string_agg("), "{sql}");
    assert!(!sql.contains("as varchar"), "{sql}");
}
#[tokio::test]
async fn parsing_uses_explicit_postgres_dictionary() {
    let plan = compile(request("MATCH (d:Document) RETURN text.rank(text.vector(d.body,'english'),text.query('search','english'),32) AS score")).await.unwrap();
    let sql = source_sql(&plan).to_lowercase();
    assert!(sql.contains("to_tsvector("), "{sql}");
    assert!(sql.contains("plainto_tsquery("), "{sql}");
    assert!(sql.contains("regconfig"), "{sql}");
}
#[tokio::test]
async fn declared_domain_argument_cast_uses_source_dialect() {
    let mut r = request("MATCH (d:Document) RETURN custom.query(d.vector) AS query");
    r["functions"] = json!([{"name":"custom.query","target":"custom_query","parameters":["domain:tsvector:string"],"returns":"domain:tsquery:string"}]);
    let plan = compile(r).await.unwrap();
    let sql = source_sql(&plan).to_lowercase();
    assert!(sql.contains("as tsvector"), "{sql}");
    assert!(!sql.contains("orchid:type:"), "{sql}");
}
#[tokio::test]
async fn declared_string_function_accepts_arrow_string_view() {
    let mut r = request("MATCH (d:Document) RETURN custom.length(d.body) AS n");
    r["functions"] = json!([{"name":"custom.length","target":"custom_text_length","parameters":["string"],"returns":"int64"}]);
    let plan = compile(r).await.unwrap();
    let sql = source_sql(&plan).to_lowercase();
    assert!(sql.contains("custom_text_length"), "{sql}");
}
#[tokio::test]
async fn fulltext_rejects_wrong_domain_and_unsupported_source() {
    assert!(
        compile(request(
            "MATCH (d:Document) RETURN text.rank(d.body,d.body,32) AS score"
        ))
        .await
        .is_err()
    );
    let mut r = request(
        "MATCH (d:Document) RETURN text.rank(d.vector,text.queryFromVector(d.vector),32) AS score",
    );
    r["sources"]["db"]["adapter"] = json!("duckdb");
    assert!(compile(r).await.is_err());
}

#[tokio::test]
async fn computed_edge_keeps_source_scope_across_multiple_matches() {
    let mut r = request(
        "MATCH (d:Document)-[:LATEST]->(n:Document) RETURN d.id AS source,n.id AS target ORDER BY source",
    );
    r["tables"].as_array_mut().unwrap().push(json!({"name":"links","source":"db","columns":[{"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"}]}));
    r["edges"] = json!([{"label":"NEXT","table":"links","id":"id","source":"src","target":"dst","source_label":"Document","target_label":"Document"}]);
    r["cypher_relationships"] = json!([{
        "name":"LATEST", "source":"Document", "target":"Document",
        "cypher":"WITH source MATCH (source)-[:NEXT]->(middle:Document) MATCH (middle)-[:NEXT]->(target:Document) RETURN target ORDER BY target.id DESC LIMIT 1",
        "returns":{"target":"target","properties":{}}
    }]);
    let plan = compile(r).await.unwrap();
    assert!(source_sql(&plan).contains("links"));
}
