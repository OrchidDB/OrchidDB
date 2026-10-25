//! Typed, synchronous access to the caller's DuckDB transaction. Only an opaque
//! token crosses shared kernel state; borrowed C++ pointers stay on their thread.
use arrow::{
    array::{RecordBatch, RecordBatchIterator, RecordBatchReader},
    compute::concat_batches,
    datatypes::{Field, Schema},
    ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream},
};
use orchiddb::ir::rel::host::{HostRelational, HostRequest};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[repr(C)]
pub struct Relation {
    name: *const c_char,
    stream: *mut FFI_ArrowArrayStream,
    host_token: u64,
}
pub type Query = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *mut FFI_ArrowArrayStream,
    *mut Relation,
    usize,
    *mut FFI_ArrowArrayStream,
) -> *mut c_char;
pub type Free = unsafe extern "C" fn(*mut c_char);

#[derive(Clone, Copy)]
struct Callbacks {
    context: *mut c_void,
    query: Query,
    free: Free,
}
thread_local! { static ACTIVE: RefCell<HashMap<u64, Callbacks>> = RefCell::new(HashMap::new()); }
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct DuckDbHost {
    token: u64,
    cancelled: Arc<AtomicBool>,
}
pub struct HostScope {
    token: u64,
    previous: Option<Callbacks>,
    _thread: PhantomData<Rc<()>>,
}
impl DuckDbHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            token: NEXT.fetch_add(1, Ordering::Relaxed),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    /// Activate this execution token only for one synchronous kernel call.
    /// Callbacks/context must remain valid until the guard is dropped. DuckDB
    /// may schedule the next kernel on a different thread; no pointer survives.
    pub unsafe fn activate(
        &self,
        context: *mut c_void,
        query: Query,
        free: Free,
    ) -> Result<HostScope, String> {
        ACTIVE.with_borrow_mut(|active| {
            let previous = active.insert(
                self.token,
                Callbacks {
                    context,
                    query,
                    free,
                },
            );
            Ok(HostScope {
                token: self.token,
                previous,
                _thread: PhantomData,
            })
        })
    }
}
impl HostScope {
    /// Callbacks and context must remain valid until this scope is dropped.
    pub unsafe fn enter(context: *mut c_void, query: Query, free: Free) -> (Self, Arc<DuckDbHost>) {
        let host = DuckDbHost::new();
        let scope = unsafe { host.activate(context, query, free) }.expect("fresh token");
        (scope, host)
    }
}
impl Drop for HostScope {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|active| {
            if let Some(previous) = self.previous {
                active.insert(self.token, previous);
            } else {
                active.remove(&self.token);
            }
        });
    }
}

fn stream(batch: RecordBatch) -> FFI_ArrowArrayStream {
    let schema = batch.schema();
    FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema)))
}
impl HostRelational for DuckDbHost {
    fn query(&self, request: HostRequest) -> Result<RecordBatch, String> {
        let callbacks = ACTIVE
            .with_borrow(|active| active.get(&self.token).copied())
            .ok_or("DuckDB host execution scope has expired or belongs to another thread")?;
        let sql = CString::new(request.sql).map_err(|e| e.to_string())?;
        let mut parameters = if request.parameters.is_empty() {
            None
        } else {
            let fields = request
                .parameters
                .iter()
                .enumerate()
                .map(|(i, p)| Field::new((i + 1).to_string(), p.data_type(), true))
                .collect::<Vec<_>>();
            let arrays = request
                .parameters
                .iter()
                .map(|p| p.to_array_of_size(1).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            Some(stream(
                RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)
                    .map_err(|e| e.to_string())?,
            ))
        };
        let names = request
            .relations
            .iter()
            .map(|r| CString::new(r.name.as_str()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut streams = request
            .relations
            .into_iter()
            .map(|r| stream(r.batch))
            .collect::<Vec<_>>();
        let mut relations = names
            .iter()
            .zip(streams.iter_mut())
            .map(|(name, stream)| Relation {
                name: name.as_ptr(),
                stream,
                host_token: self.token,
            })
            .collect::<Vec<_>>();
        let mut output = FFI_ArrowArrayStream::empty();
        let error = unsafe {
            (callbacks.query)(
                callbacks.context,
                sql.as_ptr(),
                parameters.as_mut().map_or(std::ptr::null_mut(), |p| p),
                relations.as_mut_ptr(),
                relations.len(),
                &mut output,
            )
        };
        if !error.is_null() {
            let message = unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            unsafe {
                (callbacks.free)(error);
            }
            return Err(message);
        }
        // Import and consume while the caller context is live. concat_batches
        // retains the schema even when the host returns zero rows.
        let reader = ArrowArrayStreamReader::try_new(output).map_err(|e| e.to_string())?;
        let schema = reader.schema();
        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        concat_batches(&schema, &batches).map_err(|e| e.to_string())
    }
    fn execute(&self, request: HostRequest) -> Result<(), String> {
        self.query(request).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn never(
        _: *mut c_void,
        _: *const c_char,
        _: *mut FFI_ArrowArrayStream,
        _: *mut Relation,
        _: usize,
        _: *mut FFI_ArrowArrayStream,
    ) -> *mut c_char {
        panic!("expired/wrong-thread callback must not be invoked")
    }
    unsafe extern "C" fn free(_: *mut c_char) {}
    #[test]
    fn host_token_checks_thread_and_lifetime_before_callbacks() {
        let (scope, host) = unsafe { HostScope::enter(std::ptr::null_mut(), never, free) };
        let copy = host.clone();
        assert!(
            std::thread::spawn(move || copy.query(HostRequest::new("SELECT 1")).unwrap_err())
                .join()
                .unwrap()
                .contains("another thread")
        );
        drop(scope);
        assert!(
            host.query(HostRequest::new("SELECT 1"))
                .unwrap_err()
                .contains("expired")
        );
    }
}

/// Streaming host island. All pulls remain on the thread carrying an active
/// host scope; only remote IO is driven by the federation runtime's workers.
impl DuckDbHost {
    pub fn query_batches(
        &self,
        sql: &str,
        inputs: Vec<orchiddb::execution::stream::InputRelation>,
    ) -> Result<orchiddb::execution::stream::BatchStream, String> {
        let callbacks = ACTIVE
            .with_borrow(|active| active.get(&self.token).copied())
            .ok_or("DuckDB host execution scope has expired or belongs to another thread")?;
        let sql = CString::new(sql).map_err(|e| e.to_string())?;
        let names = inputs
            .iter()
            .map(|r| CString::new(r.name.as_str()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut streams = inputs
            .into_iter()
            .map(|r| {
                Ok(FFI_ArrowArrayStream::new(Box::new(BlockingBatches(
                    r.source.open()?,
                    tokio::runtime::Handle::try_current().ok(),
                    self.cancelled.clone(),
                ))))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut relations = names
            .iter()
            .zip(streams.iter_mut())
            .map(|(name, stream)| Relation {
                name: name.as_ptr(),
                stream,
                host_token: self.token,
            })
            .collect::<Vec<_>>();
        let mut output = FFI_ArrowArrayStream::empty();
        let error = unsafe {
            (callbacks.query)(
                callbacks.context,
                sql.as_ptr(),
                std::ptr::null_mut(),
                relations.as_mut_ptr(),
                relations.len(),
                &mut output,
            )
        };
        if !error.is_null() {
            let message = unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            unsafe { (callbacks.free)(error) };
            return Err(message);
        }
        let mut reader = ArrowArrayStreamReader::try_new(output).map_err(|e| e.to_string())?;
        let schema = reader.schema();
        let token = self.token;
        Ok(orchiddb::execution::stream::BatchStream::new(
            schema,
            futures::stream::poll_fn(move |_| {
                if !ACTIVE.with_borrow(|active| active.contains_key(&token)) {
                    return std::task::Poll::Ready(Some(Err(
                        "DuckDB host stream polled outside its caller scope".into(),
                    )));
                }
                std::task::Poll::Ready(reader.next().map(|b| b.map_err(|e| e.to_string())))
            }),
        ))
    }
}
type InterruptCallback = (*mut c_void, unsafe extern "C" fn(*mut c_void) -> bool);
thread_local! {static INTERRUPTED:RefCell<Option<InterruptCallback>>=const {RefCell::new(None)};}
pub struct CancellationScope(Option<InterruptCallback>, PhantomData<Rc<()>>);
impl CancellationScope {
    pub unsafe fn enter(
        context: *mut c_void,
        callback: unsafe extern "C" fn(*mut c_void) -> bool,
    ) -> Self {
        Self(
            INTERRUPTED.with_borrow_mut(|slot| slot.replace((context, callback))),
            PhantomData,
        )
    }
}
impl Drop for CancellationScope {
    fn drop(&mut self) {
        INTERRUPTED.with_borrow_mut(|slot| *slot = self.0)
    }
}
struct BlockingBatches(
    orchiddb::execution::stream::BatchStream,
    Option<tokio::runtime::Handle>,
    Arc<AtomicBool>,
);
impl Iterator for BlockingBatches {
    type Item = Result<RecordBatch, arrow::error::ArrowError>;
    fn next(&mut self) -> Option<Self::Item> {
        let _runtime = self.1.as_ref().map(|runtime| runtime.enter());
        use futures::Stream;
        struct Wake(std::thread::Thread);
        impl futures::task::ArcWake for Wake {
            fn wake_by_ref(w: &Arc<Self>) {
                w.0.unpark();
            }
        }
        let wake = Arc::new(Wake(std::thread::current()));
        let waker = futures::task::waker_ref(&wake);
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            // Result drop is a successful early stop, not a query interruption.
            if self.2.load(Ordering::Acquire) {
                return None;
            }
            if INTERRUPTED.with_borrow(|slot| {
                slot.is_some_and(|(context, callback)| unsafe { callback(context) })
            }) {
                return Some(Err(arrow::error::ArrowError::ComputeError(
                    "DuckDB federation query interrupted".into(),
                )));
            }
            match std::pin::Pin::new(&mut self.0).poll_next(&mut cx) {
                std::task::Poll::Ready(batch) => {
                    return batch.map(|b| b.map_err(arrow::error::ArrowError::ComputeError));
                }
                std::task::Poll::Pending => {
                    std::thread::park_timeout(std::time::Duration::from_millis(10))
                }
            }
        }
    }
}
impl RecordBatchReader for BlockingBatches {
    fn schema(&self) -> arrow::datatypes::SchemaRef {
        self.0.schema()
    }
}

// Each DuckDB Arrow scan may run on a different worker. C++ supplies its current
// live context for that pull; no saved context pointer crosses scheduling steps.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_host_scope_enter(
    token: u64,
    context: *mut c_void,
    query: Query,
    free: Free,
    interrupted: unsafe extern "C" fn(*mut c_void) -> bool,
) -> *mut c_void {
    let host = DuckDbHost {
        token,
        cancelled: Arc::new(AtomicBool::new(false)),
    };
    let scope = unsafe { host.activate(context, query, free) }.expect("host scope activation");
    let cancel = unsafe { CancellationScope::enter(context, interrupted) };
    Box::into_raw(Box::new((scope, cancel))).cast()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_host_scope_free(scope: *mut c_void) {
    if !scope.is_null() {
        drop(unsafe { Box::from_raw(scope.cast::<(HostScope, CancellationScope)>()) })
    }
}
