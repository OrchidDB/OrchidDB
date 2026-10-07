//! Shared planning and statistics C ABI. Database sessions remain caller-owned.
use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::OnceLock,
};

#[unsafe(no_mangle)]
pub extern "C" fn orchiddb_abi_version() -> u32 {
    2
}

#[unsafe(no_mangle)]
pub extern "C" fn orchiddb_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn orchiddb_core_revision() -> *const c_char {
    concat!(env!("ORCHIDDB_BUILT_CORE_REVISION"), "\0")
        .as_ptr()
        .cast()
}

fn dispatch(input: String, operation: u8) -> Result<serde_json::Value, String> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    let runtime = RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    // Avoid recursively planning on small foreign-language/NIF stacks. No database is opened.
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    runtime.spawn(async move {
        let result = match operation {
            1 => orchiddb::ir::rel::statistics::command(&input).await,
            2 => {
                #[cfg(any(feature = "quickwit", feature = "elasticsearch"))]
                { orchiddb::remote::transport::command(&input).await }
                #[cfg(not(any(feature = "quickwit", feature = "elasticsearch")))]
                { Err("remote engine support is disabled".into()) }
            },
            _ => orchiddb::compiler::compile_json(&input).await,
        };
        let _ = sender.send(result);
    });
    let output = receiver
        .recv()
        .map_err(|_| "compiler worker failed".to_string())??;
    serde_json::from_str(&output).map_err(|e| e.to_string())
}

/// Compile a version-1 JSON request. The returned UTF-8 JSON is caller-owned.
///
/// # Safety
/// `input` is null or points to a valid NUL-terminated string for this call.
/// The returned pointer must be freed exactly once with `orchiddb_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchiddb_execution_command(input: *const c_char) -> *mut c_char {
    unsafe { invoke_json(input, 0) }
}

/// Bind a live Arrow C stream into a compiled SELECT. No database is accessed.
///
/// # Safety
/// `input` must be a valid UTF-8 C string containing a plan and relation name.
/// `stream` must point to a valid Arrow C stream. Ownership moves into this call:
/// its release callback is cleared and all imported resources are released here.
/// Free the returned response with `orchiddb_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchiddb_bind_arrow_json(
    input: *const c_char,
    stream: *mut arrow::ffi_stream::FFI_ArrowArrayStream,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<serde_json::Value, String> {
        if input.is_null() || stream.is_null() {
            return Err("null binding argument".into());
        }
        let reader = unsafe { arrow::ffi_stream::ArrowArrayStreamReader::from_raw(stream) }
            .map_err(|e| e.to_string())?;
        let command: serde_json::Value = serde_json::from_str(
            unsafe { CStr::from_ptr(input) }
                .to_str()
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let batches = reader.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        if matches!(command["op"].as_str(), Some("bind_operation" | "bind_search")) {
            orchiddb::federation::bind_operation_batches(command, &batches)
        } else {
            let name = command["relation"].as_str().ok_or("missing relation")?;
            orchiddb::federation::bind_plan_batches(command["plan"].clone(), name, &batches)
        }
    }));
    let response = match result {
        Ok(Ok(result)) => serde_json::json!({"ok":true,"result":result}),
        Ok(Err(error)) => serde_json::json!({"ok":false,"error":error}),
        Err(_) => {
            serde_json::json!({"ok":false,"error":"Arrow binding panicked; no SQL was executed"})
        }
    };
    CString::new(response.to_string()).unwrap().into_raw()
}

/// Execute a statistics protocol command. Response ownership matches orchiddb_execution_command.
/// # Safety
/// Input must be null or a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchiddb_statistics_json(input: *const c_char) -> *mut c_char {
    unsafe { invoke_json(input, 1) }
}

/// Open, execute, or close an explicitly requested remote HTTP session.
/// # Safety
/// Input must be null or a valid NUL-terminated UTF-8 string. Free the response
/// using orchiddb_string_free. Unlike the execution preparation command, execute performs network I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchiddb_remote_json(input: *const c_char) -> *mut c_char {
    unsafe { invoke_json(input, 2) }
}

unsafe fn invoke_json(input: *const c_char, operation: u8) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if input.is_null() {
            return Err("input must not be null".to_string());
        }
        let text = unsafe { CStr::from_ptr(input) }
            .to_str()
            .map_err(|_| "input must be UTF-8".to_string())?;
        dispatch(text.to_owned(), operation)
    }));
    let response = match result {
        Ok(Ok(result)) => serde_json::json!({"ok": true, "result": result}),
        Ok(Err(error)) => serde_json::json!({"ok": false, "error": error}),
        Err(_) => {
            serde_json::json!({"ok": false, "error": "compiler panicked; no SQL was executed"})
        }
    };
    // JSON escapes embedded NULs; CString cannot fail for the serializer's UTF-8 output.
    CString::new(response.to_string())
        .expect("JSON has no raw NUL")
        .into_raw()
}

/// Release one returned response; null is accepted. Never free version/revision pointers.
///
/// # Safety
/// `response` is null or an unfreed pointer returned by this library's compile function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchiddb_string_free(response: *mut c_char) {
    if !response.is_null() {
        drop(unsafe { CString::from_raw(response) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn invoke(ptr: *const c_char) -> serde_json::Value {
        let out = unsafe { orchiddb_execution_command(ptr) };
        assert!(!out.is_null());
        let response =
            serde_json::from_str(unsafe { CStr::from_ptr(out) }.to_str().unwrap()).unwrap();
        unsafe { orchiddb_string_free(out) };
        response
    }
    #[test]
    fn version_and_error_ownership() {
        assert_eq!(orchiddb_abi_version(), 2);
        assert_eq!(
            unsafe { CStr::from_ptr(orchiddb_version()) }
                .to_str()
                .unwrap(),
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(unsafe { invoke(std::ptr::null()) }["ok"], false);
        assert_eq!(unsafe { invoke(c"not-json".as_ptr()) }["ok"], false);
        let invalid = [255u8, 0];
        assert_eq!(unsafe { invoke(invalid.as_ptr().cast()) }["ok"], false);
        unsafe { orchiddb_string_free(std::ptr::null_mut()) };
    }
    #[test]
    fn actual_compiler_and_parallel_foreign_calls() {
        let threads: Vec<_> = (0..4).map(|_| std::thread::spawn(|| {
            let input = CString::new(r#"{"version":1,"dialect":"duckdb","language":"cypher","query":"RETURN 42 AS answer","tables":[],"nodes":[]}"#).unwrap();
            let result = unsafe { invoke(input.as_ptr()) };
            assert_eq!(result["ok"], true, "{result}");
            assert_eq!(result["result"]["fields"], serde_json::json!(["answer"]));
            assert!(result["result"]["sql"].as_str().unwrap().contains("42"));
        })).collect();
        for thread in threads {
            thread.join().unwrap();
        }
    }
    #[test]
    fn statistics_catalog_lifecycle() {
        fn command(value: serde_json::Value) -> serde_json::Value {
            let input = CString::new(value.to_string()).unwrap();
            let out = unsafe { orchiddb_statistics_json(input.as_ptr()) };
            assert!(!out.is_null());
            let response: serde_json::Value =
                serde_json::from_str(unsafe { CStr::from_ptr(out) }.to_str().unwrap()).unwrap();
            unsafe { orchiddb_string_free(out) };
            assert_eq!(response["ok"], true, "{response}");
            response["result"].clone()
        }
        let request = serde_json::json!({"version":1,"dialect":"duckdb","language":"cypher","query":"RETURN 42 AS answer","tables":[],"nodes":[]});
        let state = command(serde_json::json!({"op":"begin","request":request}));
        assert!(state["request"].is_null());
        let finished = command(serde_json::json!({"op":"finish","id":state["id"]}));
        let compiled = command(
            serde_json::json!({"op":"compile","catalog_id":finished["catalog_id"],"request":request}),
        );
        assert_eq!(compiled["fields"], serde_json::json!(["answer"]));
        let installed =
            command(serde_json::json!({"op":"install","snapshot":finished["snapshot"]}));
        command(serde_json::json!({"op":"release","catalog_id":installed["catalog_id"]}));
        command(serde_json::json!({"op":"release","catalog_id":finished["catalog_id"]}));
        let invalid = unsafe { orchiddb_statistics_json(std::ptr::null()) };
        let response: serde_json::Value =
            serde_json::from_str(unsafe { CStr::from_ptr(invalid) }.to_str().unwrap()).unwrap();
        assert_eq!(response["ok"], false);
        unsafe { orchiddb_string_free(invalid) };
    }
    #[test]
    fn malformed_arrow_does_not_poison_statistics_registry() {
        fn command(value: serde_json::Value) -> serde_json::Value {
            let input = CString::new(value.to_string()).unwrap();
            let out = unsafe { orchiddb_statistics_json(input.as_ptr()) };
            let response =
                serde_json::from_str(unsafe { CStr::from_ptr(out) }.to_str().unwrap()).unwrap();
            unsafe { orchiddb_string_free(out) };
            response
        }
        let state = command(serde_json::json!({"op":"begin", "request":{
            "version":1,"dialect":"duckdb","language":"cypher","query":"RETURN 1",
            "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"}]}]
        }}));
        assert_eq!(state["ok"], true, "{state}");
        let id = &state["result"]["id"];
        // IPC with an invalid integer bit width, formerly triggering an Arrow
        // decoder panic while the statistics registry mutex was held.
        let bad = command(serde_json::json!({"op":"submit","id":id,
            "request_id":state["result"]["request"]["id"],
            "ipc":include_str!("../tests/fixtures/invalid-arrow-int.base64").trim()}));
        assert_eq!(bad["ok"], false);
        let next = command(serde_json::json!({"op":"next","id":id}));
        assert_eq!(next["ok"], true, "{next}");
        assert_eq!(next["result"]["request"], state["result"]["request"]);
        assert_eq!(
            command(serde_json::json!({"op":"cancel","id":id}))["ok"],
            true
        );
    }
}
