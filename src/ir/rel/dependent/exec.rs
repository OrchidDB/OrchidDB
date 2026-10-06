//! Execute dependent relational operations through an engine-owned SQL session.
use super::DependentOperation;
use crate::ir::rel::{dag::coerce_sql_array, sql};
use arrow::{
    array::{RecordBatch, RecordBatchOptions},
    datatypes::SchemaRef,
};
use datafusion::{
    common::{DataFusionError, Result, ScalarValue},
    execution::{
        TaskContext,
        memory_pool::{MemoryConsumer, MemoryReservation},
    },
    physical_expr::EquivalenceProperties,
    physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
        PlanProperties, SendableRecordBatchStream,
        execution_plan::EmissionType,
        metrics::{Count, ExecutionPlanMetricsSet, Gauge, MetricBuilder, MetricsSet, Time},
        stream::RecordBatchStreamAdapter,
    },
};
use futures::{TryStreamExt, stream};
use std::{
    any::Any,
    collections::VecDeque,
    fmt,
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
        let boundedness = source.boundedness();
        Self {
            source,
            template: operation.template.clone(),
            session,
            cost,
            properties: Arc::new(PlanProperties::new(
                EquivalenceProperties::new(Arc::new(operation.schema.as_arrow().clone())),
                Partitioning::UnknownPartitioning(1),
                EmissionType::Incremental,
                boundedness,
            )),
            metrics: ExecutionPlanMetricsSet::new(),
        }
    }
}
impl DisplayAs for DependentOperationExec {
    fn fmt_as(&self, _: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DependentOperationExec: {}", self.template.sql)
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
            properties: Arc::new(PlanProperties::new(
                EquivalenceProperties::new(self.schema()),
                Partitioning::UnknownPartitioning(1),
                EmissionType::Incremental,
                children[0].boundedness(),
            )),
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
            input: datafusion::physical_plan::execute_stream(self.source.clone(), context.clone())?,
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
            pending: VecDeque::new(),
            quantum: context.session_config().batch_size().clamp(1, 64),
            metrics: BoundaryMetrics::new(&self.metrics),
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            stream::try_unfold(cursor, Cursor::next),
        )))
    }
}

// Per-operator measurements, available through DataFusion's metrics/explain API.
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

/// Return the first occurrence immediately, then amortize blocking scheduling
/// over bounded groups within the current source batch. A group stops at 64
/// inputs or 1 MiB of output (one driver result may exceed that soft threshold).
/// Dropping the stream cancels the group between requests; an already-running
/// driver call may finish. No session mutex is held while waiting for demand.
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
    pending: VecDeque<RecordBatch>,
    quantum: usize,
    metrics: BoundaryMetrics,
}
impl Drop for Cursor {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Cursor {
    fn take_output(&mut self) -> RecordBatch {
        let batch = self.pending.pop_front().expect("completed occurrence");
        let bytes = batch.get_array_memory_size();
        self.output_reservation.shrink(bytes);
        self.metrics.output_rows.add(batch.num_rows());
        self.metrics.output_batches.add(1);
        self.metrics.output_bytes.add(bytes);
        self.metrics.peak_output_batch_bytes.set_max(bytes);
        batch
    }
    async fn next(mut self) -> Result<Option<(RecordBatch, Self)>> {
        if !self.pending.is_empty() {
            let batch = self.take_output();
            return Ok(Some((batch, self)));
        }
        if !self.checked_dialect {
            let session = self.session.clone();
            let dialect = self.template.dialect.clone();
            tokio::task::spawn_blocking(move || {
                if session
                    .lock()
                    .map_err(|_| failure("SQL session poisoned"))?
                    .dialect()
                    .name()
                    != dialect
                {
                    return Err(failure("dependent operation session dialect mismatch"));
                }
                Ok(())
            })
            .await
            .map_err(failure)??;
            self.checked_dialect = true;
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
        // A slice shares the input buffers; no copy or full-input collection.
        let batch = self.batch.as_ref().unwrap();
        let count = if self.prepared.is_none() {
            1
        } else {
            self.quantum
        };
        let input = batch.slice(self.row, count.min(batch.num_rows() - self.row));
        let template = self.template.clone();
        let prepared = self.prepared.clone();
        let session = self.session.clone();
        let cost = self.cost.clone();
        let schema = self.schema.clone();
        let metrics = self.metrics.clone();
        let cancelled = self.cancelled.clone();
        let input_reservation = self.reservation.clone();
        let output_reservation = self.output_reservation.clone();
        let (pending, prepared, consumed) = tokio::task::spawn_blocking(move || {
            // Keep the reservation alive even if the waiting future is dropped.
            let _input_reservation = input_reservation;
            let prepared = match prepared {
                Some(prepared) => prepared,
                None => {
                    let _timer = metrics.binding_time.timer();
                    let prepared = Arc::new(template.prepare().map_err(failure)?);
                    metrics.template_preparations.add(1);
                    prepared
                }
            };
            let mut pending = VecDeque::new();
            for row in 0..input.num_rows() {
                let query = {
                    let _timer = metrics.binding_time.timer();
                    let values = input
                        .columns()
                        .iter()
                        .map(|a| ScalarValue::try_from_array(a.as_ref(), row))
                        .collect::<Result<Vec<_>>>()?;
                    prepared.bind(&values).map_err(failure)?
                };
                let returned = {
                    let mut session = session
                        .lock()
                        .map_err(|_| failure("SQL session poisoned"))?;
                    if cancelled.load(Ordering::Acquire) {
                        return Err(failure("dependent execution cancelled"));
                    }
                    metrics.input_rows.add(1);
                    metrics.sql_requests.add(1);
                    let _timer = metrics.sql_time.timer();
                    session.query(&query, schema.clone()).map_err(|error| {
                        metrics.sql_errors.add(1);
                        failure(error)
                    })?
                };
                if returned.num_columns() != schema.fields().len() {
                    return Err(failure("dependent operation output column count mismatch"));
                }
                if returned
                    .columns()
                    .iter()
                    .zip(schema.fields())
                    .any(|(array, field)| !field.is_nullable() && array.null_count() != 0)
                {
                    return Err(failure("NULL in non-nullable dependent operation output"));
                }
                let arrays = returned
                    .columns()
                    .iter()
                    .zip(schema.fields())
                    .map(|(a, f)| coerce_sql_array(a, f.data_type()))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let returned = RecordBatch::try_new_with_options(
                    schema.clone(),
                    arrays,
                    &RecordBatchOptions::new().with_row_count(Some(returned.num_rows())),
                )?;
                let rows = returned.num_rows();
                let bytes = returned.get_array_memory_size();
                {
                    let mut cost = cost.lock().map_err(|_| failure("query cost poisoned"))?;
                    cost.sql_executions = cost.sql_executions.saturating_add(1);
                    cost.sql_output_rows = cost.sql_output_rows.saturating_add(rows as u64);
                    cost.sql_output_bytes = cost.sql_output_bytes.saturating_add(bytes as u64);
                }
                output_reservation.try_grow(bytes)?;
                metrics
                    .peak_buffered_output_bytes
                    .set_max(output_reservation.size());
                pending.push_back(returned);
                if output_reservation.size() >= 1024 * 1024 {
                    break;
                }
            }
            let consumed = pending.len();
            Ok::<_, DataFusionError>((pending, prepared, consumed))
        })
        .await
        .map_err(failure)??;
        self.prepared = Some(prepared);
        self.pending = pending;
        self.row += consumed;
        if self.row == self.batch.as_ref().unwrap().num_rows() {
            self.batch = None;
            self.reservation.free();
        }
        let batch = self.take_output();
        Ok(Some((batch, self)))
    }
}
