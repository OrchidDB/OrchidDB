//! Nominal logical types carried losslessly by Arrow's existing type system.
//!
//! A domain has a named, single-field storage envelope. The envelope preserves
//! identity through expressions, nested collections and IPC even where a driver
//! represents an extension value using its storage type. Engine adapters choose
//! the corresponding SQL type and codec; new domains need no Arrow fork.
use arrow::{
    array::{Array, ArrayRef, StructArray},
    datatypes::{DataType, Field},
};
use datafusion::common::{DataFusionError, Result, ScalarValue};
use std::sync::Arc;

const PREFIX: &str = "__orchiddb_domain_";

pub fn data_type(name: &str, storage: DataType) -> DataType {
    DataType::Struct(vec![Field::new(format!("{PREFIX}{name}"), storage, true)].into())
}
pub fn descriptor(ty: &DataType) -> Option<(&str, &DataType)> {
    let DataType::Struct(fields) = ty else {
        return None;
    };
    if fields.len() != 1 {
        return None;
    }
    Some((
        fields[0].name().strip_prefix(PREFIX)?,
        fields[0].data_type(),
    ))
}
pub fn scalar(name: &str, storage: ScalarValue) -> Result<ScalarValue> {
    let DataType::Struct(fields) = data_type(name, storage.data_type()) else {
        unreachable!()
    };
    Ok(ScalarValue::Struct(Arc::new(StructArray::try_new(
        fields,
        vec![storage.to_array_of_size(1)?],
        None,
    )?)))
}
pub fn storage(value: &ScalarValue) -> Result<ScalarValue> {
    let ScalarValue::Struct(array) = value else {
        return Err(DataFusionError::Plan("expected a domain value".into()));
    };
    let ty = value.data_type();
    let Some((_, storage)) = descriptor(&ty) else {
        return Err(DataFusionError::Plan("expected a domain value".into()));
    };
    if array.is_null(0) {
        return ScalarValue::try_from(storage);
    }
    ScalarValue::try_from_array(array.column(0), 0)
}
pub fn json_type() -> DataType {
    data_type("json", DataType::Utf8)
}
pub fn is_json(ty: &DataType) -> bool {
    descriptor(ty) == Some(("json", &DataType::Utf8))
}
pub fn json_scalar(text: &str) -> Result<ScalarValue> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| DataFusionError::Execution(format!("invalid JSON: {e}")))?;
    scalar("json", ScalarValue::Utf8(Some(value.to_string())))
}
pub fn json_text(value: &ScalarValue) -> Result<Option<String>> {
    if !is_json(&value.data_type()) {
        return Err(DataFusionError::Plan(
            "expected a JSON value; use json.parse for text".into(),
        ));
    }
    if value.is_null() {
        return Ok(None);
    }
    match storage(value)? {
        ScalarValue::Utf8(Some(text)) => Ok(Some(text)),
        _ => Err(DataFusionError::Execution(
            "non-null JSON value has null storage".into(),
        )),
    }
}
/// Restore a driver storage array to its declared domain, preserving SQL nulls.
pub fn restore(array: &ArrayRef, target: &DataType) -> Result<ArrayRef> {
    if array.data_type() == target {
        return Ok(array.clone());
    }
    let DataType::Struct(fields) = target else {
        return Err(DataFusionError::Plan("expected a domain type".into()));
    };
    let Some((name, storage_type)) = descriptor(target) else {
        return Err(DataFusionError::Plan("expected a domain type".into()));
    };
    let mut storage = arrow::compute::cast(array, storage_type)?;
    if name == "json" {
        let mut values = Vec::with_capacity(storage.len());
        for i in 0..storage.len() {
            if storage.is_null(i) {
                values.push(None);
                continue;
            }
            let ScalarValue::Utf8(Some(text)) = ScalarValue::try_from_array(&storage, i)? else {
                unreachable!()
            };
            let value = serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
                DataFusionError::Execution(format!("invalid JSON from SQL engine: {e}"))
            })?;
            values.push(Some(value.to_string()));
        }
        storage = Arc::new(arrow::array::StringArray::from(values));
    }
    Ok(Arc::new(StructArray::try_new(
        fields.clone(),
        vec![storage.clone()],
        storage.nulls().cloned(),
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn domain_identity_and_json_null_survive_arrow() {
        let json_null = json_scalar("null").unwrap();
        let sql_null = ScalarValue::try_from(&json_type()).unwrap();
        assert!(!json_null.is_null());
        assert!(sql_null.is_null());
        assert_eq!(json_text(&json_null).unwrap(), Some("null".into()));
        assert_eq!(json_text(&sql_null).unwrap(), None);
        assert_ne!(json_type(), DataType::Utf8);
        let geo = data_type("geometry", DataType::Binary);
        assert_eq!(descriptor(&geo), Some(("geometry", &DataType::Binary)));
        assert!(!is_json(&geo));
    }
}
