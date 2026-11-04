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
