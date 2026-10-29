//! Optional DuckDB UDFs. No query planning, SQL execution, catalog, or network service.
mod typed_scalar;
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
fn request(input: Value) -> Result<Value, String> {
    match input["op"].as_str() {
        Some("typed_scalar_catalog") => Ok(Value::Array(
            orchiddb::ir::rel::native_values::typed::operations()
                .map(|operation| {
                    use orchiddb::ir::rel::native_values::typed::{self, Output};
                    let output = match typed::output(operation) {
                        Output::Value => "value",
                        Output::Boolean => "boolean",
                        Output::Integer => "integer",
                        Output::Text => "text",
                        Output::List => "list",
                        Output::Indices => "indices",
                    };
                    json!({"operation":operation,"name":typed::sql_name(operation),"output":output})
                })
                .collect(),
        )),
        Some("native_key") => {
            let values = input["rows"]
                .as_array()
                .ok_or("Missing key rows")?
                .iter()
                .map(|row| {
                    orchiddb::ir::rel::native_values::key(
                        &row[0],
                        row[1].as_bool().unwrap_or(false),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"values":values}))
        }
        Some("native_sort") => {
            let mut values = Vec::new();
            for row in input["rows"].as_array().ok_or("Missing sort rows")? {
                values.push(orchiddb::ir::rel::native_values::sort(
                    row[0].as_str().ok_or("Missing sort keys")?,
                    &row[1],
                )?);
            }
            Ok(json!({"values":values}))
        }
        Some("native_collection") => {
            let mut values = Vec::new();
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

        _ => Err("Unsupported function-module operation".into()),
    }
}
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
        stacker::maybe_grow(1024 * 1024, 16 * 1024 * 1024, || {
            request(serde_json::from_str(&input).map_err(|e| e.to_string())?)
        })
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_bridge_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

/// Convert a nullable UTF-8 batch using Cypher numeric semantics.
///
/// # Safety
/// Input and output pointers must address buffers of at least `rows` elements.
/// Each non-null string pointer must address its corresponding byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_cypher_to_float_v1(
    strings: *const *const u8,
    lengths: *const usize,
    rows: usize,
    values: *mut f64,
    valid: *mut u8,
) -> i32 {
    if rows == 0 {
        return 0;
    }
    if strings.is_null() || lengths.is_null() || values.is_null() || valid.is_null() {
        return 1;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let strings = unsafe { std::slice::from_raw_parts(strings, rows) };
        let lengths = unsafe { std::slice::from_raw_parts(lengths, rows) };
        let values = unsafe { std::slice::from_raw_parts_mut(values, rows) };
        let valid = unsafe { std::slice::from_raw_parts_mut(valid, rows) };
        for row in 0..rows {
            valid[row] = 0;
            values[row] = 0.0;
            if !strings[row].is_null() {
                let bytes = unsafe { std::slice::from_raw_parts(strings[row], lengths[row]) };
                let input = std::str::from_utf8(bytes).map_err(|_| ())?;
                if let Ok(value) = orchiddb::ir::functions::cypher_float::parse_string(input) {
                    values[row] = value;
                    valid[row] = 1;
                }
            }
        }
        Ok::<(), ()>(())
    })) {
        Ok(Ok(())) => 0,
        _ => 1,
    }
}

#[cfg(test)]
mod float_abi_tests {
    #[test]
    fn bridge_accepts_helpers_only() {
        for op in [
            "compile",
            "validate_schema",
            "resolve_schema",
            "catalog",
            "update",
            "sparql_scalar",
        ] {
            assert!(
                super::request(serde_json::json!({"op":op})).is_err(),
                "{op}"
            );
        }
        assert!(
            super::request(serde_json::json!({"op":"typed_scalar_catalog"}))
                .unwrap()
                .as_array()
                .unwrap()
                .len()
                > 0
        );
    }
    #[test]
    fn nullable_batch_uses_shared_contract() {
        let inputs = [
            Some("\u{a0}__1__2__\u{3000}"),
            Some("NaN"),
            None,
            Some("1e-999"),
            Some("1e999"),
        ];
        let pointers = inputs.map(|s| s.map_or(std::ptr::null(), str::as_ptr));
        let lengths = inputs.map(|s| s.map_or(0, str::len));
        let mut values = [0.0; 5];
        let mut valid = [0; 5];
        assert_eq!(
            unsafe {
                super::orchid_cypher_to_float_v1(
                    pointers.as_ptr(),
                    lengths.as_ptr(),
                    5,
                    values.as_mut_ptr(),
                    valid.as_mut_ptr(),
                )
            },
            0
        );
        assert_eq!(valid, [1, 0, 0, 1, 0]);
        assert_eq!(values[0], 12.0);
        assert_eq!(values[3], 0.0);
    }
}
