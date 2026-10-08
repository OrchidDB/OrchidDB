use super::{CompileRequest, CompiledSql, compile};
use arrow::datatypes::DataType;
use datafusion::common::ScalarValue;
use serde_json::{Value, json};

fn request(engine: &str, metric: &str) -> CompileRequest {
    let (function, comparison, direction) = match metric {
        "cosine" => ("vector.cosine_similarity", ">=", "DESC"),
        "dot" => ("vector.dot", ">=", "DESC"),
        "l2" => ("vector.l2_distance", "<=", "ASC"),
        _ => unreachable!(),
    };
    let dialect = if engine == "weaviate" {
        "duckdb"
    } else {
        engine
    };
    let mut schema = json!({
        "version": 1, "language": "cypher", "dialect": dialect,
        "query": "", "parameters": {"score": 0.7, "limit": 3},
        "engines": {"local": {"dialect": dialect}}, "execution_engine": "local",
        "tables": [
            {"name": "questions", "engine": "local", "columns": [
                {"name": "id", "data_type": "int64"},
                {"name": "tenant_id", "data_type": "string"},
                {"name": "embedding", "data_type": "list:float64"}
            ]},
            {"name": "documents", "engine": "local", "columns": [
                {"name": "id", "data_type": "int64"},
                {"name": "tenant_id", "data_type": "string"},
                {"name": "embedding", "data_type": "list:float64"}
            ]}
        ],
        "nodes": [
            {"label": "Question", "table": "questions", "id": "id",
             "properties": {"id": "id", "tenant_id": "tenant_id", "embedding": "embedding"}},
            {"label": "Document", "table": "documents", "id": "id",
             "properties": {"id": "id", "tenant_id": "tenant_id", "embedding": "embedding"}}
        ],
        "cypher_relationships": [{
            "name": "RELEVANT_TO", "source": "Question", "target": "Document",
            "parameters": [
                {"name": "score", "schema": {"type": "number"}, "default": 0.1},
                {"name": "limit", "schema": {"type": "integer", "minimum": 1}, "default": 9}
            ],
            "cypher": format!(
                "WITH source MATCH (target:Document) WHERE target.tenant_id = source.tenant_id \
                 WITH target, {function}(source.embedding, target.embedding) AS score \
                 WHERE score {comparison} $score RETURN target, score ORDER BY score {direction} LIMIT $limit"
            ),
            "returns": {"target": "target", "properties": {"score": {"type": "number"}}}
        }]
    });
    if engine != "duckdb" {
        let mut options = json!({"retrieval": "approximate_allowed"});
        let format = if engine == "weaviate" {
            schema["engines"]["vectors"] = json!({"dialect": "weaviate"});
            options["engine"] = json!("vectors");
            options["collection"] = json!("Documents");
            options["key_field"] = json!("doc_id");
            "weaviate"
        } else {
            "pgvector"
        };
        schema["source_metadata"] = json!([{
            "table": "documents", "format": format, "options": options,
            "indexes": [{"column": "embedding", "metric": metric}]
        }]);
    }
    let mut request: CompileRequest = serde_json::from_value(schema).unwrap();
    request.query =
        "MATCH (q:Question)-[r:RELEVANT_TO {score: $score, limit: $limit}]->(d:Document) \
                     RETURN q.id AS question, d.id AS document, r.score AS score"
            .into();
    request
}

async fn lower(engine: &str, metric: &str) -> CompiledSql {
    let result = compile(request(engine, metric)).await.unwrap();
    assert_eq!(result.fields, ["question", "document", "score"]);
    assert_eq!(result.execution_engine.as_deref(), Some("local"));
    assert!(
        !result.sql.contains("__orchiddb_logical_"),
        "{}",
        result.sql
    );
    result
}

async fn assert_sql(engine: &str, metric: &str, expected: &str) {
    let result = lower(engine, metric).await;
    assert!(result.transfers.is_empty(), "{:?}", result.transfers);
    assert!(result.sql.contains(expected), "{}", result.sql);
    assert!(result.sql.contains("0.7"), "{}", result.sql);
    assert!(result.sql.contains("tenant_id"), "{}", result.sql);
    if engine == "postgres" {
        assert!(result.sql.contains("LATERAL"), "{}", result.sql);
        assert!(result.sql.contains("LIMIT 3"), "{}", result.sql);
        assert!(result.sql.contains("ASC NULLS LAST"), "{}", result.sql);
    } else {
        assert!(
            result.sql.to_lowercase().contains("row_number"),
            "{}",
            result.sql
        );
        assert!(result.sql.contains("<= 3"), "{}", result.sql);
    }
}

async fn assert_weaviate(metric: &str) {
    let result = lower("weaviate", metric).await;
    let requests = result
        .transfers
        .iter()
        .filter_map(|transfer| transfer.request.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    let operation = requests[0];
    assert_eq!(operation.engine, "vectors");
    let template = &operation.template;
    template.validate().unwrap();
    assert_eq!(template.adapter, "weaviate");
    let payload = &template.request;
    assert_eq!(payload["api"], "weaviate");
    assert_eq!(payload["operation"], "search");
    assert_eq!(payload["index"], "Documents");
    assert_eq!(payload["metric"], metric);
    assert_eq!(payload["limit"], 3);
    assert_eq!(
        payload["body"]["score_filters"],
        json!([
            {"op": if metric == "l2" { "lte" } else { "gte" }, "value": 0.7}
        ])
    );
    assert!(
        template
            .bindings
            .iter()
            .any(|binding| binding.pointer == "/body/vector")
    );
    assert!(payload["body"]["query"].to_string().contains("tenant_id"));
    assert!(
        payload["columns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|column| column["path"] == json!(["doc_id"]))
    );
    assert!(result.sql.contains("documents"), "{}", result.sql);
    let transfer = result
        .transfers
        .iter()
        .find(|transfer| transfer.request.is_some())
        .unwrap();
    assert!(
        result.sql.contains(&transfer.target_relation),
        "{}",
        result.sql
    );
    assert!(
        result
            .sql
            .contains("\"__relationship_search_score\" AS \"__relationship_edge_73636f7265\""),
        "{}",
        result.sql
    );
    assert!(
        payload["columns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|column| column["source"] == "score")
    );
    let arguments = operation
        .input_columns
        .iter()
        .map(|column| match column.data_type.as_str() {
            "int64" => ScalarValue::Int64(Some(10)),
            "string" => ScalarValue::Utf8(Some("tenant's data".into())),
            "list:float64" => ScalarValue::List(ScalarValue::new_list(
                &[
                    ScalarValue::Float64(Some(1.0)),
                    ScalarValue::Float64(Some(0.5)),
                ],
                &DataType::Float64,
                true,
            )),
            other => panic!("unexpected request input type: {other}"),
        })
        .collect::<Vec<_>>();
    let bound = template.bind(&arguments).unwrap();
    assert_eq!(bound["body"]["vector"], json!([1.0, 0.5]));
    assert!(bound["body"]["query"].to_string().contains("tenant's data"));
}

#[tokio::test]
async fn cosine_lowers_to_duckdb() {
    assert_sql("duckdb", "cosine", "list_cosine_similarity").await;
}
#[tokio::test]
async fn dot_lowers_to_duckdb() {
    assert_sql("duckdb", "dot", "list_inner_product").await;
}
#[tokio::test]
async fn l2_lowers_to_duckdb() {
    assert_sql("duckdb", "l2", "list_distance").await;
}
#[tokio::test]
async fn cosine_lowers_to_postgres() {
    assert_sql("postgres", "cosine", "<=>").await;
}
#[tokio::test]
async fn dot_lowers_to_postgres() {
    assert_sql("postgres", "dot", "<#>").await;
}
#[tokio::test]
async fn l2_lowers_to_postgres() {
    assert_sql("postgres", "l2", "<->").await;
}
#[tokio::test]
async fn cosine_lowers_to_weaviate() {
    assert_weaviate("cosine").await;
}
#[tokio::test]
async fn dot_lowers_to_weaviate() {
    assert_weaviate("dot").await;
}
#[tokio::test]
async fn l2_lowers_to_weaviate() {
    assert_weaviate("l2").await;
}

#[tokio::test]
async fn weaviate_rejects_exact_vector_retrieval() {
    for metric in ["cosine", "dot", "l2"] {
        let mut request = request("weaviate", metric);
        request.source_metadata[0]
            .options
            .insert("retrieval".into(), Value::String("exact".into()));
        let error = compile(request).await.unwrap_err();
        assert!(error.contains("requires approximate_allowed"), "{error}");
    }
}
