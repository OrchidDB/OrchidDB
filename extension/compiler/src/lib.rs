//! Language bridge with caller-owned DuckDB binding and execution.
mod syntax;
mod authorization;
mod catalog;
mod update;
mod host;
mod managed;
mod program;

use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn request(input: Value) -> Result<Value, String> {
    match input["op"].as_str() {
        Some("validate_schema") => serde_json::to_value(orchiddb::session::Schema::from_value(input["schema"].clone())?).map_err(|e| e.to_string()),
        Some("spicedb_check") => authorization::check(&input),
        Some("native_key") => {
            let values = input["rows"].as_array().ok_or("Missing key rows")?.iter().map(|row| {
                orchiddb::ir::rel::native_values::key(&row[0], row[1].as_bool().unwrap_or(false))
            }).collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"values":values}))
        }
        Some("native_sort") => {
            let mut values=Vec::new();
            for row in input["rows"].as_array().ok_or("Missing sort rows")? {
                values.push(orchiddb::ir::rel::native_values::sort(row[0].as_str().ok_or("Missing sort keys")?,&row[1])?);
            }
            Ok(json!({"values":values}))
        }
        Some("native_collection") => {
            let mut values=Vec::new();
            for row in input["rows"].as_array().ok_or("Missing scalar rows")? {
                values.push(if input["project"] == true {
                    match orchiddb::ir::rel::native_values::project(row[0].as_str().ok_or("Missing projection expression")?, &row[1], input["statement_micros"].as_i64().ok_or("Missing statement clock")?, input["transaction_micros"].as_i64().ok_or("Missing transaction clock")?) {
                        Ok(value) => value,
                        Err(error) => return Ok(json!({"execution_error":error.message,"classification":error.diagnosis.map(|code| {
                            let (kind,detail,phase)=code.classification();json!({"type":kind,"detail":detail,"phase":phase})
                        })})),
                    }
                } else if input["procedure"] == true {
                    match orchiddb::ir::rel::native_values::procedure(row[0].as_str().ok_or("Missing procedure specification")?,&row[1]) {
                        Ok(value)=>value,
                        Err(error)=>return Ok(json!({"execution_error":error.message,"classification":error.diagnosis.map(|code|{
                            let(kind,detail,phase)=code.classification();json!({"type":kind,"detail":detail,"phase":phase})
                        })})),
                    }
                } else if input["aggregate"] == true {
                    orchiddb::ir::rel::native_values::aggregate(row[0].as_str().ok_or("Missing aggregate specification")?,&row[1])?
                } else {orchiddb::ir::rel::native_values::items(&row[0])?});
            }
            Ok(json!({"values":values}))
        }
        Some("native_scalar") => {
            let mut values = Vec::new();
            for row in input["rows"].as_array().ok_or("Missing scalar rows")? {
                values.push(if input["json"].as_bool().unwrap_or(false) {
                    orchiddb::ir::rel::native_values::json(&row[0])?
                } else {
                    match orchiddb::ir::rel::native_values::evaluate(row[0].as_str().ok_or("Missing native expression")?, &row[1], input["predicate"].as_bool().unwrap_or(false), input["statement_micros"].as_i64().ok_or("Missing statement clock")?, input["transaction_micros"].as_i64().ok_or("Missing transaction clock")?) {
                        Ok(value)=>value,
                        Err(error)=>return Ok(json!({"execution_error":error.message,"classification":error.diagnosis.map(|code|{
                            let (kind,detail,phase)=code.classification();json!({"type":kind,"detail":detail,"phase":phase})
                        })})),
                    }
                });
            }
            Ok(json!({"values":values}))
        }

        Some("sparql_scalar") => {
            let rows: Vec<Vec<Option<String>>> = serde_json::from_value(input["rows"].clone()).map_err(|e| e.to_string())?;
            Ok(json!({"values": orchiddb::language::sparql::scalar::evaluate_batch(&rows)?}))
        }
        Some("rewrite") => {
            Ok(json!({"sql": syntax::rewrite(input["sql"].as_str().ok_or("missing SQL")?)?}))
        }
        Some("compile_request") => compile(input["request"].clone(), input["inspect"].as_bool().unwrap_or(false)),
        Some("sparql_syntax") => {
            let query = input["query"].as_str().ok_or("missing SPARQL query")?;
            let base = input["base"].as_str();
            if input["update"].as_bool().unwrap_or(false) {
                orchiddb::language::sparql::parse_update(query, base).map_err(|e| e.to_string())?;
            } else if let Some(base) = base {
                orchiddb::language::sparql::parse_query_with_base(query, base).map_err(|e| e.to_string())?;
            } else {
                orchiddb::language::sparql::parse_query(query).map_err(|e| e.to_string())?;
            }
            Ok(json!({"parsed": true}))
        }
        Some("validate" | "compile") => {
            let graph: syntax::Graph =
                serde_json::from_value(input["graph"].clone()).map_err(|e| e.to_string())?;
            let tables = input["tables"].as_array().ok_or("missing bound schemas")?;
            if let Some(table) = &graph.managed_table {
                if input["op"] == "validate" {return Ok(json!({"valid":true}));}
                return compile(json!({"version":1,"dialect":"duckdb","language":input.get("language").cloned().unwrap_or(json!("cypher")),
                    "query":input["query"],"parameters":input.get("parameters").cloned().unwrap_or(json!({})),"tables":[],"managed_table":table}),false);
            }
            let (mut nodes, mut edges) = graph.mappings(tables)?;
            if let Some(policy) = &graph.authorization {
                authorization::apply(policy, &graph, tables, &mut nodes, &mut edges)?;
            }
            let request = json!({
                "version": 1, "dialect": "duckdb", "language": input.get("language").cloned().unwrap_or(json!("cypher")),
                "query": input.get("query").cloned().unwrap_or(json!("RETURN 1")),
                "parameters": input.get("parameters").cloned().unwrap_or(json!({})),
                "tables": tables, "nodes": nodes, "edges": edges,
                "computed_relationships": graph.computed_relationships
            });
            if input["op"] == "validate" {
                if !graph.computed_relationships.is_empty() {
                    let request = serde_json::from_value(request).map_err(|e| e.to_string())?;
                    let prepared = orchiddb::compiler::prepare_graph(&request)?;
                    orchiddb::ir::functions::with_operator_table(prepared.operators, ||
                        prepared.mapping.validate_computed_relationships()).map_err(|e| e.to_string())?;
                }
                return Ok(json!({"valid": true}));
            }
            if graph.authorization.is_some() {
                let parsed = serde_json::from_value(request.clone()).map_err(|e|e.to_string())?;
                let prepared = orchiddb::compiler::prepare_graph(&parsed)?;
                if orchiddb::ir::exec::contains_mutation(&prepared.plan.root) { return Err("authorized graphs are read-only".into()); }
            }
            compile(request, false)
        }
        _ => Err("unknown Orchid extension bridge operation".into()),
    }
}

fn compile(input: Value, inspect: bool) -> Result<Value, String> {
    let mut input = input;
    input["native_values"] = json!(true);
    let request: orchiddb::compiler::CompileRequest = serde_json::from_value(input.clone()).map_err(|e| e.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all().build().map_err(|e| e.to_string())?;
    let diagnostic = inspect.then(|| orchiddb::compiler::cypher_diagnostic(&request)).flatten();
    let compiled = match runtime.block_on(orchiddb::compiler::compile(request)) {
        Ok(compiled) => compiled,
        Err(error) => {
            // Invalid corpus provenance cannot be repaired by splitting the
            // query into row kernels, which no longer own the mapped corpus.
            if error.contains(orchiddb::ir::rel::mapping::BM25_CORPUS_ERROR) {
                return Err(error);
            }
            match program::compile_request_json(input) {
                Ok(compiled) => return Ok(compiled),
                Err(_) => {},
            }
            if inspect {
                let classification = diagnostic.filter(|d| d.message == error).and_then(|d| d.classification);
                return Ok(json!({"error": error, "classification": classification}));
            }
            return Err(error);
        },
    };
    if !compiled.transfers.is_empty() {
        return Err("query requires dependent execution, which is not supported by the DuckDB extension yet".into());
    }
    serde_json::to_value(compiled).map_err(|e| e.to_string())
}

/// # Safety
/// `input` must point to a NUL-terminated UTF-8 string for this call.
/// Free the returned allocation once with `orchid_bridge_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_bridge(input: *const c_char) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        if input.is_null() {
            return Err("null bridge input".into());
        }
        let input = unsafe { CStr::from_ptr(input) }
            .to_str()
            .map_err(|e| e.to_string())?
            .to_owned();
        // Foreign client threads can have small stacks. Catch panics inside the worker too.
        std::thread::Builder::new()
            .name("orchid-compiler".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                catch_unwind(AssertUnwindSafe(|| {
                    request(serde_json::from_str(&input).map_err(|e| e.to_string())?)
                }))
                .unwrap_or_else(|_| Err("Orchid compiler panicked".into()))
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "Orchid compiler worker failed".to_string())?
    }));
    let response = match response {
        Ok(Ok(value)) => json!({"ok": true, "result": value}),
        Ok(Err(error)) => json!({"ok": false, "error": error}),
        Err(_) => json!({"ok": false, "error": "Orchid bridge panicked"}),
    };
    CString::new(response.to_string())
        .expect("JSON contains no raw NUL")
        .into_raw()
}

/// Compile synchronously while borrowing the caller's DuckDB binder.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_bridge_catalog(input: *const c_char, context: *mut std::ffi::c_void, query: catalog::Query, free: catalog::Free) -> *mut c_char {
    let result=catch_unwind(AssertUnwindSafe(|| -> Result<Value,String> {
        if input.is_null() {return Err("null bridge input".into());}
        let input: Value=serde_json::from_str(unsafe{CStr::from_ptr(input)}.to_str().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        stacker::maybe_grow(128*1024,64*1024*1024,||unsafe{catalog::with_catalog(context,query,free,||request(input))})
    }));
    let result=match result {Ok(Ok(value))=>json!({"ok":true,"result":value}),Ok(Err(error))=>json!({"ok":false,"error":error}),Err(_)=>json!({"ok":false,"error":"Orchid compiler panicked"})};
    CString::new(result.to_string()).unwrap().into_raw()
}

/// # Safety
/// `value` must be null or an unfreed allocation returned by `orchid_bridge`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_bridge_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

/// Execute shared update orchestration. Callbacks are synchronous and borrow the
/// host context only for this call; all effects join its existing transaction.
/// # Safety
/// All pointers and callback functions must remain valid until this returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_update(input: *const c_char, state: *mut std::ffi::c_void, callback: update::Callback, free: update::Free) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| stacker::grow(16 * 1024 * 1024, || {
        if input.is_null() { return Err("null update input".into()); }
        let text = unsafe { CStr::from_ptr(input) }.to_str().map_err(|e| e.to_string())?;
        update::run(serde_json::from_str(text).map_err(|e| e.to_string())?, state, callback, free)
    })));
    let result = match result {
        Ok(Ok(value)) => json!({"ok":true,"result":value}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Orchid update panicked"}),
    };
    CString::new(result.to_string()).expect("JSON has no raw NUL").into_raw()
}
