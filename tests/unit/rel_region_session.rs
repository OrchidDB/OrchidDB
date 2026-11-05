use super::*;
use crate::execution::stream::{BatchSource, BatchStream, InputRelation};
use arrow::{
    array::{ArrayRef, Int32Array, Int64Array},
    datatypes::{DataType, Field, Schema},
};

#[derive(Debug)]
struct Session {
    batch: RecordBatch,
    calls: usize,
    fail: bool,
}
impl RegionSession for Session {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }
    fn query(&mut self, _: &str, _: SchemaRef) -> SqlResult<RecordBatch> {
        self.calls += 1;
        if self.fail {
            return Err(SqlError::Unsupported("test query failure".into()));
        }
        Ok(self.batch.clone())
    }
}
fn session(array: ArrayRef) -> Session {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "actual",
        array.data_type().clone(),
        true,
    )]));
    Session {
        batch: RecordBatch::try_new(schema, vec![array]).unwrap(),
        calls: 0,
        fail: false,
    }
}
fn schema(kind: DataType, nullable: bool) -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new("requested", kind, nullable)]))
}

#[tokio::test]
async fn default_region_stream_preserves_values_and_avoids_recasting_matching_arrays() {
    let values: ArrayRef = Arc::new(Int32Array::from(vec![Some(3), None, Some(-1)]));
    let mut session = session(values.clone());
    let target = schema(DataType::Int32, true);
    let stream = session
        .query_stream("SELECT values", target.clone(), vec![])
        .unwrap();
    assert_eq!(stream.schema(), target);
    let batches = stream.collect_batches().await.unwrap();
    assert_eq!(batches.len(), 1);
    assert!(Arc::ptr_eq(batches[0].column(0), &values));
    let batches = session
        .query_stream("SELECT values", schema(DataType::Int64, true), vec![])
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(
        batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap(),
        &Int64Array::from(vec![Some(3), None, Some(-1)])
    );
    assert_eq!(session.calls, 2);
    assert!(!session.supports_concurrent_statements());
}

#[tokio::test]
async fn zero_column_region_stream_preserves_row_count() {
    let schema = Arc::new(Schema::empty());
    let batch = RecordBatch::try_new_with_options(
        schema.clone(),
        vec![],
        &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(3)),
    )
    .unwrap();
    let mut session = Session {
        batch,
        calls: 0,
        fail: false,
    };
    let batches = session
        .query_stream("SELECT cardinality", schema, vec![])
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(batches[0].num_columns(), 0);
    assert_eq!(batches[0].num_rows(), 3);
}

#[test]
fn default_region_rejects_invalid_outputs_inputs_and_unverified_helpers() {
    let mut session = session(Arc::new(Int32Array::from(vec![Some(1), None])));
    let error = session
        .query_stream("SELECT values", Arc::new(Schema::empty()), vec![])
        .unwrap_err();
    assert!(error.to_string().contains("column count mismatch"));
    let error = session
        .query_stream("SELECT values", schema(DataType::Int32, false), vec![])
        .unwrap_err();
    assert!(error.to_string().contains("NULL in non-nullable"));
    session.fail = true;
    let error = session
        .query_stream("SELECT failure", schema(DataType::Int32, true), vec![])
        .unwrap_err();
    assert!(error.to_string().contains("test query failure"));
    let calls = session.calls;
    let input = InputRelation {
        name: "input".into(),
        source: BatchSource::once(BatchStream::new(
            session.batch.schema(),
            futures::stream::iter([Ok(session.batch.clone())]),
        )),
    };
    let error = session
        .query_stream("SELECT input", schema(DataType::Int32, true), vec![input])
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("typed streaming input relations")
    );
    assert_eq!(
        session.calls, calls,
        "unsupported input must fail before issuing SQL"
    );
    let capability = crate::ir::rel::language_functions::FloatHelperCapability::verified(
        "test-registration".into(),
    );
    let error = session.validate_float_helper(&capability).unwrap_err();
    assert!(error.to_string().contains("test-registration"));
}

#[tokio::test]
async fn dependent_rewrite_uses_replacement_rows_and_shares_session_and_cost_only() {
    use crate::ir::{QueryCost, rel::{
        dependent::{DependentOperation, exec::DependentOperationExec},
        engine_exec::{ExecutionPlan, InputBatches, TaskContext},
        sql::lowering::SqlTemplate,
    }};
    use datafusion::{common::DFSchema, logical_expr::LogicalPlanBuilder};
    use futures::TryStreamExt;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct EchoSession(Arc<Mutex<Vec<String>>>);
    impl RegionSession for EchoSession {
        fn dialect(&self) -> SqlDialect { SqlDialect::DuckDb }
        fn query(&mut self, sql: &str, output: SchemaRef) -> SqlResult<RecordBatch> {
            self.0.lock().unwrap().push(sql.to_owned());
            let value = if sql.contains("41") { 41 } else if sql.contains("99") { 99 } else {
                return Err(SqlError::Unsupported(format!("unexpected bound SQL: {sql}")));
            };
            Ok(RecordBatch::try_new(output, vec![Arc::new(Int64Array::from(vec![value]))]).unwrap())
        }
    }
    let output = schema(DataType::Int64, false);
    let input = |value| -> Arc<dyn ExecutionPlan> {
        Arc::new(InputBatches::new(output.clone(), vec![RecordBatch::try_new(
            output.clone(), vec![Arc::new(Int64Array::from(vec![value]))],
        ).unwrap()]))
    };
    let original_input = input(41);
    let replacement = input(99);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session: SharedRegionSession = Arc::new(Mutex::new(Box::new(EchoSession(calls.clone()))));
    let cost = Arc::new(Mutex::new(QueryCost::default()));
    let operation = DependentOperation {
        source: Arc::new(LogicalPlanBuilder::empty(false).build().unwrap()),
        template: SqlTemplate { sql: "SELECT $1 AS requested".into(), parameters: 1, dialect: "duckdb".into() },
        schema: Arc::new(DFSchema::try_from(output.as_ref().clone()).unwrap()),
    };
    let original = Arc::new(DependentOperationExec::new(&operation, original_input.clone(), session, cost.clone()));
    for children in [vec![], vec![original_input.clone(), replacement.clone()]] {
        assert!(original.clone().with_new_children(children).unwrap_err().to_string().contains("requires one input"));
    }
    let rewritten = original.clone().with_new_children(vec![replacement.clone()]).unwrap();
    assert!(rewritten.as_any().is::<DependentOperationExec>());
    assert_eq!(rewritten.schema(), output);
    assert!(Arc::ptr_eq(rewritten.children()[0], &replacement));
    assert!(Arc::ptr_eq(original.children()[0], &original_input));
    let task = Arc::new(TaskContext::default());
    let partition_error = rewritten.execute(1, task.clone()).err().expect("only partition zero is valid");
    assert!(partition_error.to_string().contains("one partition"));
    assert!(calls.lock().unwrap().is_empty());
    for (plan, expected) in [(rewritten.clone(), 99), (original.clone() as Arc<dyn ExecutionPlan>, 41)] {
        let batches = plan.execute(0, task.clone()).unwrap().try_collect::<Vec<_>>().await.unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0), expected);
        assert_eq!(plan.metrics().unwrap().output_rows(), Some(1));
        assert_eq!(plan.metrics().unwrap().sum_by_name("sql_requests").unwrap().as_usize(), 1);
    }
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_eq!(cost.lock().unwrap().sql_executions, 2);
    assert_eq!(rewritten.metrics().unwrap().output_rows(), Some(1), "rewritten and original plans must own independent metrics");
}
