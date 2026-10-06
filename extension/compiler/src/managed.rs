//! Host API for durable managed graphs; imports share the existing provider codec.
use crate::host::{Free, HostScope, Query};
use orchiddb::ir::{
    catalog::import::import_graph,
    rel::host::{managed::ManagedStore, observation::observe},
};
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_managed(
    input: *const c_char,
    context: *mut c_void,
    query: Query,
    free: Free,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        if input.is_null() {
            return Err("Missing managed graph request".into());
        }
        let request: Value = serde_json::from_str(
            unsafe { CStr::from_ptr(input) }
                .to_str()
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let table = request["table"]
            .as_str()
            .ok_or("Managed graph table is required")?;
        let store = ManagedStore::new(table);
        let (_scope, host) = unsafe { HostScope::enter(context, query, free) };
        match request["op"]
            .as_str()
            .ok_or("Managed operation is required")?
        {
            "create" => {
                store.create(host.as_ref())?;
                Ok(json!({"ok":true}))
            }
            "reset" => {
                store.reset(host.as_ref())?;
                Ok(json!({"ok":true}))
            }
            "import" => {
                let fixture = if request["fixture"].is_null() {
                    &request
                } else {
                    &request["fixture"]
                };
                let graph = import_graph(fixture)?;
                store.reset(host.as_ref())?;
                store.persist(host.as_ref(), &graph)?;
                Ok(json!({"ok":true}))
            }
            "snapshot" => {
                let graph = store.attach(host)?;
                Ok(json!({"native_snapshot":observe(&graph,None)?}))
            }
            other => Err(format!("Unknown managed graph operation {other}")),
        }
    }));
    let envelope = match result {
        Ok(Ok(result)) => json!({"ok":true,"result":result}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Managed graph host panicked"}),
    };
    CString::new(envelope.to_string())
        .expect("JSON has no raw NUL")
        .into_raw()
}
