//! Numeric operations and function signatures.

use crate::ir::runtime::{RuntimeError, IrResult};
use crate::ir::value::Value;
use super::lists::cypher_list_type_name;

pub(super) fn kuzu_function_arity_error(function: &str, actual: &str, expected: &str) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Binder exception: Function {function} did not receive correct arguments:\nActual:   {actual}\nExpected: {expected}"
    ))
}

pub(super) fn format_actual_signature(args: &[Value]) -> String {
    let types = args
        .iter()
        .map(cypher_list_type_name)
        .collect::<Vec<_>>()
        .join(",");
    format!("({types})")
}

/// COALESCE with Kuzu's numeric promotion: when any argument (null or
/// not) is DOUBLE-typed, an integer result is promoted to DOUBLE so the
/// output type matches Kuzu's unified COALESCE type. Same for integer
/// lists when a double list appears among the arguments.
pub(super) fn coalesce_promoted(values: &[Value]) -> Value {
    let result = values
        .iter()
        .find(|v| !matches!(v, Value::Null))
        .cloned()
        .unwrap_or(Value::Null);
    let any_float = values
        .iter()
        .any(|v| matches!(v, Value::Float(_) | Value::Float32(_)));
    let any_float_list = values.iter().any(|v| {
        matches!(v, Value::List(items) if items
            .iter()
            .any(|item| matches!(item, Value::Float(_) | Value::Float32(_))))
    });
    match result {
        Value::Int(n) if any_float => Value::Float(n as f64),
        Value::Long(n) if any_float => Value::Float(n as f64),
        Value::List(items) if any_float_list => Value::List(
            items
                .into_iter()
                .map(|item| match item {
                    Value::Int(n) => Value::Float(n as f64),
                    Value::Long(n) => Value::Float(n as f64),
                    other => other,
                })
                .collect(),
        ),
        other => other,
    }
}

/// Kuzu typed NEGATE: unsigned integers wrap modulo 2^n; signed MIN
/// values raise an overflow error; everything else negates normally.
pub(super) fn negate_typed_value(value: &Value) -> IrResult<Value> {
    fn min_negate_error(v: impl std::fmt::Display, ty: &str) -> RuntimeError {
        RuntimeError::Runtime(format!(
            "Overflow exception: Value {v} cannot be negated within {ty} range."
        ))
    }
    Ok(match value {
        Value::Null => Value::Null,
        Value::UInt8(v) => Value::UInt8(v.wrapping_neg()),
        Value::UInt16(v) => Value::UInt16(v.wrapping_neg()),
        Value::UInt32(v) => Value::UInt32(v.wrapping_neg()),
        Value::UInt64(v) => Value::UInt64(v.wrapping_neg()),
        Value::UInt128(v) => {
            let modulus: num_bigint::BigInt = num_bigint::BigInt::from(1u8) << 128;
            let reduced: num_bigint::BigInt = v.clone() % modulus.clone();
            let wrapped: num_bigint::BigInt = (modulus.clone() - reduced) % modulus;
            Value::UInt128(wrapped)
        }
        Value::Byte(v) => {
            if *v == i8::MIN {
                return Err(min_negate_error(v, "INT8"));
            }
            Value::Byte(-v)
        }
        Value::Short(v) => {
            if *v == i16::MIN {
                return Err(min_negate_error(v, "INT16"));
            }
            Value::Short(-v)
        }
        Value::Int(v) => {
            if *v == i32::MIN as i64 {
                return Err(min_negate_error(v, "INT32"));
            }
            Value::Int(-v)
        }
        Value::Long(v) => {
            if *v == i64::MIN {
                return Err(min_negate_error(v, "INT64"));
            }
            Value::Long(-v)
        }
        Value::Float32(v) => Value::Float32(-v),
        Value::Float(v) => Value::Float(-v),
        Value::BigInt(v) => Value::BigInt(-v.clone()),
        Value::BigDecimal(v) => Value::BigDecimal(-v.clone()),
        other => {
            return Err(RuntimeError::Type(format!(
                "negate on {}",
                other.type_name()
            )));
        }
    })
}

pub(super) fn abs_value(value: &Value) -> IrResult<Value> {
    match value {
        Value::BigInt(n) => {
            use num_traits::Signed;
            Ok(Value::BigInt(n.abs()))
        }
        Value::UInt128(n) => Ok(Value::UInt128(n.clone())),
        Value::BigDecimal(n) => Ok(Value::BigDecimal(n.abs())),
        Value::Byte(n) if *n == i8::MIN => Err(abs_overflow_error(*n as i128, "INT8")),
        Value::Byte(n) => Ok(Value::Int((*n as i64).abs())),
        Value::UInt8(n) => Ok(Value::UInt8(*n)),
        Value::Short(n) if *n == i16::MIN => Err(abs_overflow_error(*n as i128, "INT16")),
        Value::Short(n) => Ok(Value::Int((*n as i64).abs())),
        Value::UInt16(n) => Ok(Value::UInt16(*n)),
        Value::Int(n) if *n == i32::MIN as i64 => Err(abs_overflow_error(*n as i128, "INT32")),
        Value::Int(n) => n
            .checked_abs()
            .map(Value::Long)
            .ok_or_else(|| abs_overflow_error(*n as i128, "INT64")),
        Value::UInt32(n) => Ok(Value::UInt32(*n)),
        Value::Long(n) => n
            .checked_abs()
            .map(Value::Long)
            .ok_or_else(|| abs_overflow_error(*n as i128, "INT64")),
        Value::UInt64(n) => Ok(Value::UInt64(*n)),
        Value::Float32(f) => Ok(Value::Float((*f as f64).abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        _ => Ok(Value::Null),
    }
}

fn abs_overflow_error(value: i128, type_name: &str) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Overflow exception: Cannot take the absolute value of {value} within {type_name} range."
    ))
}

pub(super) fn value_as_f64(value: &Value) -> Option<f64> {
    use num_traits::ToPrimitive;
    match value {
        Value::Byte(n) => Some(*n as f64),
        Value::UInt8(n) => Some(*n as f64),
        Value::Short(n) => Some(*n as f64),
        Value::UInt16(n) => Some(*n as f64),
        Value::Int(n) | Value::Long(n) => Some(*n as f64),
        Value::UInt32(n) => Some(*n as f64),
        Value::UInt64(n) => Some(*n as f64),
        Value::Float32(n) => Some(*n as f64),
        Value::Float(n) => Some(*n),
        Value::BigInt(n) | Value::UInt128(n) => n.to_f64(),
        Value::BigDecimal(n) => n.to_f64(),
        _ => None,
    }
}

pub(super) fn value_as_i64_exact(value: &Value) -> Option<i64> {
    use num_traits::ToPrimitive;
    match value {
        Value::Byte(n) => Some(*n as i64),
        Value::UInt8(n) => Some(*n as i64),
        Value::Short(n) => Some(*n as i64),
        Value::UInt16(n) => Some(*n as i64),
        Value::Int(n) | Value::Long(n) => Some(*n),
        Value::UInt32(n) => Some(*n as i64),
        Value::UInt64(n) => i64::try_from(*n).ok(),
        Value::BigInt(n) | Value::UInt128(n) => n.to_i64(),
        _ => None,
    }
}

pub(super) fn add_i64_delta(value: &Value, delta: i64) -> Value {
    value_as_i64_exact(value)
        .map(|value| Value::Long(value + delta))
        .unwrap_or(Value::Null)
}

pub(super) fn add_i64_values(left: &Value, right: &Value) -> Value {
    match (value_as_i64_exact(left), value_as_i64_exact(right)) {
        (Some(left), Some(right)) => Value::Long(left + right),
        _ => Value::Null,
    }
}

pub(super) fn factorial_value(value: &Value) -> Value {
    let Some(n) = value_as_i64_exact(value) else {
        return Value::Null;
    };
    if n < 0 {
        return Value::Null;
    }

    let mut acc = 1_i64;
    for factor in 2..=n {
        let Some(next) = acc.checked_mul(factor) else {
            return Value::Null;
        };
        acc = next;
    }
    Value::Long(acc)
}

pub(super) fn bitwise_i64(lhs: &Value, rhs: &Value, op: impl FnOnce(i64, i64) -> i64) -> Value {
    match (value_as_i64_exact(lhs), value_as_i64_exact(rhs)) {
        (Some(lhs), Some(rhs)) => Value::Long(op(lhs, rhs)),
        _ => Value::Null,
    }
}

pub(super) fn bitshift_i64(lhs: &Value, rhs: &Value, op: impl FnOnce(i64, u32) -> Option<i64>) -> Value {
    let (Some(lhs), Some(rhs)) = (value_as_i64_exact(lhs), value_as_i64_exact(rhs)) else {
        return Value::Null;
    };
    let Ok(rhs) = u32::try_from(rhs) else {
        return Value::Null;
    };
    op(lhs, rhs).map(Value::Long).unwrap_or(Value::Null)
}

pub(super) fn next_even_number(value: f64) -> f64 {
    if !value.is_finite() {
        return f64::NAN;
    }
    let candidate = value.ceil();
    if (candidate as i128) % 2 == 0 {
        candidate
    } else {
        candidate + 1.0
    }
}

pub(super) fn log_gamma(value: f64) -> f64 {
    if value.is_nan() || value <= 0.0 {
        return f64::NAN;
    }

    // Lanczos approximation with g=7, coefficients from Numerical Recipes.
    // The arithmetic-function cases only exercise positive inputs, which keeps
    // this compact and avoids pretending to support gamma's poles/reflection.
    const COEFFICIENTS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    let z = value - 1.0;
    let mut x = COEFFICIENTS[0];
    for (idx, coefficient) in COEFFICIENTS.iter().enumerate().skip(1) {
        x += coefficient / (z + idx as f64);
    }
    let t = z + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + x.ln()
}

pub(super) fn value_as_bigint(value: &Value) -> Option<num_bigint::BigInt> {
    use num_bigint::BigInt;
    use num_traits::ToPrimitive;

    match value {
        Value::Byte(n) => Some(BigInt::from(*n)),
        Value::UInt8(n) => Some(BigInt::from(*n)),
        Value::Short(n) => Some(BigInt::from(*n)),
        Value::UInt16(n) => Some(BigInt::from(*n)),
        Value::Int(n) | Value::Long(n) => Some(BigInt::from(*n)),
        Value::UInt32(n) => Some(BigInt::from(*n)),
        Value::UInt64(n) => Some(BigInt::from(*n)),
        Value::BigInt(n) | Value::UInt128(n) => Some(n.clone()),
        Value::BigDecimal(n) => n.to_i128().map(BigInt::from),
        Value::Bool(true) => Some(BigInt::from(1)),
        Value::Bool(false) => Some(BigInt::from(0)),
        _ => None,
    }
}
