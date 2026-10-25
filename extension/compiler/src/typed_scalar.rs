//! Typed Arrow batch ABI for SQL scalar helpers. The operation is fixed by the
//! registered SQL function; neither expressions nor argument values use JSON.
use arrow::{
    array::{Array, RecordBatch, RecordBatchIterator, StructArray},
    datatypes::{Field, Schema},
    ffi::{FFI_ArrowArray, FFI_ArrowSchema, from_ffi},
    ffi_stream::FFI_ArrowArrayStream,
};
use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

/// Consumes `input`'s release callback, borrows `schema`, and transfers `output`
/// ownership on success. A non-null error must be freed with orchid_bridge_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orchid_cypher_scalar_v1(
    operation: *const c_char,
    input: *mut FFI_ArrowArray,
    schema: *const FFI_ArrowSchema,
    output: *mut FFI_ArrowArrayStream,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        if operation.is_null() || input.is_null() || schema.is_null() || output.is_null() {
            return Err("Invalid typed scalar ABI pointer".into());
        }
        let name = unsafe { CStr::from_ptr(operation) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let owned = unsafe { std::ptr::replace(input, FFI_ArrowArray::empty()) };
        let data = unsafe { from_ffi(owned, &*schema) }.map_err(|e| e.to_string())?;
        let batch = StructArray::from(data);
        let values =
            orchiddb::ir::rel::native_values::typed::evaluate(name, batch.columns(), batch.len())?;
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            values.data_type().clone(),
            true,
        )]));
        let batch =
            RecordBatch::try_new(schema.clone(), vec![values]).map_err(|e| e.to_string())?;
        unsafe {
            *output = FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
                vec![Ok(batch)],
                schema,
            )));
        }
        Ok(())
    }));
    match result {
        Ok(Ok(())) => std::ptr::null_mut(),
        result => {
            let error = match result {
                Ok(Err(error)) => error,
                _ => "Typed scalar panicked".into(),
            };
            let error = orchiddb::ir::diagnostics::QueryExecutionError::from(error);
            let classification = error.diagnosis.map(|code| {
                let (kind, detail, phase) = code.classification();
                serde_json::json!({"type":kind,"detail":detail,"phase":phase})
            });
            let envelope =
                serde_json::json!({"error":error.message,"orchid_classification":classification});
            CString::new(envelope.to_string()).unwrap().into_raw()
        }
    }
}
