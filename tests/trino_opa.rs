//! Explicit live tests: the integration runner provisions Trino + PostgreSQL + OPA.
#![cfg(feature = "trino")]
use arrow::array::{ArrayRef, Int64Array};
use orchiddb::{
    compiler,
    execution::{
        plan::{self, Operation},
        stream::{BatchStream, InputRelation},
        trino::{TrinoOptions, TrinoSession},
    },
    federation::Session,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn source(user: &str) -> TrinoSession {
    let url = std::env::var("ORCHIDDB_TEST_OPA_URL")
        .expect("run tools/diagnostics/execution-integration/run.py; live endpoint required");
    let mut options = TrinoOptions::new(url, user);
    options.result_batch_rows = 1; // Exercise multiple Arrow batches for filtered results.
    TrinoSession::source(options).unwrap()
}
struct Counted(TrinoSession, Arc<AtomicUsize>);
#[async_trait::async_trait(?Send)]
impl Session for Counted {
    fn dialect(&self) -> &str {
        "trino"
    }
    async fn query_stream(
        &mut self,
        sql: &str,
        inputs: Vec<InputRelation>,
    ) -> Result<BatchStream, String> {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.execute(sql, inputs).await
    }
}
struct NoDuckdb;
#[async_trait::async_trait(?Send)]
impl Session for NoDuckdb {
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query_stream(
        &mut self,
        _: &str,
        _: Vec<InputRelation>,
    ) -> Result<BatchStream, String> {
        panic!("fully pushed down query must not execute DuckDB")
    }
}
struct Double;
impl plan::LocalFunction for Double {
    fn call(&self, args: &[ArrayRef], _: usize) -> Result<ArrayRef, String> {
        let a = args[0].as_any().downcast_ref::<Int64Array>().unwrap();
        Ok(Arc::new(
            a.iter().map(|v| v.map(|n| n * 2)).collect::<Int64Array>(),
        ))
    }
}
fn request(engine: &str, query: &str) -> Value {
    json!({"version":1,"dialect":"trino","engine":engine,"query":query,"language":"cypher",
        "sources":{"trino":{"adapter":"trino"}},
        "tables":[{"name":"postgresql.public.documents","source":"trino","columns":[
            {"name":"id","data_type":"int64"},{"name":"tenant","data_type":"string"},{"name":"secret","data_type":"string"}]}],
        "nodes":[{"label":"Doc","table":"postgresql.public.documents","id":"id","properties":{"id":"id","tenant":"tenant","secret":"secret"}}]})
}
async fn rows(stream: BatchStream) -> Result<Value, String> {
    let batches = stream.collect_batches().await?;
    let mut writer = arrow_json::ArrayWriter::new(Vec::new());
    for batch in &batches {
        writer.write(batch).map_err(|e| e.to_string())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    serde_json::from_slice(&writer.into_inner()).map_err(|e| e.to_string())
}
async fn execute(input: Value, user: &str, functions: plan::Functions) -> Result<Value, String> {
    let compiled = compiler::compile(serde_json::from_value(input).unwrap()).await?;
    let work = compiled.execution_plan.unwrap();
    assert!(compiled.sql.is_empty());
    assert!(
        !work
            .nodes
            .iter()
            .any(|n| matches!(n.operation, Operation::Duckdb { .. }))
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let mut sources: BTreeMap<String, Box<dyn Session>> = BTreeMap::from([(
        "trino".into(),
        Box::new(Counted(source(user), calls.clone())) as Box<dyn Session>,
    )]);
    let stream = plan::execute(&work, &mut sources, &functions, Some(&mut NoDuckdb)).await?;
    let result = rows(stream).await?;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "whole query must be a single Trino operation"
    );
    Ok(result)
}
#[tokio::test]
#[ignore = "requires provisioned Trino/OPA; integration runner executes explicitly"]
async fn tenant_filters_masks_and_identity_isolation() {
    for engine in ["none", "duckdb"] {
        for (user, expected) in [
            (
                "alice",
                json!([{"id":1,"secret":"masked"},{"id":3,"secret":"masked"}]),
            ),
            ("bob", json!([{"id":2,"secret":"masked"}])),
            (
                "admin",
                json!([{"id":1,"secret":"alice-private"},{"id":2,"secret":"bob-private"},{"id":3}]),
            ),
            (
                "alice",
                json!([{"id":1,"secret":"masked"},{"id":3,"secret":"masked"}]),
            ),
        ] {
            let actual = execute(
                request(
                    engine,
                    "MATCH (d:Doc) RETURN d.id AS id, d.secret AS secret ORDER BY id",
                ),
                user,
                Default::default(),
            )
            .await
            .unwrap();
            assert_eq!(actual, expected, "{engine}/{user}");
        }
        for user in ["denied", "restricted"] {
            let err = execute(
                request(engine, "MATCH (d:Doc) RETURN d.id AS id"),
                user,
                Default::default(),
            )
            .await
            .unwrap_err();
            assert!(err.to_lowercase().contains("denied"), "{err}");
        }
    }
}
#[tokio::test]
#[ignore = "requires provisioned Trino/OPA; integration runner executes explicitly"]
async fn source_functions_aggregation_and_local_udf() {
    for engine in ["none", "duckdb"] {
        assert_eq!(
            execute(
                request(engine, "MATCH (d:Doc) RETURN count(d) AS n"),
                "alice",
                Default::default()
            )
            .await
            .unwrap(),
            json!([{"n":2}])
        );
        assert_eq!(
            execute(
                request(
                    engine,
                    "MATCH (d:Doc) RETURN toUpper(d.tenant) AS tenant ORDER BY d.id"
                ),
                "bob",
                Default::default()
            )
            .await
            .unwrap(),
            json!([{"tenant":"BOB"}])
        );
        let mut input = request(engine, "MATCH (d:Doc) RETURN twice(d.id) AS doubled");
        input["functions"] = json!([{"name":"twice","target":"twice","parameters":["int64"],"returns":"int64","local":true}]);
        let functions: plan::Functions = BTreeMap::from([(
            "twice".into(),
            Arc::new(Double) as Arc<dyn plan::LocalFunction>,
        )]);
        assert_eq!(
            execute(input, "bob", functions).await.unwrap(),
            json!([{"doubled":4}])
        );
    }
}
#[tokio::test]
#[ignore = "requires provisioned Trino/OPA; integration runner executes explicitly"]
async fn cross_catalog_join_pushes_to_trino() {
    let mut input = request(
        "none",
        "MATCH (d:Doc), (l:Label) WHERE d.id = l.id RETURN d.id AS id, l.title AS title ORDER BY id",
    );
    input["tables"].as_array_mut().unwrap().push(json!({"name":"memory.orchid.labels","source":"trino","columns":[{"name":"id","data_type":"int64"},{"name":"title","data_type":"string"}]}));
    input["nodes"].as_array_mut().unwrap().push(json!({"label":"Label","table":"memory.orchid.labels","id":"id","properties":{"id":"id","title":"title"}}));
    for engine in ["none", "duckdb"] {
        input["engine"] = json!(engine);
        assert_eq!(
            execute(input.clone(), "alice", Default::default())
                .await
                .unwrap(),
            json!([{"id":1,"title":"first"},{"id":3,"title":"third"}])
        );
    }
}

#[tokio::test]
#[ignore = "requires provisioned Trino/OPA; integration runner executes explicitly"]
async fn iceberg_connector_retains_policy_enforcement() {
    for engine in ["none", "duckdb"] {
        let mut input = request(
            engine,
            "MATCH (d:Doc) RETURN d.id AS id, d.secret AS secret ORDER BY id",
        );
        input["tables"][0]["name"] = json!("iceberg.orchid.documents");
        input["nodes"][0]["table"] = json!("iceberg.orchid.documents");
        assert_eq!(
            execute(input, "alice", Default::default()).await.unwrap(),
            json!([{"id":1,"secret":"masked"},{"id":3,"secret":"masked"}])
        );
    }
}

#[tokio::test]
#[ignore = "requires provisioned Trino/OPA; integration runner executes explicitly"]
async fn source_stream_types_empty_results_and_errors() {
    use arrow::{
        array::{Array, Decimal128Array},
        datatypes::DataType,
    };
    let session = source("admin");
    let batches = session
        .execute(
            "SELECT amount FROM postgresql.public.documents ORDER BY id",
            vec![],
        )
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 3);
    assert!(batches.iter().all(|b| b.num_rows() == 1));
    assert_eq!(
        batches[0].schema().field(0).data_type(),
        &DataType::Decimal128(20, 2)
    );
    assert_eq!(
        batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .unwrap()
            .value(0),
        1250
    );
    assert!(batches[2].column(0).is_null(0));
    assert_eq!(
        rows(
            session
                .execute(
                    "SELECT id FROM postgresql.public.documents WHERE false",
                    vec![]
                )
                .await
                .unwrap()
        )
        .await
        .unwrap(),
        json!([])
    );
    let error = match session
        .execute(
            "SELECT * FROM postgresql.public.missing_orchid_fixture",
            vec![],
        )
        .await
    {
        Ok(_) => panic!("missing table unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(error.contains("missing_orchid_fixture"), "{error}");
    assert_eq!(
        rows(
            session
                .execute("SELECT CAST(42 AS BIGINT) AS n", vec![])
                .await
                .unwrap()
        )
        .await
        .unwrap(),
        json!([{"n":42}])
    );
}
