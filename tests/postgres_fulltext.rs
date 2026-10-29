//! PostgreSQL text-search domains stay in the source; no completion engine.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn request(query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
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

fn latest_request(limit: &str, constrained: bool) -> Value {
    let mut r = request("MATCH (d:Document)-[:LATEST]->(n:Document) RETURN d.id AS source,n.id AS target ORDER BY source");
    r["tables"].as_array_mut().unwrap().push(json!({"name":"links","source":"db","columns":[{"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"}]}));
    r["edges"] = json!([{"label":"NEXT","table":"links","id":"id","source":"src","target":"dst","source_label":"Document","target_label":"Document"}]);
    r["cypher_relationships"] = json!([{"name":"LATEST","source":"Document","target":"Document","cypher":format!("WITH source MATCH (source)-[:NEXT]->(target:Document) RETURN target ORDER BY target.id DESC {limit}"),"returns":{"target":"target","properties":{}}}]);
    if constrained {
        let facts = json!([
            {"name":"pk","fact":{"kind":"unique","columns":["id"],"nulls_equal":true},"evidence":{"kind":"enforced"}},
            {"name":"nonnull","fact":{"kind":"non_null","columns":["id"]},"evidence":{"kind":"enforced"}}
        ]);
        r["constraints"] = json!({"tables":{"documents":facts,"links":facts}});
    }
    r
}

#[tokio::test]
async fn postgres_top_one_uses_distinct_on_and_proven_source_keys() {
    let plan = compile(latest_request("LIMIT 1", true)).await.unwrap();
    let sql = source_sql(&plan);
    assert!(sql.contains("DISTINCT ON"), "{sql}");
    assert!(!sql.contains("__apply_corr_key_row"), "{sql}");
    assert!(!sql.contains("\"__apply_corr_key_0\" IS NULL"), "{sql}");
}

#[tokio::test]
async fn postgres_top_one_does_not_rewrite_offsets_or_larger_limits() {
    for slice in ["SKIP 1 LIMIT 1", "LIMIT 2"] {
        let plan = compile(latest_request(slice, true)).await.unwrap();
        assert!(!source_sql(&plan).contains("DISTINCT ON"), "{}", source_sql(&plan));
    }
}

#[tokio::test]
async fn unproved_source_uniqueness_keeps_occurrence_identity() {
    let plan = compile(latest_request("LIMIT 1", false)).await.unwrap();
    assert!(source_sql(&plan).contains("__apply_corr_key_row"));
}

#[tokio::test]
async fn constant_native_text_expressions_never_evaluate_locally() {
    let plan = compile(request("MATCH (d:Document) RETURN d.id AS id, text.rank(text.vector('some words','english'),text.queryFromVector(text.vector('words','english')),32) AS score")).await.unwrap();
    let sql = source_sql(&plan);
    assert!(sql.contains("to_tsvector"), "{sql}");
}

#[tokio::test]
async fn source_vector_query_is_shared_before_candidate_fanout() {
    let plan = compile(request("MATCH (d:Document), (q:Document) WHERE q.id=1 RETURN d.id AS id, text.rank(d.vector,text.queryFromVector(q.vector),32) AS score")).await.unwrap();
    let sql = source_sql(&plan);
    assert!(sql.contains("__w_sql_cte_source_scalar_"), "{sql}");
    assert!(sql.contains("MATERIALIZED"), "{sql}");
}

#[tokio::test]
async fn source_vector_without_fanout_does_not_materialize() {
    let plan = compile(request("MATCH (d:Document) RETURN d.id AS id, text.queryFromVector(d.vector) AS query")).await.unwrap();
    assert!(!source_sql(&plan).contains("__w_sql_cte_source_scalar_"));
}

#[tokio::test]
async fn untrusted_keys_do_not_remove_occurrence_identity() {
    let mut r = latest_request("LIMIT 1", true);
    for facts in r["constraints"]["tables"].as_object_mut().unwrap().values_mut() {
        for fact in facts.as_array_mut().unwrap() { fact["evidence"] = json!({"kind":"declared"}); }
    }
    let plan = compile(r).await.unwrap();
    assert!(source_sql(&plan).contains("__apply_corr_key_row"));
}

#[tokio::test]
async fn volatile_relationship_selection_retains_window_fallback() {
    let mut r = latest_request("LIMIT 1", true);
    r["cypher_relationships"][0]["cypher"] = json!("WITH source MATCH (source)-[:NEXT]->(target:Document) RETURN target ORDER BY rand() LIMIT 1");
    // Some catalog implementations reject volatile relationship definitions;
    // neither rejection nor the fallback may silently select DISTINCT ON.
    if let Ok(plan) = compile(r).await {
        assert!(!source_sql(&plan).contains("DISTINCT ON"));
    }
}

#[tokio::test]
async fn observing_computed_edge_identity_keeps_its_ordinal() {
    let mut r = latest_request("LIMIT 1", true);
    r["query"] = json!("MATCH (d:Document)-[edge:LATEST]->(n:Document) RETURN d.id AS source,n.id AS target,id(edge) AS edge_id");
    let plan = compile(r).await.unwrap();
    assert!(source_sql(&plan).contains("row_number()"), "{}", source_sql(&plan));
}
