//! Borrow the caller's binder on this thread. Stored plans retain only catalog
//! metadata; callbacks cannot outlive the synchronous compilation scope.
use orchiddb::ir::functions::{OperatorTable, host_catalog::HostCatalog};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_void},
    sync::Arc,
};
pub type Query = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *mut arrow::ffi::FFI_ArrowSchema,
) -> *mut c_char;
pub type Free = unsafe extern "C" fn(*mut c_char);
#[derive(Clone, Copy)]
struct Callback {
    context: *mut c_void,
    query: Query,
    free: Free,
}
thread_local! { static ACTIVE: RefCell<Vec<Callback>> = RefCell::new(Vec::new()); }
pub struct Scope(std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|m| m.pop());
    }
}
pub unsafe fn enter(context: *mut c_void, query: Query, free: Free) -> Scope {
    ACTIVE.with_borrow_mut(|m| {
        m.push(Callback {
            context,
            query,
            free,
        })
    });
    Scope(std::marker::PhantomData)
}
fn query(request: Value, schema: *mut arrow::ffi::FFI_ArrowSchema) -> Result<Value, String> {
    let cb = ACTIVE
        .with_borrow(|m| m.last().copied())
        .ok_or("DuckDB binder scope expired or belongs to another thread")?;
    let input = CString::new(request.to_string()).map_err(|e| e.to_string())?;
    let result = unsafe { (cb.query)(cb.context, input.as_ptr(), schema) };
    if result.is_null() {
        return Err("DuckDB binder returned no response".into());
    }
    let text = unsafe { CStr::from_ptr(result) }
        .to_string_lossy()
        .into_owned();
    unsafe { (cb.free)(result) };
    let result: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if result["ok"] != true {
        return Err(result["error"]
            .as_str()
            .unwrap_or("DuckDB binding failed")
            .into());
    }
    Ok(result["result"].clone())
}
pub unsafe fn with_catalog<T>(
    context: *mut c_void,
    callback: Query,
    free: Free,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _guard = unsafe { enter(context, callback, free) };
    let rows = query(json!({"op":"functions"}), std::ptr::null_mut())?;
    let catalog: Arc<dyn OperatorTable> = Arc::new(HostCatalog::new(
        rows.as_array().ok_or("Invalid DuckDB catalog response")?,
        move |sql| {
            let mut schema = arrow::ffi::FFI_ArrowSchema::empty();
            let consistent = query(json!({"op":"bind","sql":sql}), &mut schema)?;
            let schema = arrow::datatypes::Schema::try_from(&schema).map_err(|e| e.to_string())?;
            Ok((
                schema.field(0).data_type().clone(),
                consistent.as_bool().unwrap_or(false),
            ))
        },
    ));
    orchiddb::ir::functions::with_operator_table(catalog, run)
}

/// Read-only adapter attestation. Only the registered C++ callback can supply
/// this result; there is no serialized query/request capability override.
pub fn rule_context() -> Result<orchiddb::ir::rules::RuleContext, String> {
    let mut context = orchiddb::ir::rules::RuleContext::default();
    if !ACTIVE.with_borrow(|callbacks| !callbacks.is_empty()) { return Ok(context); }
    let capability = query(json!({"op":"float_helper_capability"}), std::ptr::null_mut())?;
    context.capabilities.language_functions_enabled = true;
    if let Some(identity) = capability["registration_identity"].as_str() {
        context.capabilities.float_helper = Some(unsafe {
            orchiddb::ir::rules::FloatHelperCapability::from_verified_adapter(identity.to_string())
        });
        context.session_fingerprint = identity.to_string();
        context.owner = Some(orchiddb::ir::rules::EngineIdentity {
            name: "duckdb".into(), version: capability["version"].as_str().unwrap_or("").into(),
        });
    }
    Ok(context)
}
