//! Caller-owned DuckDB federation. The executor is pulled only inside an active
//! host scope. Tokio workers drive HTTP IO, never DuckDB callbacks.
use crate::host::{DuckDbHost, Free, Query};
use arrow::{
    array::RecordBatchIterator,
    datatypes::{Field, Schema},
    ffi_stream::FFI_ArrowArrayStream,
};
use futures::StreamExt;
use orchiddb::{
    compiler::CompiledSql,
    execution::stream::{BatchSource, BatchStream, InputRelation},
    federation::Session,
};
use std::{
    collections::BTreeMap,
    ffi::{CStr, CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

type Interrupted = unsafe extern "C" fn(*mut c_void) -> bool;
fn run<T>(
    runtime: &tokio::runtime::Runtime,
    context: *mut c_void,
    interrupted: Interrupted,
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    runtime.block_on(async {
        let mut future = std::pin::pin!(future);
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(10));
        futures::future::poll_fn(|cx| {
            if unsafe { interrupted(context) } {
                return std::task::Poll::Ready(Err("DuckDB federation query interrupted".into()));
            }
            let _ = tick.poll_tick(cx);
            future.as_mut().poll(cx)
        })
        .await
    })
}
struct HostSession(Arc<DuckDbHost>);
#[async_trait::async_trait(?Send)]
impl Session for HostSession {
    fn supports_concurrent_statements(&self) -> bool {
        true
    }
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query_stream(
        &mut self,
        sql: &str,
        inputs: Vec<InputRelation>,
    ) -> Result<BatchStream, String> {
        self.0.query_batches(sql, inputs)
    }
    async fn query_requests_stream(
        &mut self,
        queries: orchiddb::federation::QueryStream,
        columns: Vec<orchiddb::federation::TransferColumn>,
    ) -> Result<BatchStream, String> {
        let schema = Arc::new(Schema::new(
            columns
                .iter()
                .map(|column| {
                    Ok(Field::new(
                        &column.name,
                        orchiddb::compiler::data_type(&column.data_type)?,
                        column.nullable,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        ));
        let host = self.0.clone();
        let output = schema.clone();
        Ok(BatchStream::new(
            output,
            futures::stream::try_unfold(
                (queries, None::<BatchStream>, host, schema),
                |(mut queries, mut current, host, schema)| async move {
                    loop {
                        if let Some(stream) = &mut current {
                            if let Some(batch) = stream.next().await {
                                let batch = batch?;
                                if batch.num_columns() != schema.fields().len() {
                                    return Err("dependent host SQL column count mismatch".into());
                                }
                                let arrays = batch
                                    .columns()
                                    .iter()
                                    .zip(schema.fields())
                                    .map(|(array, field)| {
                                        if array.data_type() == field.data_type() {
                                            Ok(array.clone())
                                        } else {
                                            arrow::compute::cast(array, field.data_type())
                                                .map_err(|e| e.to_string())
                                        }
                                    })
                                    .collect::<Result<Vec<_>, String>>()?;
                                let batch = arrow::array::RecordBatch::try_new_with_options(
                                    schema.clone(),
                                    arrays,
                                    &arrow::record_batch::RecordBatchOptions::new()
                                        .with_row_count(Some(batch.num_rows())),
                                )
                                .map_err(|e| e.to_string())?;
                                return Ok(Some((batch, (queries, current, host, schema))));
                            }
                        }
                        drop(current.take());
                        match queries.next().await {
                            Some(query) => current = Some(host.query_batches(&query?, vec![])?),
                            None => return Ok(None),
                        }
                    }
                },
            ),
        ))
    }
}
struct Execution {
    // Drop streams (and their HTTP/caller query guards) before the IO runtime.
    stream: Option<BatchStream>,
    host: Arc<DuckDbHost>,
    runtime: tokio::runtime::Runtime,
}
fn ffi(f: impl FnOnce() -> Result<(), String>) -> *mut c_char {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(())) => std::ptr::null_mut(),
        other => CString::new(
            match other {
                Ok(Err(e)) => e,
                _ => "Federation bridge panicked".into(),
            }
            .replace('\0', " "),
        )
        .unwrap()
        .into_raw(),
    }
}
unsafe fn plan(input: *const c_char) -> Result<CompiledSql, String> {
    if input.is_null() {
        return Err("null federation plan".into());
    }
    serde_json::from_str(
        unsafe { CStr::from_ptr(input) }
            .to_str()
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
/// Bind with schema-only inputs; no remote operation or host scan is executed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_federation_schema(
    input: *const c_char,
    context: *mut c_void,
    query: Query,
    free: Free,
    output: *mut FFI_ArrowArrayStream,
) -> *mut c_char {
    ffi(|| {
        let plan = unsafe { plan(input) }?;
        let (_scope, host) = unsafe { crate::host::HostScope::enter(context, query, free) };
        let inputs = plan
            .transfers
            .iter()
            .map(|t| {
                let fields = t
                    .columns
                    .iter()
                    .map(|c| {
                        Ok(Field::new(
                            &c.name,
                            orchiddb::compiler::data_type(&c.data_type)?,
                            c.nullable,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let schema = Arc::new(Schema::new(fields));
                Ok(InputRelation {
                    name: t.target_relation.clone(),
                    source: BatchSource::once(BatchStream::new(schema, futures::stream::empty())),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let stream = host.query_batches(&plan.sql, inputs)?;
        unsafe {
            *output = FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
                std::iter::empty(),
                stream.schema(),
            )))
        };
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_federation_open(
    input: *const c_char,
    sources: *const c_char,
    context: *mut c_void,
    query: Query,
    free: Free,
    interrupted: Interrupted,
    output: *mut *mut c_void,
) -> *mut c_char {
    ffi(|| {
        let _cancel = unsafe { crate::host::CancellationScope::enter(context, interrupted) };
        let plan = unsafe { plan(input) }?;
        let options: serde_json::Value = serde_json::from_str(
            unsafe { CStr::from_ptr(sources) }
                .to_str()
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (_scope, host) = unsafe { crate::host::HostScope::enter(context, query, free) };
        let mut sessions: BTreeMap<String, Box<dyn Session>> = BTreeMap::new();
        sessions.insert(
            plan.execution_engine
                .clone()
                .ok_or("missing host execution engine")?,
            Box::new(HostSession(host.clone())),
        );
        let adapters = plan.transfers.iter().flat_map(|t| {
            std::iter::once((&t.source_engine, t.source_dialect.as_str())).chain(
                t.request
                    .iter()
                    .map(|r| (&r.engine, r.template.adapter.as_str())),
            )
        });
        for (id, adapter) in adapters {
            if sessions.contains_key(id) {
                continue;
            }
            if adapter == "duckdb" {
                return Err(format!(
                    "DuckDB island `{id}` is not the selected caller-owned host"
                ));
            }
            if adapter == "trino" {
                return Err("External Trino SQL islands require standalone engine sessions; the DuckDB extension uses its caller-owned SQL host".into());
            }
            let entry = options.get(id).ok_or_else(|| {
                format!("Remote source `{id}` has not been registered in this DuckDB session")
            })?;
            if entry["adapter"] != adapter {
                return Err(format!("Registered source adapter differs for `{id}`"));
            }
            #[cfg(any(
                feature = "quickwit",
                feature = "elasticsearch",
                feature = "weaviate",
                feature = "prometheus",
                feature = "loki"
            ))]
            sessions.insert(
                id.clone(),
                Box::new(orchiddb::remote::transport::HttpSession::from_json_options(
                    adapter,
                    entry["options"].clone(),
                )?),
            );
            #[cfg(not(any(
                feature = "quickwit",
                feature = "elasticsearch",
                feature = "weaviate",
                feature = "prometheus",
                feature = "loki"
            )))]
            return Err(format!(
                "Remote source `{adapter}` is not enabled in this extension build"
            ));
        }
        let stream = run(
            &runtime,
            context,
            interrupted,
            orchiddb::federation::execute_stream(&plan, &mut sessions),
        )?;
        unsafe {
            *output = Box::into_raw(Box::new(Execution {
                stream: Some(stream),
                host,
                runtime,
            }))
            .cast()
        };
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_federation_next(
    state: *mut c_void,
    context: *mut c_void,
    query: Query,
    free: Free,
    interrupted: Interrupted,
    output: *mut FFI_ArrowArrayStream,
    done: *mut bool,
) -> *mut c_char {
    ffi(|| {
        let _cancel = unsafe { crate::host::CancellationScope::enter(context, interrupted) };
        let state = unsafe { &mut *state.cast::<Execution>() };
        let _scope = unsafe { state.host.activate(context, query, free) }?;
        let stream = state
            .stream
            .as_mut()
            .ok_or("federation stream has finished")?;
        let batch = run(&state.runtime, context, interrupted, async {
            Ok(stream.next().await)
        })?;
        match batch {
            Some(batch) => {
                let batch = batch?;
                let schema = batch.schema();
                unsafe {
                    *output = FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
                        [Ok(batch)],
                        schema,
                    )));
                    *done = false
                }
            }
            None => {
                state.stream.take();
                unsafe { *done = true }
            }
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_federation_free(state: *mut c_void) {
    if !state.is_null() {
        let state = unsafe { Box::from_raw(state.cast::<Execution>()) };
        state.host.cancel();
        drop(state);
    }
}
