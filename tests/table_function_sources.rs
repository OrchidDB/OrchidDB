//! Abstract table-function declarations, input binding, and result filtering.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn request(function: &str, query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
        "tables":[{"name":"samples","columns":[
            {"name":"id","data_type":"string"},
            {"name":"query","data_type":"string"},
            {"name":"mode","data_type":"string"},
            {"name":"value","data_type":"float64"}
        ]}],
        "nodes":[{"label":"Sample","table":"samples","id":"id","properties":{
            "query":"query","mode":"mode","value":"value"
        }}],
        "source_metadata":[{"table":"samples","format":"table_function","options":{
            "function":[function],"arguments":[
                {"column":"query"},
                {"column":"mode","name":"mode","value":"standard"}
            ]
        }}]
    })
}
async fn sql(request: Value) -> String {
    let result: Value =
        serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap();
    result["sql"].as_str().unwrap().to_owned()
}
#[tokio::test]
async fn native_expressions_are_explicit_inputs_and_result_filters_remain() {
    for function in ["rows_for_query", "lookup_records", "arbitrary_rows"] {
        let sql = sql(request(function,
            "MATCH (s:Sample) WHERE s.query = 'search text' AND s.value > 3 RETURN s.value AS value"
        )).await;
        assert!(sql.contains(function), "{sql}");
        assert!(sql.contains("search text"), "{sql}");
        assert!(
            sql.contains("mode :=") || sql.contains("\"mode\" :="),
            "{sql}"
        );
        assert!(sql.contains("> 3"), "{sql}");
    }
}
#[tokio::test]
async fn required_inputs_reject_ambiguous_predicates() {
    for predicate in [
        "s.value > 0",
        "s.query <> 'up'",
        "s.query IS NULL",
        "s.query = 'up' OR s.query = 'down'",
    ] {
        let request = request(
            "rows_for_query",
            &format!("MATCH (s:Sample) WHERE {predicate} RETURN s.value"),
        );
        let error = compile_json(&request.to_string()).await.unwrap_err();
        assert!(
            error.to_string().contains("required argument query"),
            "{error}"
        );
    }
}
#[tokio::test]
async fn defaults_optional_arguments_and_reversed_equality() {
    let mut req = request(
        "rows_for_query",
        "MATCH (s:Sample) WHERE 'up' = s.query RETURN s.value",
    );
    req["source_metadata"][0]["options"]["arguments"][1] =
        json!({"column":"mode","name":"mode","optional":true});
    let rendered = sql(req).await;
    assert!(rendered.contains("'up'"), "{rendered}");
    assert!(!rendered.contains(":="), "{rendered}");
    let mut req = request(
        "rows_for_query",
        "MATCH (s:Sample) WHERE s.query = 'different' RETURN s.value",
    );
    req["source_metadata"][0]["options"]["arguments"][0]["value"] = json!("up");
    let rendered = sql(req).await;
    assert!(rendered.contains("'up'"), "{rendered}");
    assert!(rendered.contains("'different'"), "{rendered}");
}
#[tokio::test]
async fn contradictory_inputs_fail_closed_when_optimizer_removes_bindings() {
    let req = request(
        "rows_for_query",
        "MATCH (s:Sample) WHERE s.query = 'up' AND s.query = 'down' RETURN s.value",
    );
    let error = compile_json(&req.to_string()).await.unwrap_err();
    assert!(error.contains("required argument query"), "{error}");
}

#[tokio::test]
async fn postgres_uses_the_same_declaration() {
    let mut req = request(
        "rows_for_query",
        "MATCH (s:Sample) WHERE s.query = 'up' RETURN s.value",
    );
    req["dialect"] = json!("postgres");
    let rendered = sql(req).await;
    assert!(
        rendered.contains("rows_for_query") && rendered.contains("'up'"),
        "{rendered}"
    );
}

#[tokio::test]
async fn declared_function_executes_with_residual_filters() {
    // Exercise actual SQL binding and result semantics with a local table macro.
    if std::process::Command::new("duckdb")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let rendered = sql(request(
        "rows_for_query",
        "MATCH (s:Sample) WHERE s.query = 'up' AND s.value > 3 RETURN s.value AS value",
    ))
    .await;
    let setup = "CREATE MACRO rows_for_query(q, mode := 'default') AS TABLE SELECT 'a' AS id, 1.0 AS value UNION ALL SELECT 'b', 5.0;";
    let output = std::process::Command::new("duckdb")
        .args(["-json", ":memory:", &format!("{setup} {rendered}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{rendered}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result, json!([{"value":5.0}]));
}

#[tokio::test]
async fn parameters_and_independent_scan_bindings() {
    let mut req = request(
        "rows_for_query",
        "MATCH (s:Sample) WHERE s.query = $query RETURN s.value",
    );
    req["parameters"] = json!({"query":"O'Reilly"});
    let rendered = sql(req).await;
    assert!(rendered.contains("O''Reilly"), "{rendered}");
    let rendered = sql(request("rows_for_query", "MATCH (a:Sample), (b:Sample) WHERE a.query = 'first' AND b.query = 'second' RETURN a.value, b.value")).await;
    assert!(
        rendered.contains("'first'") && rendered.contains("'second'"),
        "{rendered}"
    );
}

#[tokio::test]
async fn invalid_declarations_are_rejected() {
    for arguments in [
        json!([{"column":"missing"}]),
        json!([{"column":"query"}, {"column":"query"}]),
        json!([{"column":"query","optional":true}]),
        json!([{"column":"query","name":"x"}, {"column":"mode"}]),
        json!([{"column":"query","value":{"sql":"untrusted"}}]),
    ] {
        let mut req = request("rows_for_query", "MATCH (s:Sample) RETURN s.value");
        req["source_metadata"][0]["options"]["arguments"] = arguments;
        assert!(compile_json(&req.to_string()).await.is_err());
    }
}

#[tokio::test]
async fn aggregate_projection_and_optional_match_keep_input_bindings() {
    let rendered = sql(request(
        "rows_for_query",
        "MATCH (s:Sample) WHERE s.query = 'up' RETURN count(*) AS n",
    ))
    .await;
    assert!(
        rendered.contains("rows_for_query") && rendered.contains("'up'"),
        "{rendered}"
    );
    let rendered = sql(request("rows_for_query", "MATCH (a:Sample) WHERE a.query = 'first' OPTIONAL MATCH (b:Sample) WHERE b.query = 'second' RETURN a.value, b.value")).await;
    assert!(
        rendered.contains("'first'") && rendered.contains("'second'"),
        "{rendered}"
    );
    assert!(
        rendered.to_uppercase().contains("LEFT OUTER JOIN"),
        "{rendered}"
    );
}
