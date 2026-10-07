//! DuckDB physical-operator ABI for the shared compiled kernel DAG. There is
//! no program scheduler here: DuckDB binds every descriptor and invokes it.
use crate::host::{DuckDbHost, Free, HostScope, Query};
use arrow::{
    array::{RecordBatch, RecordBatchIterator, RecordBatchReader},
    compute::concat_batches,
    ffi::FFI_ArrowSchema,
    ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream},
};
use orchiddb::{
    compiler::{CompileRequest, PreparedGraphBindings},
    ir::{
        diagnostics::QueryExecutionError,
        functions::with_operator_table,
        plan::Node,
        policy::ResultForm,
        rel::{
            host::mapped_storage,
            native_values,
            runtime::{
                CompileOptions, KernelState, SubplanRunner, compile_for_host,
                host::{self as kernel_host, KernelQueryScope, KernelSourceCursor, LogicalPlan},
                program::{CompiledProgram, StageOperation},
            },
        },
        runtime::{IrResult, Row, RuntimeError},
    },
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    fmt,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// The program and child state are borrowed only for this synchronous callback.
/// DuckDB must use the supplied child state for all nested kernel invocations.
pub type Nested = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    *mut c_void,
    *mut FFI_ArrowArrayStream,
) -> *mut c_char;
#[derive(Clone, Copy)]
struct Callback {
    context: *mut c_void,
    nested: Nested,
    free: Free,
}
thread_local! { static ACTIVE: RefCell<HashMap<u64, Callback>> = RefCell::new(HashMap::new()); }
static NEXT: AtomicU64 = AtomicU64::new(1);

struct Program {
    compiled: CompiledProgram,
    prepared: Arc<PreparedGraphBindings>,
    fields: Vec<String>,
    result_form: ResultForm,
    policy: orchiddb::ir::policy::GraphPlanPolicy,
    mutating: bool,
}
impl Program {
    fn new(mut request: CompileRequest) -> Result<Self, QueryExecutionError> {
        if request.dialect != "duckdb"
            || !request.engines.is_empty()
            || !matches!(request.language.as_str(), "cypher" | "gremlin")
        {
            return Err("Compiled graph programs require local DuckDB Cypher or Gremlin".into());
        }
        request.native_values = true;
        let prepared = Arc::new(orchiddb::compiler::prepare_graph(&request)?);
        orchiddb::ir::jvm::validate_computer_plan(&prepared.plan.root)?;
        let computer = orchiddb::ir::jvm::contains_computer(&prepared.plan.root);
        let mutating = !computer && orchiddb::ir::exec::contains_mutation(&prepared.plan.root);
        // Computed-edge writes must retain source-backed element identities and
        // use the shared catalog's read-only enforcement. Partial SQL islands
        // do not preserve that contract across mutation operators.
        let computed_mutation = mutating && !request.computed_relationships.is_empty();
        let logical = with_operator_table(prepared.operators.clone(), || {
            compile_for_host(
                &prepared.plan,
                &prepared.graph,
                CompileOptions {
                    sql_islands: prepared.managed_table.is_none() && !computed_mutation,
                    ..Default::default()
                },
            )
        })?;
        let compiled = CompiledProgram::new(&logical)?;
        // UNION aligns its branches to the left return contract in the existing planner.
        let mut result_node = prepared.plan.root.as_ref();
        while let Node::GraphUnion { left, .. } = result_node { result_node = left; }
        let (fields, result_form) = match result_node {
            Node::GraphReturn {
                fields,
                result_form,
                ..
            } => (fields.clone(), *result_form),
            Node::GraphAsk { field, .. } => (vec![field.clone()], ResultForm::Boolean),
            _ => (vec![], ResultForm::RowSet),
        };
        let policy = prepared.plan.policy.clone();
        let prepared = Arc::new(prepared.bindings());
        Ok(Self {
            compiled,
            prepared,
            fields,
            result_form,
            policy,
            mutating,
        })
    }
    fn result_fields(&self) -> Vec<String> {
        if self.fields.is_empty() {
            vec!["__orchid_void".into()]
        } else {
            self.fields.clone()
        }
    }
    fn metadata(&self) -> Value {
        json!({"manifest":self.compiled.manifest(),"fields":self.fields,"result_form":format!("{:?}",self.result_form),
            "mutating":self.mutating,"targets":if self.mutating {self.prepared.managed_table.as_ref().map(|table|vec![table.clone()]).unwrap_or_else(||self.prepared.mapping.physical_table_names().into_iter().collect())} else {Vec::new()}})
    }
}

pub fn compile_request_json(input: Value) -> Result<Value, String> {
    let request: CompileRequest =
        serde_json::from_value(input.clone()).map_err(|e| e.to_string())?;
    let program = Program::new(request).map_err(|e| e.to_string())?;
    let query = input.to_string().replace('\'', "''");
    Ok(
        json!({"version":1,"dialect":"duckdb","program_request":input,"sql":format!("SELECT * FROM __orchid_program('{query}')"),
        "fields":program.fields,"field_types":vec![Value::Null;program.fields.len()],
        "result_form":format!("{:?}",program.result_form),"logical_plan":program.compiled.manifest().to_string(),
        "transfers":[],"execution_engine":null,"constraint_proofs":[],"layout_selections":[],"representation_selections":[],
        "plan_estimates":[],"statistics_usage":null,"optimizer_decisions":[]}),
    )
}

struct Services {
    host: Arc<DuckDbHost>,
    token: u64,
    prepared: Arc<PreparedGraphBindings>,
    policy: orchiddb::ir::policy::GraphPlanPolicy,
    programs: std::sync::Mutex<HashMap<LogicalPlan, Arc<Program>>>,
}
impl fmt::Debug for Services {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DuckDbSubplanRunner")
            .field("token", &self.token)
            .finish()
    }
}
impl SubplanRunner for Services {
    fn run(&self, plan: &LogicalPlan, state: &mut KernelState) -> IrResult<Vec<Row>> {
        let callback = ACTIVE
            .with_borrow(|active| active.get(&self.token).copied())
            .ok_or_else(|| {
                RuntimeError::Runtime(
                    "DuckDB nested execution scope has expired or belongs to another thread".into(),
                )
            })?;
        // Cache immutable descriptors within this statement, as the legacy
        // runner caches its prepared subplans. Never cache frontier or rows.
        let cached = self.programs.lock().map_err(|_| RuntimeError::Runtime("Subplan cache poisoned".into()))?
            .get(plan).cloned();
        let nested = match cached {
            Some(program) => program,
            None => {
                let program = Arc::new(Program {
                    compiled: CompiledProgram::new(plan).map_err(runtime_error)?,
                    prepared: self.prepared.clone(), fields: vec![],
                    result_form: ResultForm::RowSet, policy: self.policy.clone(), mutating: false,
                });
                self.programs.lock().map_err(|_| RuntimeError::Runtime("Subplan cache poisoned".into()))?
                    .insert(plan.clone(), program.clone());
                program
            }
        };
        let mut output = FFI_ArrowArrayStream::empty();
        let error = unsafe {
            (callback.nested)(
                callback.context,
                Arc::as_ptr(&nested) as *const c_void,
                state as *mut KernelState as *mut c_void,
                &mut output,
            )
        };
        if !error.is_null() {
            let message = unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            unsafe {
                (callback.free)(error);
            }
            return Err(nested_error(message));
        }
        let batch = import_owned(output).map_err(runtime_error)?;
        kernel_host::decode_rows(&batch).map_err(runtime_error)
    }
}
fn nested_error(message: String) -> RuntimeError {
    #[derive(serde::Deserialize)]
    struct Failure {
        error: String,
        #[serde(default)]
        diagnosis: Option<orchiddb::ir::diagnostics::RuntimeDiagnosis>,
    }
    match serde_json::from_str::<Failure>(&message) {
        Ok(error) => runtime_error(QueryExecutionError {
            message: error.error,
            diagnosis: error.diagnosis,
        }),
        Err(_) => RuntimeError::Runtime(message),
    }
}
fn runtime_error(error: QueryExecutionError) -> RuntimeError {
    match error.diagnosis {
        Some(code) => RuntimeError::Diagnosed {
            code,
            message: error.message,
        },
        None => RuntimeError::Runtime(error.message),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use orchiddb::ir::diagnostics::RuntimeDiagnosis;

    #[test]
    fn nested_callback_retains_authoritative_diagnosis() {
        let error = QueryExecutionError {
            message: "deleted entity".into(),
            diagnosis: Some(RuntimeDiagnosis::DeletedEntityAccess),
        };
        let payload = json!({"error": error.message, "diagnosis": error.diagnosis}).to_string();
        let restored = QueryExecutionError::from_error(nested_error(payload));
        assert_eq!(restored.message, "deleted entity");
        assert_eq!(restored.diagnosis, error.diagnosis);
        assert!(
            matches!(nested_error("host exception".into()), RuntimeError::Runtime(message) if message == "host exception")
        );
    }
}
struct SourceCursor {
    cursor: KernelSourceCursor,
    prepared: Arc<PreparedGraphBindings>,
}
struct Execution {
    state: KernelState,
    services: Arc<Services>,
    scope: KernelQueryScope,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    finished: bool,
}
struct Scope {
    _host: HostScope,
    _functions: orchiddb::ir::functions::host_execution::Scope,
    _catalog: crate::catalog::Scope,
    token: u64,
    previous: Option<Callback>,
    _thread: PhantomData<Rc<()>>,
}
impl Scope {
    unsafe fn enter(
        services: &Services,
        context: *mut c_void,
        query: Query,
        free: Free,
        nested: Nested,
        catalog: crate::catalog::Query,
    ) -> Result<Self, QueryExecutionError> {
        let host = unsafe { services.host.activate(context, query, free) }?;
        let previous = ACTIVE.with_borrow_mut(|active| {
            active.insert(
                services.token,
                Callback {
                    context,
                    nested,
                    free,
                },
            )
        });
        Ok(Self {
            _host: host,
            _functions: orchiddb::ir::functions::host_execution::enter(services.host.clone()),
            _catalog: unsafe { crate::catalog::enter(context, catalog, free) },
            token: services.token,
            previous,
            _thread: PhantomData,
        })
    }
}
impl Drop for Scope {
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

fn export(batch: RecordBatch) -> FFI_ArrowArrayStream {
    let schema = batch.schema();
    FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema)))
}
fn import_owned(stream: FFI_ArrowArrayStream) -> Result<RecordBatch, QueryExecutionError> {
    let reader =
        ArrowArrayStreamReader::try_new(stream).map_err(QueryExecutionError::from_error)?;
    let schema = reader.schema();
    let batches = reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(QueryExecutionError::from_error)?;
    concat_batches(&schema, &batches).map_err(QueryExecutionError::from_error)
}
unsafe fn import(input: *mut FFI_ArrowArrayStream) -> Result<RecordBatch, QueryExecutionError> {
    if input.is_null() {
        return Err("Missing host Arrow input".into());
    }
    import_owned(unsafe { std::ptr::replace(input, FFI_ArrowArrayStream::empty()) })
}
fn message(error: QueryExecutionError) -> Value {
    json!({"error":error.message,"diagnosis":error.diagnosis,"classification":error.diagnosis.map(|diagnosis| {
        let (kind, detail, phase) = diagnosis.classification(); json!({"type":kind,"detail":detail,"phase":phase})
    })})
}
fn allocation(text: String) -> *mut c_char {
    CString::new(text)
        .unwrap_or_else(|_| CString::new("Invalid zero byte in native error").unwrap())
        .into_raw()
}
fn boundary(call: impl FnOnce() -> Result<(), QueryExecutionError>) -> *mut c_char {
    match catch_unwind(AssertUnwindSafe(|| {
        // Existing value codecs also recurse; reserve the same stack budget as
        // the legacy runtime before crossing from a small DuckDB worker stack.
        stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, call)
    })) {
        Ok(Ok(())) => std::ptr::null_mut(),
        Ok(Err(error)) => allocation(message(error).to_string()),
        Err(_) => allocation("Compiled Orchid operator panicked".into()),
    }
}
unsafe fn program<'a>(pointer: *const c_void) -> Result<&'a Program, QueryExecutionError> {
    unsafe { pointer.cast::<Program>().as_ref() }.ok_or_else(|| "Missing compiled program".into())
}
unsafe fn state<'a>(pointer: *mut c_void) -> Result<&'a mut KernelState, QueryExecutionError> {
    unsafe { pointer.cast::<KernelState>().as_mut() }.ok_or_else(|| "Missing kernel state".into())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_new(input: *const c_char, context: *mut c_void, catalog: crate::catalog::Query, free: Free) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(
        || -> Result<Value, QueryExecutionError> {
            if input.is_null() {
                return Err("Missing compiled request".into());
            }
            let request = serde_json::from_str(
                unsafe { CStr::from_ptr(input) }
                    .to_str()
                    .map_err(QueryExecutionError::from_error)?,
            )
            .map_err(QueryExecutionError::from_error)?;
            let program =
                stacker::maybe_grow(128 * 1024, 64 * 1024 * 1024, || unsafe { crate::catalog::with_catalog(context, catalog, free, || Ok(Program::new(request))) })??;
            let mut metadata = program.metadata();
            metadata["handle"] = json!(Box::into_raw(Box::new(program)) as usize);
            Ok(metadata)
        },
    ));
    allocation(
        match result {
            Ok(Ok(value)) => json!({"ok":true,"result":value}),
            Ok(Err(error)) => {
                let mut out = message(error);
                out["ok"] = json!(false);
                out
            }
            Err(_) => json!({"ok":false,"error":"Orchid program compiler panicked"}),
        }
        .to_string(),
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_manifest(handle: *const c_void) -> *mut c_char {
    allocation(
        match unsafe { program(handle) } {
            Ok(program) => json!({"ok":true,"result":program.metadata()}),
            Err(error) => json!({"ok":false,"error":error.message}),
        }
        .to_string(),
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_schema(
    handle: *const c_void,
    node: u64,
    output: *mut FFI_ArrowSchema,
) -> *mut c_char {
    boundary(|| {
        if output.is_null() {
            return Err("Missing Arrow schema output".into());
        }
        let program = unsafe { program(handle) }?;
        let schema = if node == u64::MAX {
            native_values::result_schema(&program.result_fields())
        } else {
            program.compiled.stage(node as usize)?.schema.clone()
        };
        let schema =
            FFI_ArrowSchema::try_from(schema.as_ref()).map_err(QueryExecutionError::from_error)?;
        unsafe {
            std::ptr::write(output, schema);
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_free(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<Program>()) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_state_new(
    handle: *const c_void,
    context: *mut c_void,
    query: Query,
    free: Free,
    nested: Nested,
    catalog: crate::catalog::Query,
    statement_micros: i64,
    transaction_micros: i64,
    execution_output: *mut *mut c_void,
    state_output: *mut *mut c_void,
) -> *mut c_char {
    boundary(|| {
        if execution_output.is_null() || state_output.is_null() {
            return Err("Missing execution handle outputs".into());
        }
        let program = unsafe { program(handle) }?;
        let services = Arc::new(Services {
            host: DuckDbHost::new(),
            token: NEXT.fetch_add(1, Ordering::Relaxed),
            prepared: program.prepared.clone(),
            policy: program.policy.clone(),
            programs: Default::default(),
        });
        let _scope = unsafe { Scope::enter(&services, context, query, free, nested, catalog) }?;
        let graph = program.prepared.execution_graph(services.host.clone())?;
        let state = KernelState::for_host(graph, services.clone());
        state.start_clocks(statement_micros, transaction_micros)?;
        let scope = state.query_scope();
        let cancelled = state.cancellation_token();
        let execution = Box::into_raw(Box::new(Execution {
            state,
            services,
            scope,
            cancelled,
            finished: false,
        }));
        unsafe {
            *state_output = &mut (*execution).state as *mut KernelState as *mut c_void;
            *execution_output = execution.cast();
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_scope_enter(
    execution: *mut c_void,
    context: *mut c_void,
    query: Query,
    free: Free,
    nested: Nested,
    catalog: crate::catalog::Query,
    output: *mut *mut c_void,
) -> *mut c_char {
    boundary(|| {
        if execution.is_null() {
            return Err("Missing execution state".into());
        }
        // The outer kernel may hold &mut Execution.state during this call.
        // Borrow only the disjoint immutable services field.
        let services = unsafe { &(*execution.cast::<Execution>()).services };
        if output.is_null() {
            return Err("Missing execution scope output".into());
        }
        let scope = unsafe { Scope::enter(services, context, query, free, nested, catalog) }?;
        unsafe {
            *output = Box::into_raw(Box::new(scope)).cast();
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_scope_free(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<Scope>()) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_state_free(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<Execution>()) });
    }
}

/// Returns an independently owned atomic token; its lifetime does not borrow
/// the live state, and cancellation may be signalled by a host watcher thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_cancel_token(execution: *const c_void) -> *mut c_void {
    if execution.is_null() {
        return std::ptr::null_mut();
    }
    let cancelled = unsafe { &(*execution.cast::<Execution>()).cancelled };
    Box::into_raw(Box::new(cancelled.clone())).cast()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_cancel(token: *const c_void) {
    if let Some(token) = unsafe { token.cast::<Arc<std::sync::atomic::AtomicBool>>().as_ref() } {
        token.store(true, Ordering::Release);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_cancel_token_free(token: *mut c_void) {
    if !token.is_null() {
        drop(unsafe { Box::from_raw(token.cast::<Arc<std::sync::atomic::AtomicBool>>()) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_invoke(
    handle: *const c_void,
    node: u64,
    kernel_state: *mut c_void,
    inputs: *mut *mut FFI_ArrowArrayStream,
    count: usize,
    output: *mut FFI_ArrowArrayStream,
) -> *mut c_char {
    boundary(|| {
        if output.is_null() || (count > 0 && inputs.is_null()) {
            return Err("Missing kernel Arrow buffers".into());
        }
        let program = unsafe { program(handle) }?;
        let StageOperation::Kernel(kernel) = &program.compiled.stage(node as usize)?.operation
        else {
            return Err("SQL stages execute in DuckDB".into());
        };
        if kernel.inputs().len() != count {
            return Err("Compiled kernel input arity mismatch".into());
        }
        let state = unsafe { state(kernel_state) }?;
        let mut rows = Vec::with_capacity(count);
        for index in 0..count {
            let batch = unsafe { import(*inputs.add(index)) }?;
            rows.push(if let Some(fields) = kernel.relational_fields() {
                native_values::decode_rows(&batch, fields, &state.graph)?
            } else {
                kernel_host::decode_rows(&batch)?
            });
        }
        let rows = with_operator_table(program.prepared.operators.clone(), || {
            kernel.invoke(rows, state)
        })?;
        unsafe {
            std::ptr::write(output, export(kernel_host::encode_rows(rows)?));
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_source_open(
    handle: *const c_void,
    node: u64,
    output: *mut *mut c_void,
) -> *mut c_char {
    boundary(|| {
        if output.is_null() {
            return Err("Missing source cursor output".into());
        }
        let program = unsafe { program(handle) }?;
        let StageOperation::Kernel(kernel) = &program.compiled.stage(node as usize)?.operation
        else {
            return Err("SQL source executes in DuckDB".into());
        };
        let cursor = kernel
            .source_cursor()
            .ok_or("Compiled kernel is not a source")?;
        unsafe {
            *output = Box::into_raw(Box::new(SourceCursor {
                cursor,
                prepared: program.prepared.clone(),
            }))
            .cast();
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_source_next(
    cursor: *mut c_void,
    kernel_state: *mut c_void,
    size: usize,
    output: *mut FFI_ArrowArrayStream,
    done: *mut bool,
) -> *mut c_char {
    boundary(|| {
        if output.is_null() || done.is_null() {
            return Err("Missing source output buffers".into());
        }
        let cursor =
            unsafe { cursor.cast::<SourceCursor>().as_mut() }.ok_or("Missing source cursor")?;
        let state = unsafe { state(kernel_state) }?;
        let rows = with_operator_table(cursor.prepared.operators.clone(), || {
            cursor.cursor.next(size, state)
        })?;
        unsafe {
            *done = rows.is_none();
            std::ptr::write(
                output,
                export(kernel_host::encode_rows(rows.unwrap_or_default())?),
            );
        }
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_source_free(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<SourceCursor>()) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_program_finish(
    handle: *const c_void,
    execution: *mut c_void,
    input: *mut FFI_ArrowArrayStream,
    output: *mut FFI_ArrowArrayStream,
) -> *mut c_char {
    boundary(|| {
        if output.is_null() {
            return Err("Missing program result output".into());
        }
        let program = unsafe { program(handle) }?;
        let execution =
            unsafe { execution.cast::<Execution>().as_mut() }.ok_or("Missing execution state")?;
        if execution.finished {
            return Err("Compiled program has already finalized".into());
        }
        let rows = kernel_host::decode_rows(&unsafe { import(input) }?)?;
        execution.state.prepare_results(&rows)?;
        let batch = native_values::result_batch(
            &program.result_fields(),
            program.result_form,
            if program.fields.is_empty() {
                &[]
            } else {
                &rows
            },
            &execution.state.graph,
            &program.policy,
        )?;
        if program.mutating {
            match &program.prepared.managed_table {
                Some(table) => orchiddb::ir::rel::host::managed::ManagedStore::new(table)
                    .persist(execution.services.host.as_ref(), &execution.state.graph)?,
                None => mapped_storage::persist(
                    execution.services.host.as_ref(),
                    &execution.state.graph,
                    &program.prepared.mapping,
                )?,
            }
        }
        execution.scope.complete();
        execution.finished = true;
        unsafe {
            std::ptr::write(output, export(batch));
        }
        Ok(())
    })
}
