//! Orchid's transport scheduler. Relational operations execute only in SQL engines.
//!
//! DataFusion logical plans remain compiler IR; no DataFusion physical planner,
//! task context, runtime environment, or execution operator participates here.
use arrow::{array::RecordBatch, datatypes::SchemaRef};
use datafusion::common::{DataFusionError, Result};
use futures::{Stream, TryStreamExt};
use std::{
    any::Any,
    collections::BTreeMap,
    fmt,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

pub trait RecordBatchStream: Stream<Item = Result<RecordBatch>> {
    fn schema(&self) -> SchemaRef;
}
pub type SendableRecordBatchStream = Pin<Box<dyn RecordBatchStream + Send>>;
pub struct RecordBatchStreamAdapter<S> {
    schema: SchemaRef,
    inner: Pin<Box<S>>,
}
impl<S> RecordBatchStreamAdapter<S> {
    pub fn new(schema: SchemaRef, inner: S) -> Self {
        Self {
            schema,
            inner: Box::pin(inner),
        }
    }
}
impl<S: Stream<Item = Result<RecordBatch>>> Stream for RecordBatchStreamAdapter<S> {
    type Item = Result<RecordBatch>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
impl<S: Stream<Item = Result<RecordBatch>>> RecordBatchStream for RecordBatchStreamAdapter<S> {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}

#[derive(Debug, Clone)]
pub struct PlanProperties {
    schema: SchemaRef,
}
impl PlanProperties {
    pub fn new(schema: SchemaRef) -> Self {
        Self { schema }
    }
}

/// Only engine SQL operations and explicit language/transport boundaries implement this.
pub trait ExecutionPlan: fmt::Debug + Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> String {
        self.name().into()
    }
    fn as_any(&self) -> &dyn Any;
    fn properties(&self) -> &Arc<PlanProperties>;
    fn schema(&self) -> SchemaRef {
        self.properties().schema.clone()
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>>;
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>>;
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream>;
    fn metrics(&self) -> Option<MetricsSet> {
        None
    }
}

#[derive(Debug)]
pub struct TaskContext {
    batch_size: usize,
    pool: Arc<dyn MemoryPool>,
}
impl Default for TaskContext {
    fn default() -> Self {
        Self {
            batch_size: 1024,
            pool: Arc::new(GreedyMemoryPool::new(usize::MAX)),
        }
    }
}
impl TaskContext {
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.max(1);
        self
    }
    pub fn with_memory_pool(mut self, pool: Arc<dyn MemoryPool>) -> Self {
        self.pool = pool;
        self
    }
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
    pub fn memory_pool(&self) -> &Arc<dyn MemoryPool> {
        &self.pool
    }
}
pub fn execute_stream(
    plan: Arc<dyn ExecutionPlan>,
    context: Arc<TaskContext>,
) -> Result<SendableRecordBatchStream> {
    plan.execute(0, context)
}
/// Terminal compatibility API. Internal SQL exchanges consume streams directly.
pub async fn collect(
    plan: Arc<dyn ExecutionPlan>,
    context: Arc<TaskContext>,
) -> Result<Vec<RecordBatch>> {
    execute_stream(plan, context)?.try_collect().await
}
pub fn explain(plan: &Arc<dyn ExecutionPlan>) -> String {
    fn visit(plan: &Arc<dyn ExecutionPlan>, depth: usize, out: &mut String) {
        out.push_str(&format!("{}{}\n", "  ".repeat(depth), plan.description()));
        for child in plan.children() {
            visit(child, depth + 1, out);
        }
    }
    let mut out = String::new();
    visit(plan, 0, &mut out);
    out
}

/// Memory accounting belongs to the transport boundary, not a DataFusion runtime.
pub trait MemoryPool: fmt::Debug + Send + Sync {
    fn try_grow(&self, bytes: usize) -> Result<()>;
    fn shrink(&self, bytes: usize);
    fn reserved(&self) -> usize;
}
#[derive(Debug)]
pub struct GreedyMemoryPool {
    limit: usize,
    used: AtomicUsize,
}
impl GreedyMemoryPool {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
        }
    }
}
impl MemoryPool for GreedyMemoryPool {
    fn try_grow(&self, bytes: usize) -> Result<()> {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|total| *total <= self.limit)
            })
            .map(|_| ())
            .map_err(|_| {
                DataFusionError::ResourcesExhausted(format!(
                    "Orchid stream budget exceeded while reserving {bytes} bytes"
                ))
            })
    }
    fn shrink(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::AcqRel);
    }
    fn reserved(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}
pub struct MemoryConsumer;
impl MemoryConsumer {
    pub fn new(_: &str) -> Self {
        Self
    }
    pub fn register(self, pool: &Arc<dyn MemoryPool>) -> MemoryReservation {
        MemoryReservation {
            pool: pool.clone(),
            used: AtomicUsize::new(0),
        }
    }
}
pub struct MemoryReservation {
    pool: Arc<dyn MemoryPool>,
    used: AtomicUsize,
}
impl MemoryReservation {
    pub fn try_grow(&self, bytes: usize) -> Result<()> {
        self.pool.try_grow(bytes)?;
        self.used.fetch_add(bytes, Ordering::AcqRel);
        Ok(())
    }
    pub fn shrink(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::AcqRel);
        self.pool.shrink(bytes)
    }
    pub fn try_resize(&self, bytes: usize) -> Result<()> {
        let used = self.size();
        if bytes > used {
            self.try_grow(bytes - used)
        } else {
            self.shrink(used - bytes);
            Ok(())
        }
    }
    pub fn size(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
    pub fn free(&self) {
        let bytes = self.used.swap(0, Ordering::AcqRel);
        self.pool.shrink(bytes)
    }
}
impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.free()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Count(Arc<AtomicUsize>);
impl Count {
    pub fn add(&self, n: usize) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }
    pub fn set_max(&self, n: usize) {
        self.0.fetch_max(n, Ordering::Relaxed);
    }
    pub fn as_usize(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}
pub type Gauge = Count;
#[derive(Clone)]
pub struct Time(Count);
pub struct Timer {
    metric: Count,
    start: std::time::Instant,
}
impl Time {
    pub fn timer(&self) -> Timer {
        Timer {
            metric: self.0.clone(),
            start: std::time::Instant::now(),
        }
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        self.metric
            .add(self.start.elapsed().as_nanos().min(usize::MAX as u128) as usize)
    }
}
#[derive(Debug, Clone, Default)]
pub struct ExecutionPlanMetricsSet(Arc<Mutex<BTreeMap<String, Count>>>);
#[derive(Clone)]
pub struct MetricsSet(BTreeMap<String, Count>);
impl ExecutionPlanMetricsSet {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clone_inner(&self) -> MetricsSet {
        MetricsSet(self.0.lock().unwrap().clone())
    }
}
impl MetricsSet {
    pub fn sum_by_name(&self, name: &str) -> Option<Count> {
        self.0.get(name).cloned()
    }
    pub fn output_rows(&self) -> Option<usize> {
        self.sum_by_name("output_rows").map(|n| n.as_usize())
    }
}
pub struct MetricBuilder<'a>(&'a ExecutionPlanMetricsSet);
impl<'a> MetricBuilder<'a> {
    pub fn new(metrics: &'a ExecutionPlanMetricsSet) -> Self {
        Self(metrics)
    }
    pub fn counter(self, name: &str, _: usize) -> Count {
        self.0
            .0
            .lock()
            .unwrap()
            .entry(name.into())
            .or_default()
            .clone()
    }
    pub fn gauge(self, name: &str, partition: usize) -> Gauge {
        self.counter(name, partition)
    }
    pub fn subset_time(self, name: &str, partition: usize) -> Time {
        Time(self.counter(name, partition))
    }
    pub fn output_rows(self, partition: usize) -> Count {
        self.counter("output_rows", partition)
    }
    pub fn output_batches(self, partition: usize) -> Count {
        self.counter("output_batches", partition)
    }
    pub fn output_bytes(self, partition: usize) -> Count {
        self.counter("output_bytes", partition)
    }
}

#[derive(Debug)]
pub struct InputBatches {
    batches: Vec<RecordBatch>,
    properties: Arc<PlanProperties>,
}
impl InputBatches {
    pub fn new(schema: SchemaRef, batches: Vec<RecordBatch>) -> Self {
        Self {
            batches,
            properties: Arc::new(PlanProperties::new(schema)),
        }
    }
}
impl ExecutionPlan for InputBatches {
    fn name(&self) -> &str {
        "LiteralInput"
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
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if !children.is_empty() {
            return Err(DataFusionError::Plan(
                "literal input has no dependencies".into(),
            ));
        }
        Ok(self)
    }
    fn execute(&self, _: usize, _: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            futures::stream::iter(self.batches.clone().into_iter().map(Ok)),
        )))
    }
}
