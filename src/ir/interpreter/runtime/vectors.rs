//! Numeric vector operations.

use crate::ir::value::Value;
use super::maps::runtime_list;
use super::numeric::value_as_f64;

pub(super) fn numeric_vector(value: &Value) -> Option<Vec<f64>> {
    runtime_list(value)?
        .iter()
        .map(value_as_f64)
        .collect::<Option<Vec<_>>>()
}

pub(super) fn float_vector(value: &Value) -> Option<Vec<f64>> {
    let items = runtime_list(value)?;
    if !items
        .iter()
        .any(|item| matches!(item, Value::Float32(_) | Value::Float(_)))
    {
        return None;
    }
    items.iter().map(value_as_f64).collect::<Option<Vec<_>>>()
}

pub(super) fn array_cross_product_value(left: &[Value], right: &[Value]) -> Value {
    let has_supported_type = signed_integer_vector(left).is_some()
        && signed_integer_vector(right).is_some()
        || float_numeric_vector(left).is_some() && float_numeric_vector(right).is_some();
    if !has_supported_type {
        return Value::String(
            "Binder exception: ARRAY_CROSS_PRODUCT can only be applied on array of floating points or signed integers"
                .to_string(),
        );
    }
    if left.len() != 3 || right.len() != 3 {
        return Value::String(
            "Binder exception: ARRAY_CROSS_PRODUCT requires both arrays to have the same element type and size of 3"
                .to_string(),
        );
    }
    if left.iter().all(|item| matches!(item, Value::Short(_)))
        && right.iter().all(|item| matches!(item, Value::Short(_)))
    {
        return Value::List(cross_product_i16(left, right));
    }
    if let (Some(left), Some(right)) = (signed_integer_vector(left), signed_integer_vector(right)) {
        return Value::List(cross_product_bigint(&left, &right));
    }
    if let (Some(left), Some(right)) = (float_numeric_vector(left), float_numeric_vector(right)) {
        return Value::List(cross_product_f64(&left, &right));
    }
    Value::String(
        "Binder exception: ARRAY_CROSS_PRODUCT can only be applied on array of floating points or signed integers"
            .to_string(),
    )
}

fn cross_product_i16(left: &[Value], right: &[Value]) -> Vec<Value> {
    let to_i16 = |value: &Value| match value {
        Value::Short(n) => *n,
        _ => 0,
    };
    let left = [to_i16(&left[0]), to_i16(&left[1]), to_i16(&left[2])];
    let right = [to_i16(&right[0]), to_i16(&right[1]), to_i16(&right[2])];
    vec![
        Value::Short(
            left[1]
                .wrapping_mul(right[2])
                .wrapping_sub(left[2].wrapping_mul(right[1])),
        ),
        Value::Short(
            left[2]
                .wrapping_mul(right[0])
                .wrapping_sub(left[0].wrapping_mul(right[2])),
        ),
        Value::Short(
            left[0]
                .wrapping_mul(right[1])
                .wrapping_sub(left[1].wrapping_mul(right[0])),
        ),
    ]
}

fn signed_integer_vector(items: &[Value]) -> Option<Vec<num_bigint::BigInt>> {
    use num_bigint::BigInt;
    Some(
        items
            .iter()
            .map(|item| match item {
                Value::Byte(n) => Some(BigInt::from(*n)),
                Value::Short(n) => Some(BigInt::from(*n)),
                Value::Int(n) | Value::Long(n) => Some(BigInt::from(*n)),
                Value::BigInt(n) => Some(n.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?,
    )
}

fn float_numeric_vector(items: &[Value]) -> Option<Vec<f64>> {
    if !items
        .iter()
        .any(|item| matches!(item, Value::Float32(_) | Value::Float(_)))
    {
        return None;
    }
    items.iter().map(value_as_f64).collect::<Option<Vec<_>>>()
}

fn cross_product_bigint(left: &[num_bigint::BigInt], right: &[num_bigint::BigInt]) -> Vec<Value> {
    vec![
        Value::BigInt(&left[1] * &right[2] - &left[2] * &right[1]),
        Value::BigInt(&left[2] * &right[0] - &left[0] * &right[2]),
        Value::BigInt(&left[0] * &right[1] - &left[1] * &right[0]),
    ]
}

fn cross_product_f64(left: &[f64], right: &[f64]) -> Vec<Value> {
    vec![
        Value::Float(left[1] * right[2] - left[2] * right[1]),
        Value::Float(left[2] * right[0] - left[0] * right[2]),
        Value::Float(left[0] * right[1] - left[1] * right[0]),
    ]
}
