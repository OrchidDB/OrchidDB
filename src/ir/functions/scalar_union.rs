//! Lossless heterogeneous primitive properties. Each payload keeps its native
//! width; the tag is never inferred from a string representation.
use crate::ir::Value;
use arrow::{
    array::StructArray,
    datatypes::{DataType, Field},
};
use datafusion::common::{Result, ScalarValue};
use std::sync::Arc;

pub const TAG: &str = "__cypher_scalar_tag";
pub const INTEGER: &str = "integer";
pub const STRING: &str = "string";
pub const BOOLEAN: &str = "boolean";
pub fn data_type() -> DataType {
    DataType::Struct(
        vec![
            Field::new(TAG, DataType::Int8, false),
            Field::new(INTEGER, DataType::Int64, true),
            Field::new(STRING, DataType::Utf8, true),
            Field::new(BOOLEAN, DataType::Boolean, true),
        ]
        .into(),
    )
}
pub fn is_type(ty: &DataType) -> bool {
    let DataType::Struct(actual) = ty else {
        return false;
    };
    let DataType::Struct(expected) = data_type() else {
        unreachable!()
    };
    // SQL engines may mark every struct child nullable when reading a schema.
    // Field names, order and native payload types identify this reserved carrier.
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected.iter())
            .all(|(a, b)| a.name() == b.name() && a.data_type() == b.data_type())
}
pub fn supports_type(ty: &DataType) -> bool {
    is_type(ty)
        || matches!(
            ty,
            DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::Utf8
                | DataType::Boolean
                | DataType::Null
        )
}
pub fn scalar(value: &Value) -> Result<ScalarValue> {
    if matches!(value, Value::Null) {
        return ScalarValue::try_from(&data_type());
    }
    let mut values = vec![
        ScalarValue::Int8(None),
        ScalarValue::Int64(None),
        ScalarValue::Utf8(None),
        ScalarValue::Boolean(None),
    ];
    let (tag, payload) = match value {
        Value::Int(n) | Value::Long(n) => (1, ScalarValue::Int64(Some(*n))),
        Value::Byte(n) => (1, ScalarValue::Int64(Some(i64::from(*n)))),
        Value::Short(n) => (1, ScalarValue::Int64(Some(i64::from(*n)))),
        Value::String(s) => (2, ScalarValue::Utf8(Some(s.clone()))),
        Value::Bool(b) => (3, ScalarValue::Boolean(Some(*b))),
        _ => {
            return Err(datafusion::common::DataFusionError::Plan(
                "unsupported heterogeneous property kind".into(),
            ));
        }
    };
    values[0] = ScalarValue::Int8(Some(tag));
    values[tag as usize] = payload;
    let DataType::Struct(fields) = data_type() else {
        unreachable!()
    };
    Ok(ScalarValue::Struct(Arc::new(StructArray::try_new(
        fields,
        values.iter().map(|v| v.to_array()).collect::<Result<_>>()?,
        None,
    )?)))
}
pub fn decode(value: &ScalarValue) -> Option<Value> {
    if !is_type(&value.data_type()) {
        return None;
    }
    if value.is_null() {
        return Some(Value::Null);
    }
    let ScalarValue::Struct(a) = value else {
        return None;
    };
    let ScalarValue::Int8(Some(tag)) = ScalarValue::try_from_array(a.column(0), 0).ok()? else {
        return None;
    };
    if !(1..=3).contains(&tag) {
        return None;
    }
    // Reject malformed payloads rather than silently interpreting a missing
    // payload as null or ignoring a second active kind.
    for index in 1..=3 {
        let payload = ScalarValue::try_from_array(a.column(index), 0).ok()?;
        if payload.is_null() == (index == tag as usize) {
            return None;
        }
    }
    crate::ir::value::scalar_semantic_value(
        &ScalarValue::try_from_array(a.column(tag as usize), 0).ok()?,
    )
}
