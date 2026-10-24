//! Compiler-to-community-extension contracts. Live runs use real extensions and
//! a deterministic local HTTP fixture, never a remote production service.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn request(engine: &str, query: &str, options: Value) -> Value {
    let value = if engine == "prometheus" {
        "value"
    } else {
        "line"
    };
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
        "tables":[{"name":"up","columns":[
            {"name":"sample_id","data_type":"string"},
            {"name":"timestamp","data_type":"timestamp"},
            {"name":"job","data_type":"string"},
            {"name":"missing","data_type":"string"},
            {"name":"labels","data_type":"string"},
            {"name":value,"data_type":if engine=="prometheus" {"float64"} else {"string"}}
        ]}],
        "nodes":[{"label":"Sample","table":"up","id":"sample_id","properties":{
            "sample_id":"sample_id","timestamp":"timestamp","job":"job","missing":"missing","labels":"labels",value:value
        }}],
        "source_metadata":[{"table":"up","format":format!("duckdb_{engine}"),"options":options}]
    })
}
fn options() -> Value {
    json!({"endpoint":"http://127.0.0.1:1","start":"2026-10-24T00:00:01Z","end":"2026-10-24T00:03:00Z"})
}
async fn compile(engine: &str, query: &str, options: Value) -> Value {
    let request = request(engine, query, options);
    serde_json::from_str(
        &compile_json(&request.to_string())
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}
#[tokio::test]
async fn generated_selectors_keep_residuals() {
    for (engine, query, fragment) in [
        (
            "prometheus",
            "MATCH (s:Sample) WHERE s.job = 'api' AND s.value > 0 RETURN s.value AS value",
            "job=\"api\"",
        ),
        (
            "loki",
            "MATCH (s:Sample) WHERE s.job = 'api' AND s.line CONTAINS 'timeout' RETURN s.line AS line",
            "|= \"timeout\"",
        ),
    ] {
        let result = compile(engine, query, options()).await;
        let sql = result["sql"].as_str().unwrap();
        assert!(sql.contains(&format!("{engine}_scan(")), "{sql}");
        assert!(sql.contains(fragment), "{sql}");
        assert!(sql.contains("WHERE"));
        if engine == "prometheus" {
            assert!(sql.contains("00:01:00"));
            assert!(sql.contains("or (("));
        } else {
            assert!(sql.contains("9223372036854775807"));
        }
    }
}
#[tokio::test]
async fn raw_queries_and_secrets() {
    let mut opts = options();
    opts["query"] = json!("sum by(job) (rate(up[5m]))");
    let result = compile(
        "prometheus",
        "MATCH (s:Sample) WHERE s.job = 'other' RETURN count(*) AS n",
        opts,
    )
    .await;
    let sql = result["sql"].as_str().unwrap();
    assert!(sql.contains("sum by(job) (rate(up[5m]))"));
    assert!(!sql.contains("job=\"other\""));
    let mut opts = options();
    opts.as_object_mut().unwrap().remove("endpoint");
    opts["secret"] = json!("logs'private");
    opts["query"] = json!("{job=\"O'Reilly\"} |= \"timeout\"");
    let result = compile("loki", "MATCH (s:Sample) RETURN s.line AS line", opts).await;
    let sql = result["sql"].as_str().unwrap();
    assert!(sql.contains("logs''private"));
    assert!(sql.contains("O''Reilly"));
}
#[tokio::test]
async fn validation_and_empty_grid() {
    for (engine, opts) in [
        ("prometheus", json!({"endpoint":"http://localhost"})),
        ("loki", options()), // no nonempty label selector
        (
            "prometheus",
            json!({"secret":"unsupported","start":"2026-10-24T00:00:00Z","end":"2026-10-24T00:01:00Z"}),
        ),
        (
            "prometheus",
            json!({"endpoint":"http://localhost","start":"bad","end":"bad"}),
        ),
        (
            "prometheus",
            json!({"endpoint":"http://localhost","unknown":true}),
        ),
    ] {
        assert!(
            compile_json(
                &request(engine, "MATCH (s:Sample) RETURN count(*) AS n", opts).to_string()
            )
            .await
            .is_err()
        );
    }
    let mut opts = options();
    opts["end"] = json!("2026-10-24T00:00:02Z");
    let result = compile("prometheus", "MATCH (s:Sample) RETURN count(*) AS n", opts).await;
    let sql = result["sql"].as_str().unwrap();
    assert!(!sql.contains("prometheus_scan"));
    assert!(sql.contains("false"));
}
#[tokio::test]
async fn actual_extensions() {
    if std::env::var_os("ORCHIDDB_TEST_DUCKDB_OBSERVABILITY").is_none() {
        return;
    }
    for (engine, query) in [
        (
            "prometheus",
            "MATCH (s:Sample) WHERE s.job = 'api' AND s.value > 0 AND s.missing IS NULL RETURN count(*) AS n, sum(s.value) AS total",
        ),
        (
            "loki",
            "MATCH (s:Sample) WHERE s.job = 'api' AND s.line = 'timeout' AND s.missing IS NULL RETURN count(*) AS n",
        ),
    ] {
        let result = compile(engine, query, options()).await;
        let output = std::process::Command::new(
            std::env::var("ORCHIDDB_TEST_PYTHON").unwrap_or("python3".into()),
        )
        .args([
            "tools/diagnostics/duckdb_observability.py",
            engine,
            result["sql"].as_str().unwrap(),
        ])
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            response["rows"][0][0],
            json!(if engine == "prometheus" { 2 } else { 1 })
        );
        if engine == "prometheus" {
            assert_eq!(response["rows"][0][1], json!(3.0));
        }
    }
}

#[tokio::test]
async fn limit_barriers() {
    let result=compile("prometheus", "MATCH (s:Sample) WITH s ORDER BY s.value LIMIT 1 WHERE s.job = 'api' RETURN s.value AS value", options()).await;
    assert!(
        !result["sql"].as_str().unwrap().contains("job=\"api\""),
        "filter moved across LIMIT: {}",
        result["sql"]
    );
}

#[cfg(all(feature = "prometheus", feature = "loki"))]
#[test]
fn native_adapters_still_emit_requests() {
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use datafusion::{
        common::ScalarValue,
        datasource::{empty::EmptyTable, provider_as_source},
        logical_expr::{LogicalPlanBuilder, col, lit},
    };
    use std::sync::Arc;
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("job", DataType::Utf8, true),
        Field::new("sample_id", DataType::Utf8, false),
    ]));
    let start = 1792800000i64 * 1_000_000;
    let plan = LogicalPlanBuilder::scan(
        "up",
        provider_as_source(Arc::new(EmptyTable::new(schema))),
        None,
    )
    .unwrap()
    .filter(
        col("timestamp")
            .gt_eq(lit(ScalarValue::TimestampMicrosecond(Some(start), None)))
            .and(col("timestamp").lt(lit(ScalarValue::TimestampMicrosecond(
                Some(start + 60_000_000),
                None,
            )))),
    )
    .unwrap()
    .filter(
        col("job")
            .eq(lit("api"))
            .and(col("sample_id").eq(lit("source-key"))),
    )
    .unwrap()
    .build()
    .unwrap();
    for adapter in orchiddb::remote::adapters() {
        if !matches!(adapter.name(), "prometheus" | "loki") {
            continue;
        }
        let operation = adapter.lower(&plan).unwrap().unwrap();
        assert_eq!(
            operation.template.request["query"],
            json!(if adapter.name() == "prometheus" {
                "up{job=\"api\"}"
            } else {
                "{job=\"api\"}"
            })
        );
        assert!(operation.replacement.is_some());
    }
}

#[tokio::test]
async fn actual_extension_identity_and_timestamp() {
    if std::env::var_os("ORCHIDDB_TEST_DUCKDB_OBSERVABILITY").is_none() {
        return;
    }
    for engine in ["prometheus", "loki"] {
        let predicate = if engine == "prometheus" {
            "s.value > 0"
        } else {
            "s.line = 'timeout'"
        };
        let query = format!(
            "MATCH (s:Sample) WHERE s.job = 'api' AND {predicate} RETURN s.sample_id AS id, s.timestamp AS timestamp, s.labels AS labels ORDER BY timestamp"
        );
        let result = compile(engine, &query, options()).await;
        let output = std::process::Command::new(
            std::env::var("ORCHIDDB_TEST_PYTHON").unwrap_or("python3".into()),
        )
        .args([
            "tools/diagnostics/duckdb_observability.py",
            engine,
            result["sql"].as_str().unwrap(),
        ])
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        let row = &response["rows"][0];
        assert_eq!(row[1], json!("2026-10-24 00:01:00"));
        let key: Value = serde_json::from_str(row[0].as_str().unwrap()).unwrap();
        assert_eq!(key[1]["job"], json!("api"));
        if engine == "loki" {
            assert!(key[0].as_str().unwrap().ends_with("123"));
            assert_eq!(key[2], json!("timeout"));
        } else {
            assert!(key[2].is_null());
        }
    }
}

#[tokio::test]
async fn actual_loki_paging_has_no_default_total_cap() {
    if std::env::var_os("ORCHIDDB_TEST_DUCKDB_OBSERVABILITY").is_none() {
        return;
    }
    let result = compile(
        "loki",
        "MATCH (s:Sample) WHERE s.job = 'api' AND s.line = 'timeout' RETURN count(*) AS n",
        options(),
    )
    .await;
    let output = std::process::Command::new(
        std::env::var("ORCHIDDB_TEST_PYTHON").unwrap_or("python3".into()),
    )
    .args([
        "tools/diagnostics/duckdb_observability.py",
        "loki",
        result["sql"].as_str().unwrap(),
        "paged",
    ])
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["rows"][0][0], json!(5002));
}
