//! Numeric scalar function dispatch.
use super::*;

pub(super) fn call(name: &str, args: &[Value]) -> IrResult<Option<Value>> {
    match (name, args) {
        // ----- math functions used by Cypher (case-insensitive) -----
        ("abs", [v]) => Ok(Some(abs_value(v)?)),
        ("ceil", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.ceil()))
                .unwrap_or(Value::Null),
        )),
        ("floor", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.floor()))
                .unwrap_or(Value::Null),
        )),
        ("round", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.round()))
                .unwrap_or(Value::Null),
        )),
        ("sqrt", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.sqrt()))
                .unwrap_or(Value::Null),
        )),
        ("cbrt", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.cbrt()))
                .unwrap_or(Value::Null),
        )),
        ("sign", [v]) => Ok(Some(match value_as_f64(v) {
            Some(f) if f > 0.0 => Value::Int(1),
            Some(f) if f < 0.0 => Value::Int(-1),
            Some(_) => Value::Int(0),
            None => Value::Null,
        })),
        ("exp", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.exp()))
                .unwrap_or(Value::Null),
        )),
        ("ln", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.ln()))
                .unwrap_or(Value::Null),
        )),
        ("log", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.log10()))
                .unwrap_or(Value::Null),
        )),
        ("log2", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.log2()))
                .unwrap_or(Value::Null),
        )),
        ("log10", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.log10()))
                .unwrap_or(Value::Null),
        )),
        ("gamma", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(log_gamma(f).exp()))
                .unwrap_or(Value::Null),
        )),
        ("lgamma", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(log_gamma(f)))
                .unwrap_or(Value::Null),
        )),
        ("factorial", [v]) => Ok(Some(factorial_value(v))),
        ("bitwise_and", [lhs, rhs]) => Ok(Some(bitwise_i64(lhs, rhs, |a, b| a & b))),
        ("bitwise_or", [lhs, rhs]) => Ok(Some(bitwise_i64(lhs, rhs, |a, b| a | b))),
        ("bitshift_left", [lhs, rhs]) => Ok(Some(bitshift_i64(lhs, rhs, i64::checked_shl))),
        ("bitshift_right", [lhs, rhs]) => {
            Ok(Some(bitshift_i64(lhs, rhs, i64::checked_shr)))
        }
        ("sin", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.sin()))
                .unwrap_or(Value::Null),
        )),
        ("cos", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.cos()))
                .unwrap_or(Value::Null),
        )),
        ("tan", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.tan()))
                .unwrap_or(Value::Null),
        )),
        ("cot", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(1.0 / f.tan()))
                .unwrap_or(Value::Null),
        )),
        ("asin", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.asin()))
                .unwrap_or(Value::Null),
        )),
        ("acos", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.acos()))
                .unwrap_or(Value::Null),
        )),
        ("atan", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.atan()))
                .unwrap_or(Value::Null),
        )),
        ("atan2", [y, x]) => match (value_as_f64(y), value_as_f64(x)) {
            (Some(y), Some(x)) => Ok(Some(Value::Float(y.atan2(x)))),
            _ => Ok(Some(Value::Null)),
        },
        ("degrees", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.to_degrees()))
                .unwrap_or(Value::Null),
        )),
        ("radians", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| Value::Float(f.to_radians()))
                .unwrap_or(Value::Null),
        )),
        ("haversin", [v]) => Ok(Some(
            value_as_f64(v)
                .map(|f| {
                    let sin = (f / 2.0).sin();
                    Value::Float(sin * sin)
                })
                .unwrap_or(Value::Null),
        )),
        ("pi", []) => Ok(Some(Value::Float(std::f64::consts::PI))),
        ("e", []) => Ok(Some(Value::Float(std::f64::consts::E))),
        ("rand", []) => Ok(Some(Value::Float(next_kuzu_random()))),
        _ => Ok(None),
    }
}
