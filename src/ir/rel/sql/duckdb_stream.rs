//! Bounded Arrow exchange for the embedded executable engine.
use super::{DuckDbExecutor, PreparedSql};
use crate::execution::stream::{BatchSource, BatchStream, InputRelation};
use arrow::{array::RecordBatch, datatypes::SchemaRef};
use duckdb::{
    core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId},
    vtab::{BindInfo, InitInfo, TableFunctionInfo, VTab},
};
use futures::StreamExt;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

static NEXT: AtomicU64 = AtomicU64::new(1);
pub(super) struct Cancellation {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
    interrupt: Mutex<Option<Arc<duckdb::InterruptHandle>>>,
}
impl Cancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
        if let Some(handle) = self.interrupt.lock().unwrap().as_ref() {
            handle.interrupt();
        }
    }
}
struct Output {
    rx: tokio::sync::mpsc::Receiver<Result<RecordBatch, String>>,
    cancel: Arc<Cancellation>,
}
impl futures::Stream for Output {
    type Item = Result<RecordBatch, String>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
struct Deadline(Option<std::sync::mpsc::Sender<()>>);
impl Drop for Deadline {
    fn drop(&mut self) {
        if let Some(done) = self.0.take() {
            let _ = done.send(());
        }
    }
}
struct Pending(Option<Arc<Cancellation>>);
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(cancel) = self.0.take() {
            cancel.cancel();
        }
    }
}
struct WorkerDone(Arc<Cancellation>);
impl Drop for WorkerDone {
    fn drop(&mut self) {
        self.0.interrupt.lock().unwrap().take();
    }
}
type Entry = (BatchSource, Arc<Mutex<Option<String>>>, Arc<Cancellation>);
fn registry() -> &'static Mutex<BTreeMap<String, Entry>> {
    static ITEMS: OnceLock<Mutex<BTreeMap<String, Entry>>> = OnceLock::new();
    ITEMS.get_or_init(Default::default)
}
pub(super) struct Scan;
pub(super) struct Cursor {
    stream: Option<BatchStream>,
    batch: Option<RecordBatch>,
    offset: usize,
}
impl VTab for Scan {
    type BindData = Entry;
    type InitData = Mutex<Cursor>;
    fn bind(info: &BindInfo) -> Result<Entry, Box<dyn std::error::Error>> {
        let entry = registry()
            .lock()
            .map_err(|_| "stream registry poisoned")?
            .get(&info.get_parameter(0).to_string())
            .cloned()
            .ok_or("expired stream relation")?;
        if let Some(rows) = entry.0.estimated_rows() {
            info.set_cardinality(rows, false);
        }
        for field in entry.0.schema().fields() {
            info.add_result_column(
                field.name(),
                duckdb::vtab::arrow::to_duckdb_logical_type(field.data_type())?,
            );
        }
        Ok(entry)
    }
    fn init(info: &InitInfo) -> Result<Self::InitData, Box<dyn std::error::Error>> {
        info.set_max_threads(1);
        Ok(Mutex::new(Cursor {
            stream: None,
            batch: None,
            offset: 0,
        }))
    }
    fn parameters() -> Option<Vec<LogicalTypeHandle>> {
        Some(vec![LogicalTypeHandle::from(LogicalTypeId::Varchar)])
    }
    fn func(
        info: &TableFunctionInfo<Self>,
        out: &mut DataChunkHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = info.get_bind_data();
        let mut cursor = info
            .get_init_data()
            .lock()
            .map_err(|_| "stream cursor poisoned")?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
            if cursor.stream.is_none() {
                cursor.stream = Some(source.0.open()?);
            }
            loop {
                if let Some(batch) = &cursor.batch {
                    if cursor.offset < batch.num_rows() {
                        let count = (batch.num_rows() - cursor.offset).min(2048);
                        let slice = batch.slice(cursor.offset, count);
                        cursor.offset += count;
                        return duckdb::vtab::arrow::record_batch_to_duckdb_data_chunk(&slice, out)
                            .map_err(|e| e.to_string());
                    }
                }
                cursor.batch = None;
                let next = futures::executor::block_on(async {
                    let cancelled = source.2.notify.notified();
                    if source.2.cancelled.load(Ordering::Acquire) {
                        return Some(Err("DuckDB input cancelled".into()));
                    }
                    tokio::select! {batch=cursor.stream.as_mut().unwrap().next()=>batch,_=cancelled=>Some(Err("DuckDB input cancelled".into()))}
                });
                match next {
                    Some(Ok(batch)) => {
                        cursor.batch = Some(batch);
                        cursor.offset = 0;
                    }
                    Some(Err(error)) => return Err(error),
                    None => {
                        out.set_len(0);
                        return Ok(());
                    }
                }
            }
        })).unwrap_or_else(|_|Err("Arrow input producer panicked".into()));
        if let Err(error) = &result {
            *source.1.lock().unwrap() = Some(error.clone());
        }
        result.map_err(Into::into)
    }
}

/// Caller-owned DuckDB session with bounded output and Arrow input relations.
#[derive(Debug, Clone)]
pub struct DuckDbSession {
    pub(crate) executor: Arc<Mutex<DuckDbExecutor>>,
}
impl Default for DuckDbSession {
    fn default() -> Self {
        Self::new(DuckDbExecutor::new())
    }
}
impl DuckDbSession {
    pub fn new(executor: DuckDbExecutor) -> Self {
        Self {
            executor: Arc::new(Mutex::new(executor)),
        }
    }
    pub(crate) async fn prepared(
        &self,
        prepared: PreparedSql,
        inputs: Vec<InputRelation>,
        expected: Option<SchemaRef>,
    ) -> Result<BatchStream, String> {
        let executor = self.executor.clone();
        let cancellation = Arc::new(Cancellation {
            cancelled: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
            interrupt: Mutex::new(None),
        });
        let cancel = cancellation.clone();
        let mut pending = Pending(Some(cancel.clone()));
        let (schema_tx, schema_rx) = tokio::sync::oneshot::channel();
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let runtime = tokio::runtime::Handle::try_current().ok();
        std::thread::Builder::new()
            .name("orchid-duckdb-stream".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let _entered = runtime.as_ref().map(|r| r.enter());
                let mut schema_tx = Some(schema_tx);
                let mut keys = Vec::new();
                let mut views = Vec::new();
                let error = Arc::new(Mutex::new(None));
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || -> Result<(), String> {
                        let mut executor =
                            executor.lock().map_err(|_| "DuckDB session poisoned")?;
                        let _done = WorkerDone(cancel.clone());
                        *cancel.interrupt.lock().unwrap() = Some(
                            executor
                                .source_query_connection()
                                .map_err(|e| e.to_string())?
                                .interrupt_handle(),
                        );
                        executor.prepare_stream_setup(&prepared)?;
                        let timeout = executor.query_timeout();
                        let conn = executor
                            .source_query_connection()
                            .map_err(|e| e.to_string())?;
                        *cancel.interrupt.lock().unwrap() = Some(conn.interrupt_handle());
                        if cancel.cancelled.load(Ordering::Acquire) {
                            return Err("DuckDB query cancelled".into());
                        }
                        let _deadline = if let Some(timeout) = timeout {
                            let (done, wait) = std::sync::mpsc::channel();
                            let cancel = cancel.clone();
                            std::thread::spawn(move || {
                                if matches!(
                                    wait.recv_timeout(timeout),
                                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                                ) {
                                    cancel.cancel();
                                }
                            });
                            Deadline(Some(done))
                        } else {
                            Deadline(None)
                        };
                        let run = (|| -> Result<(), String> {
                            for input in inputs {
                                let key = NEXT.fetch_add(1, Ordering::Relaxed).to_string();
                                registry()
                                    .lock()
                                    .map_err(|_| "stream registry poisoned")?
                                    .insert(
                                        key.clone(),
                                        (input.source, error.clone(), cancel.clone()),
                                    );
                                keys.push(key.clone());
                                let name = super::SqlDialect::DuckDb.quote_ident(&input.name);
                                conn.execute_batch(&format!(
                            "CREATE TEMP VIEW {name} AS SELECT * FROM __orchid_stream_scan('{key}')"
                        ))
                        .map_err(|e| e.to_string())?;
                                views.push(name);
                            }
                            let mut shape = conn
                                .prepare(&format!(
                                    "SELECT * FROM ({}) AS __orchid_schema LIMIT 0",
                                    prepared.query
                                ))
                                .map_err(|e| e.to_string())?;
                            let physical = shape
                                .query_arrow([])
                                .map_err(|e| e.to_string())?
                                .get_schema();
                            drop(shape);
                            let schema = expected.unwrap_or_else(|| physical.clone());
                            if schema.fields().len() != physical.fields().len() {
                                return Err("DuckDB output column count mismatch".into());
                            }
                            if schema_tx.take().unwrap().send(Ok(schema.clone())).is_err() {
                                return Ok(());
                            }
                            let mut statement =
                                conn.prepare(&prepared.query).map_err(|e| e.to_string())?;
                            let mut reader = statement
                                .stream_arrow([], physical)
                                .map_err(|e| e.to_string())?;
                            while let Some(batch) =
                                reader.next_result().map_err(|e| e.to_string())?
                            {
                                let columns = batch
                                    .columns()
                                    .iter()
                                    .zip(schema.fields())
                                    .map(|(a, f)| {
                                        super::super::dag::coerce_sql_array(a, f.data_type())
                                    })
                                    .collect::<Result<Vec<_>, _>>()
                                    .map_err(|e| e.to_string())?;
                                let batch = RecordBatch::try_new_with_options(
                                    schema.clone(),
                                    columns,
                                    &arrow::record_batch::RecordBatchOptions::new()
                                        .with_row_count(Some(batch.num_rows())),
                                )
                                .map_err(|e| e.to_string())?;
                                if tx.blocking_send(Ok(batch)).is_err() {
                                    return Ok(());
                                }
                            }
                            if let Some(error) = error.lock().unwrap().take() {
                                return Err(error);
                            }
                            Ok(())
                        })();
                        for name in &views {
                            let _ = conn.execute_batch(&format!("DROP VIEW {name}"));
                        }
                        let cleanup = executor.finish_stream_setup();
                        run.and(cleanup)
                    },
                ))
                .unwrap_or_else(|_| Err("DuckDB streaming worker panicked".into()));
                for key in keys {
                    if let Ok(mut registry) = registry().lock() {
                        registry.remove(&key);
                    }
                }
                if let Err(error) = result {
                    if let Some(schema_tx) = schema_tx {
                        let _ = schema_tx.send(Err(error));
                    } else {
                        let _ = tx.blocking_send(Err(error));
                    }
                }
            })
            .map_err(|e| format!("start DuckDB stream worker: {e}"))?;
        let schema = schema_rx
            .await
            .map_err(|_| "DuckDB worker stopped before publishing schema")??;
        pending.0 = None;
        Ok(BatchStream::new(
            schema,
            Output {
                rx,
                cancel: cancellation,
            },
        ))
    }
}
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for DuckDbSession {
    fn concurrency_key(&self)->Option<String>{Some(format!("duckdb:{:p}",Arc::as_ptr(&self.executor)))}
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query_requests_stream(
        &mut self,
        queries: crate::federation::QueryStream,
        columns: Vec<crate::federation::TransferColumn>,
    ) -> Result<BatchStream, String> {
        let schema = crate::federation::transfer_schema(&columns)?;
        let stream = futures::stream::try_unfold(
            (self.clone(), queries, None::<BatchStream>),
            move |(session, mut queries, mut active)| {
                let columns = columns.clone();
                async move {
                    loop {
                        if let Some(output) = &mut active {
                            if let Some(batch) = output.next().await {
                                return batch
                                    .map(|batch| Some((batch, (session, queries, active))));
                            }
                            active = None;
                        }
                        let Some(query) = queries.next().await else {
                            return Ok(None);
                        };
                        let stream = session
                            .prepared(
                                PreparedSql::query_only(
                                    query?,
                                    Arc::new(arrow::datatypes::Schema::empty()),
                                ),
                                vec![],
                                None,
                            )
                            .await?;
                        active = Some(crate::federation::normalize_transfer_stream(
                            stream, &columns,
                        )?);
                    }
                }
            },
        );
        Ok(BatchStream::new(schema, stream))
    }
    async fn query_stream(
        &mut self,
        sql: &str,
        inputs: Vec<InputRelation>,
    ) -> Result<BatchStream, String> {
        self.prepared(
            PreparedSql::query_only(sql.into(), Arc::new(arrow::datatypes::Schema::empty())),
            inputs,
            None,
        )
        .await
    }
}

impl super::region::RegionSession for DuckDbSession {
    fn dialect(&self) -> super::SqlDialect {
        super::SqlDialect::DuckDb
    }
    fn validate_float_helper(
        &self,
        required: &super::super::language_functions::FloatHelperCapability,
    ) -> super::SqlResult<()> {
        let executor = self
            .executor
            .lock()
            .map_err(|_| super::SqlError::Execution("DuckDB session poisoned".into()))?;
        super::SqlExecutor::validate_float_helper(&*executor, required)
    }
    fn query_stream(
        &mut self,
        sql: &str,
        schema: SchemaRef,
        inputs: Vec<InputRelation>,
    ) -> super::SqlResult<BatchStream> {
        futures::executor::block_on(self.prepared(
            PreparedSql::query_only(sql.into(), schema.clone()),
            inputs,
            Some(schema),
        ))
        .map_err(super::SqlError::Execution)
    }
    fn query(&mut self, sql: &str, schema: SchemaRef) -> super::SqlResult<RecordBatch> {
        let stream = super::region::RegionSession::query_stream(self, sql, schema.clone(), vec![])?;
        let batches = futures::executor::block_on(stream.collect_batches())
            .map_err(super::SqlError::Execution)?;
        arrow::compute::concat_batches(&schema, &batches).map_err(Into::into)
    }
}
