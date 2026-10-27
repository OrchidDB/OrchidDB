use arrow::{
    array::{ArrayRef, Int64Array},
    datatypes::{Field, Schema},
    record_batch::RecordBatch,
};
use orchiddb::{
    compiler,
    execution::{
        plan::{self, Engine, Operation},
        stream::BatchStream,
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
fn request(engine: &str, query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","engine":engine,"language":"cypher","query":query,
        "sources":{"db":{"adapter":"postgres"}},
        "tables":[{"name":"people","source":"db","columns":[{"name":"id","data_type":"int64"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id"}}]})
}
async fn compile(value: Value) -> Result<compiler::CompiledSql, String> {
    compiler::compile(serde_json::from_value(value).unwrap()).await
}
struct Source(
    Arc<AtomicUsize>,
    Vec<orchiddb::federation::TransferColumn>,
    &'static str,
);
#[async_trait::async_trait(?Send)]
impl Session for Source {
    fn dialect(&self) -> &str {
        self.2
    }
    async fn query_stream(
        &mut self,
        sql: &str,
        inputs: Vec<orchiddb::execution::stream::InputRelation>,
    ) -> Result<BatchStream, String> {
        assert!(inputs.is_empty());
        assert!(sql.contains("people"));
        self.0.fetch_add(1, Ordering::SeqCst);
        let schema = Arc::new(Schema::new(
            self.1
                .iter()
                .map(|c| {
                    Field::new(
                        &c.name,
                        compiler::data_type(&c.data_type).unwrap(),
                        c.nullable,
                    )
                })
                .collect::<Vec<_>>(),
        ));
        let arrays =
            self.1
                .iter()
                .map(|c| match c.data_type.as_str() {
                    "int64" => Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
                    "string" => Arc::new(arrow::array::StringArray::from(vec!["Person", "Person"]))
                        as ArrayRef,
                    other => panic!("unexpected fake source type {other}"),
                })
                .collect();
        let batch = RecordBatch::try_new(schema.clone(), arrays).unwrap();
        Ok(BatchStream::new(schema, futures::stream::iter([Ok(batch)])))
    }
}
struct Tripwire;
#[async_trait::async_trait(?Send)]
impl Session for Tripwire {
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query_stream(
        &mut self,
        _: &str,
        _: Vec<orchiddb::execution::stream::InputRelation>,
    ) -> Result<BatchStream, String> {
        panic!("DuckDB must not run")
    }
}
struct Double;
impl plan::LocalFunction for Double {
    fn call(&self, args: &[ArrayRef], _: usize) -> Result<ArrayRef, String> {
        let a = args[0].as_any().downcast_ref::<Int64Array>().unwrap();
        Ok(Arc::new(
            a.iter().map(|v| v.map(|v| v * 2)).collect::<Int64Array>(),
        ))
    }
}
#[tokio::test]
async fn source_only_never_uses_duckdb() {
    for (engine, dialect) in [
        ("none", "postgres"),
        ("duckdb", "postgres"),
        ("none", "trino"),
        ("duckdb", "trino"),
    ] {
        let mut input = request(engine, "MATCH (p:Person) RETURN p.id AS id");
        input["sources"]["db"]["adapter"] = json!(dialect);
        let work = compile(input).await.unwrap();
        assert!(work.sql.is_empty());
        let plan = work.execution_plan.unwrap();
        assert!(
            !plan
                .nodes
                .iter()
                .any(|n| matches!(n.operation, Operation::Duckdb { .. }))
        );
        let calls = Arc::new(AtomicUsize::new(0));
        let mut sources: BTreeMap<String, Box<dyn Session>> = BTreeMap::from([(
            "db".into(),
            Box::new(Source(
                calls.clone(),
                plan.nodes
                    .iter()
                    .find(|n| matches!(n.operation, Operation::SourceSql { .. }))
                    .unwrap()
                    .columns
                    .clone(),
                dialect,
            )) as Box<dyn Session>,
        )]);
        let rows = plan::execute(
            &plan,
            &mut sources,
            &Default::default(),
            Some(&mut Tripwire),
        )
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
        assert_eq!(rows[0].num_rows(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn local_function_without_engine() {
    let mut value = request("none", "MATCH (p:Person) RETURN twice(p.id) AS doubled");
    value["functions"] = json!([{"name":"twice","target":"twice","parameters":["int64"],"returns":"int64","local":true}]);
    let work = compile(value).await.unwrap();
    let plan = work.execution_plan.unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut sources: BTreeMap<String, Box<dyn Session>> = BTreeMap::from([(
        "db".into(),
        Box::new(Source(
            calls.clone(),
            plan.nodes
                .iter()
                .find(|n| matches!(n.operation, Operation::SourceSql { .. }))
                .unwrap()
                .columns
                .clone(),
            "postgres",
        )) as Box<dyn Session>,
    )]);
    assert!(
        plan::execute(&plan, &mut sources, &Default::default(), None)
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let functions: plan::Functions = BTreeMap::from([(
        "twice".into(),
        Arc::new(Double) as Arc<dyn plan::LocalFunction>,
    )]);
    let rows = plan::execute(&plan, &mut sources, &functions, None)
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    let a = rows[0]
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(a.values(), &[2, 4]);
}
#[tokio::test]
async fn constants_without_sources_or_engine() {
    let value = json!({"version":1,"dialect":"duckdb","engine":"none","language":"cypher","query":"RETURN 42 AS answer"});
    let work = compile(value).await.unwrap();
    let plan = work.execution_plan.unwrap();
    assert_eq!(plan.engine, Engine::None);
    let rows = plan::execute(&plan, &mut Default::default(), &Default::default(), None)
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(rows[0].num_rows(), 1);
}
#[tokio::test]
async fn cross_source_join_requires_engine() {
    let mut value = request(
        "none",
        "MATCH (p:Person), (q:Other) WHERE p.id = q.id RETURN p.id AS id",
    );
    value["sources"]["other"] = json!({"adapter":"trino"});
    value["tables"].as_array_mut().unwrap().push(
        json!({"name":"others","source":"other","columns":[{"name":"id","data_type":"int64"}]}),
    );
    value["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"label":"Other","table":"others","id":"id","properties":{"id":"id"}}));
    let error = compile(value.clone()).await.unwrap_err();
    assert!(error.contains("engine none"), "{error}");
    value["engine"] = json!("duckdb");
    let work = compile(value).await.unwrap();
    assert!(
        work.execution_plan
            .unwrap()
            .nodes
            .iter()
            .any(|n| matches!(n.operation, Operation::Duckdb { .. }))
    );
}

struct Requests(Arc<AtomicUsize>);
#[async_trait::async_trait(?Send)]
impl Session for Requests {
    fn dialect(&self) -> &str {
        "test_requests"
    }
    async fn execute_requests_stream(
        &mut self,
        requests: orchiddb::federation::RequestStream,
        columns: Vec<orchiddb::federation::TransferColumn>,
    ) -> Result<BatchStream, String> {
        use futures::StreamExt;
        let calls = self.0.clone();
        let schema = Arc::new(Schema::new(vec![Field::new(
            &columns[0].name,
            arrow::datatypes::DataType::Int64,
            true,
        )]));
        let out = schema.clone();
        Ok(BatchStream::new(
            schema,
            requests.map(move |requests| {
                let requests = requests?;
                assert!(requests.len() <= 64);
                calls.fetch_add(1, Ordering::SeqCst);
                let values = requests
                    .iter()
                    .map(|r| r["id"].as_i64().unwrap())
                    .collect::<Vec<_>>();
                RecordBatch::try_new(out.clone(), vec![Arc::new(Int64Array::from(values))])
                    .map_err(|e| e.to_string())
            }),
        ))
    }
}
#[tokio::test]
async fn local_udf_feeds_a_dependent_request_without_engine() {
    let mut input = request("none", "MATCH (p:Person) RETURN twice(p.id) AS id");
    input["functions"] = json!([{"name":"twice","target":"twice","parameters":["int64"],"returns":"int64","local":true}]);
    let mut plan = compile(input).await.unwrap().execution_plan.unwrap();
    let columns = plan.nodes[plan.result].columns.clone();
    let schema_ipc = plan.nodes[plan.result].schema_ipc.clone();
    plan.nodes.push(plan::Node {
        columns,
        schema_ipc,
        operation: Operation::SourceRequest {
            source: "api".into(),
            input: Some(plan.result),
            template: orchiddb::operations::RequestTemplate {
                adapter: "test_requests".into(),
                parameters: 1,
                request: json!({"id":null}),
                bindings: vec![orchiddb::operations::RequestBinding {
                    pointer: "/id".into(),
                    parameter: 0,
                    encoding: Default::default(),
                }],
            },
        },
    });
    plan.result = plan.nodes.len() - 1;
    let calls = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let source_columns = plan
        .nodes
        .iter()
        .find(|n| matches!(n.operation, Operation::SourceSql { .. }))
        .unwrap()
        .columns
        .clone();
    let mut sources: BTreeMap<String, Box<dyn Session>> = BTreeMap::from([
        (
            "db".into(),
            Box::new(Source(calls.clone(), source_columns, "postgres")) as Box<dyn Session>,
        ),
        (
            "api".into(),
            Box::new(Requests(requests.clone())) as Box<dyn Session>,
        ),
    ]);
    let functions: plan::Functions = BTreeMap::from([(
        "twice".into(),
        Arc::new(Double) as Arc<dyn plan::LocalFunction>,
    )]);
    let mut engine = plan::LazyDuckdb::new(|| panic!("DuckDB factory must not run"));
    let result = plan::execute(&plan, &mut sources, &functions, Some(&mut engine))
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(
        result[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .values(),
        &[2, 4]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn local_predicate_is_completed_in_duckdb_without_pushing_the_function() {
    let mut value = request(
        "duckdb",
        "MATCH (p:Person) WHERE twice(p.id) > 2 RETURN p.id AS id",
    );
    value["functions"] = json!([{"name":"twice","target":"twice","parameters":["int64"],"returns":"int64","local":true}]);
    let plan = compile(value.clone())
        .await
        .unwrap()
        .execution_plan
        .unwrap();
    assert!(
        plan.nodes
            .iter()
            .any(|n| matches!(n.operation, Operation::LocalProject { .. }))
    );
    assert!(
        plan.nodes
            .iter()
            .any(|n| matches!(n.operation, Operation::Duckdb { .. }))
    );
    for node in plan.nodes {
        match node.operation {
            Operation::SourceSql { sql, .. } | Operation::Duckdb { sql, .. } => {
                assert!(!sql.contains("twice"), "{sql}")
            }
            _ => {}
        }
    }
    value["engine"] = json!("none");
    assert!(compile(value).await.unwrap_err().contains("engine none"));
}
