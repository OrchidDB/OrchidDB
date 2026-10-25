//! Execute dependent relational operations through an engine-owned SQL session.
use super::DependentOperation;
use crate::ir::rel::engine_exec::{
    self, Count, ExecutionPlan, ExecutionPlanMetricsSet, Gauge, MemoryConsumer, MemoryReservation,
    MetricBuilder, MetricsSet, PlanProperties, RecordBatchStreamAdapter, SendableRecordBatchStream,
    TaskContext, Time,
};
use crate::ir::rel::{dag::coerce_sql_array, sql};
use arrow::{
    array::{RecordBatch, RecordBatchOptions},
    datatypes::SchemaRef,
};
use datafusion::common::{DataFusionError, Result, ScalarValue};
use futures::{TryStreamExt, stream};
use std::{
    any::Any,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
pub struct DependentOperationExec {
    source: Arc<dyn ExecutionPlan>,
    template: sql::lowering::SqlTemplate,
    session: sql::region::SharedRegionSession,
    cost: Arc<Mutex<crate::ir::QueryCost>>,
    properties: Arc<PlanProperties>,
    metrics: ExecutionPlanMetricsSet,
}
impl DependentOperationExec {
    pub fn new(
        operation: &DependentOperation,
        source: Arc<dyn ExecutionPlan>,
        session: sql::region::SharedRegionSession,
        cost: Arc<Mutex<crate::ir::QueryCost>>,
    ) -> Self {
        Self {
            source,
            template: operation.template.clone(),
            session,
            cost,
            properties: Arc::new(PlanProperties::new(Arc::new(
                operation.schema.as_arrow().clone(),
            ))),
            metrics: ExecutionPlanMetricsSet::new(),
        }
    }
}
impl ExecutionPlan for DependentOperationExec {
    fn name(&self) -> &str {
        "DependentOperationExec"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }
    fn metrics(&self) -> Option<MetricsSet> {
        Some(self.metrics.clone_inner())
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.source]
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.len() != 1 {
            return Err(DataFusionError::Plan(
                "dependent operation requires one input".into(),
            ));
        }
        Ok(Arc::new(Self {
            source: children[0].clone(),
            template: self.template.clone(),
            session: self.session.clone(),
            cost: self.cost.clone(),
            properties: Arc::new(PlanProperties::new(self.schema())),
            metrics: ExecutionPlanMetricsSet::new(),
        }))
    }
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(
                "dependent operation has one partition".into(),
            ));
        }
        let cursor = Cursor {
            input: engine_exec::execute_stream(self.source.clone(), context.clone())?,
            batch: None,
            row: 0,
            template: self.template.clone(),
            prepared: None,
            session: self.session.clone(),
            cost: self.cost.clone(),
            schema: self.schema(),
            checked_dialect: false,
            cancelled: Arc::new(AtomicBool::new(false)),
            reservation: Arc::new(
                MemoryConsumer::new("DependentOperationExec input").register(context.memory_pool()),
            ),
            output_reservation: Arc::new(
                MemoryConsumer::new("DependentOperationExec output")
                    .register(context.memory_pool()),
            ),
            response: None,
            metrics: BoundaryMetrics::new(&self.metrics),
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            stream::try_unfold(cursor, Cursor::next),
        )))
    }
}

// Per-boundary measurements owned by Orchid's stream scheduler.
// Byte gauges describe Arrow batches at this boundary, not driver-internal memory.
#[derive(Clone)]
struct BoundaryMetrics {
    input_batches: Count,
    input_rows: Count,
    sql_requests: Count,
    sql_errors: Count,
    template_preparations: Count,
    output_rows: Count,
    output_batches: Count,
    output_bytes: Count,
    peak_input_batch_bytes: Gauge,
    peak_output_batch_bytes: Gauge,
    peak_buffered_output_bytes: Gauge,
    binding_time: Time,
    sql_time: Time,
}
impl BoundaryMetrics {
    fn new(metrics: &ExecutionPlanMetricsSet) -> Self {
        Self {
            input_batches: MetricBuilder::new(metrics).counter("input_batches", 0),
            input_rows: MetricBuilder::new(metrics).counter("input_rows", 0),
            sql_requests: MetricBuilder::new(metrics).counter("sql_requests", 0),
            sql_errors: MetricBuilder::new(metrics).counter("sql_errors", 0),
            template_preparations: MetricBuilder::new(metrics).counter("template_preparations", 0),
            output_rows: MetricBuilder::new(metrics).output_rows(0),
            output_batches: MetricBuilder::new(metrics).output_batches(0),
            output_bytes: MetricBuilder::new(metrics).output_bytes(0),
            peak_input_batch_bytes: MetricBuilder::new(metrics).gauge("peak_input_batch_bytes", 0),
            peak_output_batch_bytes: MetricBuilder::new(metrics)
                .gauge("peak_output_batch_bytes", 0),
            peak_buffered_output_bytes: MetricBuilder::new(metrics)
                .gauge("peak_buffered_output_bytes", 0),
            binding_time: MetricBuilder::new(metrics).subset_time("binding_time", 0),
            sql_time: MetricBuilder::new(metrics).subset_time("sql_time", 0),
        }
    }
}

fn failure(error: impl std::fmt::Display) -> DataFusionError {
    DataFusionError::Execution(error.to_string())
}

/// A response remains a stream until the consumer requests its next batch.
/// Only the current source batch and response batch are owned by this boundary.
struct Cursor {
    input: SendableRecordBatchStream,
    batch: Option<RecordBatch>,
    row: usize,
    template: sql::lowering::SqlTemplate,
    prepared: Option<Arc<sql::lowering::PreparedSqlTemplate>>,
    session: sql::region::SharedRegionSession,
    cost: Arc<Mutex<crate::ir::QueryCost>>,
    schema: SchemaRef,
    checked_dialect: bool,
    cancelled: Arc<AtomicBool>,
    reservation: Arc<MemoryReservation>,
    output_reservation: Arc<MemoryReservation>,
    response: Option<crate::execution::stream::BatchStream>,
    metrics: BoundaryMetrics,
}
impl Drop for Cursor {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Cursor {
    async fn next(mut self) -> Result<Option<(RecordBatch, Self)>> {
        loop {
            if let Some(response) = &mut self.response {
                let returned = response.try_next().await.map_err(|error| {
                    self.metrics.sql_errors.add(1);
                    failure(error)
                })?;
                if let Some(returned) = returned {
                    if returned.num_columns() != self.schema.fields().len() {
                        return Err(failure("dependent operation output column count mismatch"));
                    }
                    if returned
                        .columns()
                        .iter()
                        .zip(self.schema.fields())
                        .any(|(a, f)| !f.is_nullable() && a.null_count() != 0)
                    {
                        return Err(failure("NULL in non-nullable dependent operation output"));
                    }
                    let arrays = returned
                        .columns()
                        .iter()
                        .zip(self.schema.fields())
                        .map(|(a, f)| coerce_sql_array(a, f.data_type()))
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    let batch = RecordBatch::try_new_with_options(
                        self.schema.clone(),
                        arrays,
                        &RecordBatchOptions::new().with_row_count(Some(returned.num_rows())),
                    )?;
                    let bytes = batch.get_array_memory_size();
                    self.output_reservation.try_resize(bytes)?;
                    self.metrics.peak_buffered_output_bytes.set_max(bytes);
                    self.metrics.peak_output_batch_bytes.set_max(bytes);
                    self.metrics.output_rows.add(batch.num_rows());
                    self.metrics.output_batches.add(1);
                    self.metrics.output_bytes.add(bytes);
                    {
                        let mut cost = self
                            .cost
                            .lock()
                            .map_err(|_| failure("query cost poisoned"))?;
                        cost.sql_output_rows += batch.num_rows() as u64;
                        cost.sql_output_bytes += bytes as u64;
                    }
                    self.output_reservation.free();
                    return Ok(Some((batch, self)));
                }
                self.response = None;
            }
            while self.batch.is_none() {
                let Some(batch) = self.input.try_next().await? else {
                    return Ok(None);
                };
                self.metrics.input_batches.add(1);
                if batch.num_rows() == 0 {
                    continue;
                }
                let bytes = batch.get_array_memory_size();
                self.reservation.try_resize(bytes)?;
                self.metrics.peak_input_batch_bytes.set_max(bytes);
                self.batch = Some(batch);
                self.row = 0;
            }
            let query = {
                let _timer = self.metrics.binding_time.timer();
                if self.prepared.is_none() {
                    self.prepared = Some(Arc::new(self.template.prepare().map_err(failure)?));
                    self.metrics.template_preparations.add(1);
                }
                let values = self
                    .batch
                    .as_ref()
                    .unwrap()
                    .columns()
                    .iter()
                    .map(|a| ScalarValue::try_from_array(a.as_ref(), self.row))
                    .collect::<Result<Vec<_>>>()?;
                self.prepared
                    .as_ref()
                    .unwrap()
                    .bind(&values)
                    .map_err(failure)?
            };
            let session = self.session.clone();
            let schema = self.schema.clone();
            let dialect = self.template.dialect.clone();
            let cancelled = self.cancelled.clone();
            let reservation = self.reservation.clone();
            let metrics = self.metrics.clone();
            let response = tokio::task::spawn_blocking(move || {
                let _reservation = reservation;
                let mut session = session
                    .lock()
                    .map_err(|_| failure("SQL session poisoned"))?;
                if cancelled.load(Ordering::Acquire) {
                    return Err(failure("dependent execution cancelled"));
                }
                if session.dialect().name() != dialect {
                    return Err(failure("dependent operation session dialect mismatch"));
                }
                metrics.input_rows.add(1);
                metrics.sql_requests.add(1);
                let _timer = metrics.sql_time.timer();
                session
                    .query_stream(&query, schema, vec![])
                    .map_err(|error| {
                        metrics.sql_errors.add(1);
                        failure(error)
                    })
            })
            .await
            .map_err(failure)??;
            self.checked_dialect = true;
            self.cost
                .lock()
                .map_err(|_| failure("query cost poisoned"))?
                .sql_executions += 1;
            self.response = Some(response.record_cost(self.cost.clone()));
            self.row += 1;
            if self.row == self.batch.as_ref().unwrap().num_rows() {
                self.batch = None;
                self.reservation.free();
            }
        }
    }
}
