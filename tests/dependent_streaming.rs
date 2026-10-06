//! Contract tests for demand, memory ownership, and failure at SQL boundaries.
use arrow::{
    array::{Int64Array, RecordBatch, RecordBatchOptions},
    datatypes::{DataType, Field, Schema, SchemaRef},
};
use datafusion::{
    common::{DFSchema, DataFusionError, Result},
    execution::{
        TaskContext,
        memory_pool::{GreedyMemoryPool, MemoryPool},
        runtime_env::RuntimeEnvBuilder,
    },
    logical_expr::LogicalPlanBuilder,
    physical_expr::EquivalenceProperties,
    physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
        SendableRecordBatchStream,
        execution_plan::{Boundedness, EmissionType},
        limit::GlobalLimitExec,
        stream::RecordBatchStreamAdapter,
    },
    prelude::{SessionConfig, SessionContext},
};
use futures::{StreamExt, TryStreamExt, stream};
use orchiddb::ir::{
    QueryCost,
    rel::{
        dependent::{DependentOperation, exec::DependentOperationExec},
        sql::{SqlDialect, SqlError, SqlResult, lowering::SqlTemplate, region::RegionSession},
    },
};
use std::{
    any::Any,
    collections::VecDeque,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn schema(nullable: bool) -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        nullable,
    )]))
}
fn batch(values: Vec<i64>) -> RecordBatch {
    RecordBatch::try_new(schema(false), vec![Arc::new(Int64Array::from(values))]).unwrap()
}
fn values(batches: &[RecordBatch]) -> Vec<i64> {
    batches
        .iter()
        .flat_map(|b| {
            b.column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .values()
                .to_vec()
        })
        .collect()
}

#[derive(Debug)]
struct Feed {
    batches: Vec<RecordBatch>,
    fail_at_end: bool,
    polls: Arc<AtomicUsize>,
    properties: Arc<PlanProperties>,
}
impl DisplayAs for Feed {
    fn fmt_as(&self, _: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Feed")
    }
}
impl ExecutionPlan for Feed {
    fn name(&self) -> &str {
        "Feed"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![]
    }
    fn with_new_children(
        self: Arc<Self>,
        _: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Ok(self)
    }
    fn execute(&self, _: usize, _: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        let mut batches = self.batches.iter().cloned().map(Ok).collect::<Vec<_>>();
        if self.fail_at_end {
            batches.push(Err(DataFusionError::Execution("source failed".into())));
        }
        let polls = self.polls.clone();
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            stream::iter(batches).inspect(move |_| {
                polls.fetch_add(1, Ordering::SeqCst);
            }),
        )))
    }
}

#[derive(Debug)]
struct Session {
    responses: VecDeque<SqlResult<RecordBatch>>,
    calls: Arc<Mutex<Vec<String>>>,
}
impl RegionSession for Session {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }
    fn query(&mut self, sql: &str, _: SchemaRef) -> SqlResult<RecordBatch> {
        self.calls.lock().unwrap().push(sql.to_string());
        self.responses.pop_front().expect("no speculative requests")
    }
}
struct Fixture {
    plan: Arc<DependentOperationExec>,
    task: Arc<TaskContext>,
    calls: Arc<Mutex<Vec<String>>>,
    polls: Arc<AtomicUsize>,
    pool: Arc<dyn MemoryPool>,
    session: Arc<Mutex<Box<dyn RegionSession>>>,
}
impl Fixture {
    fn new(
        inputs: Vec<RecordBatch>,
        responses: Vec<SqlResult<RecordBatch>>,
        output: SchemaRef,
        budget: usize,
        fail_at_end: bool,
    ) -> Self {
        let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(budget));
        let runtime = RuntimeEnvBuilder::new()
            .with_memory_pool(pool.clone())
            .build_arc()
            .unwrap();
        let ctx = SessionContext::new_with_config_rt(SessionConfig::new(), runtime);
        let polls = Arc::new(AtomicUsize::new(0));
        let input_schema = inputs
            .first()
            .map(RecordBatch::schema)
            .unwrap_or_else(|| schema(false));
        let source = Arc::new(Feed {
            batches: inputs,
            fail_at_end,
            polls: polls.clone(),
            properties: Arc::new(PlanProperties::new(
                EquivalenceProperties::new(input_schema),
                Partitioning::UnknownPartitioning(1),
                EmissionType::Incremental,
                Boundedness::Bounded,
            )),
        });
        let operation = DependentOperation {
            source: Arc::new(LogicalPlanBuilder::empty(false).build().unwrap()),
            template: SqlTemplate {
                sql: "SELECT $1 AS value".into(),
                parameters: 1,
                dialect: "duckdb".into(),
            },
            schema: Arc::new(DFSchema::try_from(output.as_ref().clone()).unwrap()),
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        let session = Arc::new(Mutex::new(Box::new(Session {
            responses: responses.into(),
            calls: calls.clone(),
        }) as Box<dyn RegionSession>));
        let plan = Arc::new(DependentOperationExec::new(
            &operation,
            source,
            session.clone(),
            Arc::new(Mutex::new(QueryCost::default())),
        ));
        Self {
            plan,
            task: ctx.task_ctx(),
            calls,
            polls,
            pool,
            session,
        }
    }
    fn metric(&self, name: &str) -> usize {
        let metrics = self.plan.metrics().unwrap();
        if name == "output_rows" {
            metrics.output_rows().unwrap()
        } else {
            metrics.sum_by_name(name).unwrap().as_usize()
        }
    }
}

#[tokio::test]
async fn limit_one_does_not_execute_remaining_rows_or_poll_later_batches() {
    let f = Fixture::new(
        vec![batch(vec![2, 3]), batch(vec![4])],
        vec![Ok(batch(vec![20]))],
        schema(false),
        4096,
        false,
    );
    let limit = GlobalLimitExec::new(f.plan.clone(), 0, Some(1));
    let mut stream = limit.execute(0, f.task.clone()).unwrap();
    assert_eq!(f.polls.load(Ordering::SeqCst), 0);
    assert!(f.calls.lock().unwrap().is_empty());
    assert_eq!(
        values(&[stream.try_next().await.unwrap().unwrap()]),
        vec![20]
    );
    assert!(stream.try_next().await.unwrap().is_none());
    drop(stream);
    assert_eq!(f.polls.load(Ordering::SeqCst), 1);
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    assert_eq!(f.metric("template_preparations"), 1);
    assert_eq!(f.metric("sql_requests"), 1);
    assert_eq!(f.pool.reserved(), 0);
}

#[tokio::test]
async fn streaming_preserves_duplicates_empty_results_and_multirow_output_order() {
    let f = Fixture::new(
        vec![batch(vec![]), batch(vec![2, 2]), batch(vec![3])],
        vec![
            Ok(batch(vec![20, 21])),
            Ok(batch(vec![])),
            Ok(batch(vec![30, 31])),
        ],
        schema(false),
        4096,
        false,
    );
    let result = datafusion::physical_plan::collect(f.plan.clone(), f.task.clone())
        .await
        .unwrap();
    assert_eq!(values(&result), vec![20, 21, 30, 31]);
    assert_eq!(
        *f.calls.lock().unwrap(),
        vec![
            "SELECT CAST(2 AS BIGINT) AS value",
            "SELECT CAST(2 AS BIGINT) AS value",
            "SELECT CAST(3 AS BIGINT) AS value"
        ]
    );
    assert_eq!(f.metric("template_preparations"), 1);
    assert_eq!(f.metric("input_batches"), 3);
    assert_eq!(f.metric("input_rows"), 3);
    assert_eq!(f.metric("output_rows"), 4);
    assert_eq!(f.pool.reserved(), 0);
}

#[tokio::test]
async fn dropping_a_partially_consumed_stream_releases_memory_and_stops_requests() {
    let f = Fixture::new(
        vec![batch(vec![2, 3])],
        vec![Ok(batch(vec![20]))],
        schema(false),
        4096,
        false,
    );
    let mut stream = f.plan.execute(0, f.task.clone()).unwrap();
    stream.try_next().await.unwrap().unwrap();
    assert!(f.pool.reserved() > 0);
    drop(stream);
    assert_eq!(f.pool.reserved(), 0);
    assert_eq!(f.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn memory_budget_rejects_input_before_a_sql_request() {
    let f = Fixture::new(vec![batch(vec![2, 3])], vec![], schema(false), 1, false);
    let error = datafusion::physical_plan::collect(f.plan.clone(), f.task.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Resources exhausted"), "{error}");
    assert!(f.calls.lock().unwrap().is_empty());
    assert_eq!(f.pool.reserved(), 0);
}

#[tokio::test]
async fn later_driver_failure_is_emitted_once_without_retry_or_following_requests() {
    let f = Fixture::new(
        vec![batch(vec![2, 3, 4])],
        vec![
            Ok(batch(vec![20])),
            Err(SqlError::Execution("driver failed".into())),
        ],
        schema(false),
        4096,
        false,
    );
    let mut stream = f.plan.execute(0, f.task.clone()).unwrap();
    assert_eq!(
        values(&[stream.try_next().await.unwrap().unwrap()]),
        vec![20]
    );
    assert!(
        stream
            .try_next()
            .await
            .unwrap_err()
            .to_string()
            .contains("driver failed")
    );
    assert!(stream.try_next().await.unwrap().is_none());
    assert_eq!(f.calls.lock().unwrap().len(), 2);
    assert_eq!(f.metric("sql_errors"), 1);
    assert_eq!(f.pool.reserved(), 0);
}

#[tokio::test]
async fn upstream_error_follows_already_emitted_rows_without_retry() {
    let f = Fixture::new(
        vec![batch(vec![2])],
        vec![Ok(batch(vec![20]))],
        schema(false),
        4096,
        true,
    );
    let mut stream = f.plan.execute(0, f.task.clone()).unwrap();
    stream.try_next().await.unwrap().unwrap();
    assert!(
        stream
            .try_next()
            .await
            .unwrap_err()
            .to_string()
            .contains("source failed")
    );
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    assert_eq!(f.pool.reserved(), 0);
}

#[tokio::test]
async fn output_contract_errors_remain_errors() {
    let wrong_columns = RecordBatch::new_empty(Arc::new(Schema::empty()));
    let null =
        RecordBatch::try_new(schema(true), vec![Arc::new(Int64Array::from(vec![None]))]).unwrap();
    for (response, message) in [
        (wrong_columns, "column count mismatch"),
        (null, "NULL in non-nullable"),
    ] {
        let f = Fixture::new(
            vec![batch(vec![2, 3])],
            vec![Ok(response)],
            schema(false),
            4096,
            false,
        );
        let error = datafusion::physical_plan::collect(f.plan.clone(), f.task.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(f.calls.lock().unwrap().len(), 1);
        assert_eq!(f.pool.reserved(), 0);
    }
}

#[tokio::test]
async fn zero_column_results_preserve_row_count() {
    let output = Arc::new(Schema::empty());
    let response = RecordBatch::try_new_with_options(
        output.clone(),
        vec![],
        &RecordBatchOptions::new().with_row_count(Some(3)),
    )
    .unwrap();
    let f = Fixture::new(
        vec![batch(vec![2])],
        vec![Ok(response)],
        output,
        4096,
        false,
    );
    let result = datafusion::physical_plan::collect(f.plan.clone(), f.task.clone())
        .await
        .unwrap();
    assert_eq!(result.iter().map(RecordBatch::num_rows).sum::<usize>(), 3);
    assert_eq!(f.metric("output_rows"), 3);
}

#[tokio::test]
async fn later_demand_uses_bounded_groups_and_drop_releases_queued_output() {
    let f = Fixture::new(
        vec![batch(vec![2; 1024])],
        (0..65).map(|_| Ok(batch(vec![20]))).collect(),
        schema(false),
        64 * 1024,
        false,
    );
    let mut stream = f.plan.execute(0, f.task.clone()).unwrap();
    stream.try_next().await.unwrap().unwrap();
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    stream.try_next().await.unwrap().unwrap();
    assert_eq!(f.calls.lock().unwrap().len(), 65);
    assert!(f.pool.reserved() > 0);
    // Consuming queued output needs neither another worker nor more SQL calls.
    stream.try_next().await.unwrap().unwrap();
    assert_eq!(f.calls.lock().unwrap().len(), 65);
    drop(stream);
    assert_eq!(f.pool.reserved(), 0);
}

#[tokio::test]
async fn output_budget_failure_stops_following_occurrences_and_frees_reservations() {
    let f = Fixture::new(
        vec![batch(vec![2, 3])],
        vec![Ok(batch(vec![20; 1024]))],
        schema(false),
        1024,
        false,
    );
    let error = datafusion::physical_plan::collect(f.plan.clone(), f.task.clone())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Resources exhausted"), "{error}");
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    assert_eq!(f.pool.reserved(), 0);
}

#[tokio::test]
async fn large_fanout_stops_a_group_at_the_output_byte_threshold() {
    let f = Fixture::new(
        vec![batch(vec![2; 100])],
        vec![Ok(batch(vec![20])), Ok(batch(vec![30; 140_000]))],
        schema(false),
        4 * 1024 * 1024,
        false,
    );
    let mut stream = f.plan.execute(0, f.task.clone()).unwrap();
    stream.try_next().await.unwrap().unwrap();
    assert_eq!(
        stream.try_next().await.unwrap().unwrap().num_rows(),
        140_000
    );
    assert_eq!(f.calls.lock().unwrap().len(), 2);
    drop(stream);
    assert_eq!(f.pool.reserved(), 0);
}

#[derive(Debug)]
struct BlockingSession {
    started: Option<tokio::sync::oneshot::Sender<()>>,
    resume: std::sync::mpsc::Receiver<()>,
    calls: Arc<AtomicUsize>,
}
impl RegionSession for BlockingSession {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }
    fn query(&mut self, _: &str, _: SchemaRef) -> SqlResult<RecordBatch> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
            self.started.take().unwrap().send(()).unwrap();
            self.resume.recv().unwrap();
        }
        Ok(batch(vec![20]))
    }
}

#[tokio::test]
async fn cancelling_during_a_group_keeps_live_memory_charged_and_stops_following_sql() {
    let f = Fixture::new(
        vec![batch(vec![2; 100])],
        vec![],
        schema(false),
        64 * 1024,
        false,
    );
    let (started, running) = tokio::sync::oneshot::channel();
    let (resume, blocked) = std::sync::mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    *f.session.lock().unwrap() = Box::new(BlockingSession {
        started: Some(started),
        resume: blocked,
        calls: calls.clone(),
    });
    let mut stream = f.plan.execute(0, f.task.clone()).unwrap();
    stream.try_next().await.unwrap().unwrap();
    let consumer = tokio::spawn(async move { stream.try_next().await });
    running.await.unwrap();
    consumer.abort();
    assert!(consumer.await.unwrap_err().is_cancelled());
    // The blocking driver still owns a slice of the input batch.
    assert!(f.pool.reserved() > 0);
    resume.send(()).unwrap();
    let started = std::time::Instant::now();
    while f.pool.reserved() != 0 {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "worker did not release memory"
        );
        tokio::task::yield_now().await;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn prepared_template_does_not_retain_a_previous_rows_bound_value() {
    let input = RecordBatch::try_new(
        schema(true),
        vec![Arc::new(Int64Array::from(vec![Some(7), None, Some(9)]))],
    )
    .unwrap();
    let f = Fixture::new(
        vec![input],
        (0..3).map(|_| Ok(batch(vec![1]))).collect(),
        schema(false),
        4096,
        false,
    );
    datafusion::physical_plan::collect(f.plan.clone(), f.task.clone())
        .await
        .unwrap();
    assert_eq!(
        *f.calls.lock().unwrap(),
        vec![
            "SELECT CAST(7 AS BIGINT) AS value",
            "SELECT CAST(NULL AS BIGINT) AS value",
            "SELECT CAST(9 AS BIGINT) AS value",
        ]
    );
    assert_eq!(f.metric("template_preparations"), 1);
}
