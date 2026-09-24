//! Gremlin numeric promotion, kept separate from SQL/Cypher comparisons.
//!
//! Mirrors TinkerPop 3.7.4 NumberHelper: integral widths promote first,
//! floating point uses the widest participating width, and BigDecimal
//! converts doubles through their decimal spelling (BigDecimal.valueOf).
use super::value::Value;
use bigdecimal::BigDecimal;
use num_traits::ToPrimitive;
use std::cmp::Ordering;

pub fn numeric_width(value: &Value) -> Option<(u16, bool)> {
    Some(match value {
        Value::Byte(_) | Value::UInt8(_) => (8, false),
        Value::Short(_) | Value::UInt16(_) => (16, false),
        Value::Int(_) | Value::UInt32(_) => (32, false),
        Value::Long(_) | Value::UInt64(_) => (64, false),
        Value::BigInt(_) | Value::UInt128(_) => (128, false),
        Value::Float32(_) => (32, true),
        Value::Float(_) => (64, true),
        Value::BigDecimal(_) => (128, true),
        _ => return None,
    })
}

pub fn decimal(value: &Value) -> Option<BigDecimal> {
    Some(match value {
        Value::Byte(v) => (*v).into(),
        Value::UInt8(v) => (*v).into(),
        Value::Short(v) => (*v).into(),
        Value::UInt16(v) => (*v).into(),
        Value::Int(v) | Value::Long(v) => (*v).into(),
        Value::UInt32(v) => (*v).into(),
        Value::UInt64(v) => (*v).into(),
        Value::BigInt(v) | Value::UInt128(v) => v.clone().into(),
        Value::BigDecimal(v) => v.clone(),
        // Float is widened to double before BigDecimal.valueOf upstream.
        Value::Float32(v) => (*v as f64).to_string().parse().ok()?,
        Value::Float(v) => v.to_string().parse().ok()?,
        _ => return None,
    })
}

fn double(value: &Value) -> Option<f64> {
    match value {
        Value::Float(v) => Some(*v),
        Value::Float32(v) => Some(*v as f64),
        _ => decimal(value)?.to_f64(),
    }
}

pub fn is_nan(value: &Value) -> bool {
    matches!(value, Value::Float(v) if v.is_nan())
        || matches!(value, Value::Float32(v) if v.is_nan())
}

pub fn numeric_cmp(left: &Value, right: &Value) -> Option<Ordering> {
    let (lb, lf) = numeric_width(left)?;
    let (rb, rf) = numeric_width(right)?;
    if is_nan(left) || is_nan(right) {
        return None;
    }
    let bits = lb.max(rb);
    if (lf || rf) && bits <= 32 {
        return (double(left)? as f32).partial_cmp(&(double(right)? as f32));
    }
    if (lf || rf) && bits <= 64 {
        return double(left)?.partial_cmp(&double(right)?);
    }
    // BigDecimal has no infinities; NumberHelper orders them explicitly.
    for (value, reverse) in [(left, false), (right, true)] {
        if let Some(v) = double(value).filter(|v| v.is_infinite()) {
            if matches!(value, Value::Float(_) | Value::Float32(_)) {
                let order = if v.is_sign_negative() {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
                return Some(if reverse { order.reverse() } else { order });
            }
        }
    }
    Some(decimal(left)?.cmp(&decimal(right)?))
}

pub fn equals(left: &Value, right: &Value) -> bool {
    if numeric_width(left).is_some() || numeric_width(right).is_some() {
        return numeric_cmp(left, right) == Some(Ordering::Equal);
    }
    match (left, right) {
        (Value::List(a), Value::List(b)) | (Value::Path(a), Value::Path(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equals(a, b))
        }
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Map(a), Value::Map(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| equals(value, other)))
        }
        (Value::TypedMap(a), Value::TypedMap(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.iter()
                        .any(|(other_key, other)| equals(key, other_key) && equals(value, other))
                })
        }
        (Value::TypedMap(entries), Value::Map(map))
        | (Value::Map(map), Value::TypedMap(entries)) => {
            entries.len() == map.len()
                && entries.iter().all(|(key, value)| match key {
                    Value::String(key) => map.get(key).is_some_and(|other| equals(value, other)),
                    _ => false,
                })
        }
        (Value::MapEntry(a), Value::MapEntry(b)) => equals(&a.0, &b.0) && equals(&a.1, &b.1),
        _ => left.three_valued_eq(right) == Some(true),
    }
}

pub fn predicate(op: &str, left: &Value, right: &Value) -> Value {
    if op == "eq" || op == "neq" {
        return Value::Bool(equals(left, right) == (op == "eq"));
    }
    let order = if numeric_width(left).is_some() || numeric_width(right).is_some() {
        numeric_cmp(left, right)
    } else {
        left.three_valued_cmp(right)
    };
    order
        .map(|ord| {
            Value::Bool(match op {
                "lt" => ord.is_lt(),
                "lte" => !ord.is_gt(),
                "gt" => ord.is_gt(),
                "gte" => !ord.is_lt(),
                _ => false,
            })
        })
        .unwrap_or(Value::Null)
}

/// Total numeric order for Gremlin sort keys. NaN sorts after infinity;
/// predicate comparability still returns unknown for every NaN comparison.
pub fn compare_order_keys(left: &Value, right: &Value) -> Ordering {
    if numeric_width(left).is_some() && numeric_width(right).is_some() {
        return match (is_nan(left), is_nan(right)) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => numeric_cmp(left, right).unwrap_or(Ordering::Equal),
        };
    }
    if let (Value::List(a), Value::List(b)) = (left, right) {
        return a
            .iter()
            .zip(b)
            .map(|(a, b)| compare_order_keys(a, b))
            .find(|ord| !ord.is_eq())
            .unwrap_or_else(|| a.len().cmp(&b.len()));
    }
    crate::ir::interpreter::expr::compare_values(left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn promotion_preserves_decimal_and_uses_upstream_widths() {
        for text in ["0.2", "0.4"] {
            let d = Value::BigDecimal(text.parse().unwrap());
            assert!(equals(&d, &Value::Float(text.parse().unwrap())));
            assert_eq!(
                numeric_cmp(&d, &Value::Float(text.parse().unwrap())),
                Some(Ordering::Equal)
            );
        }
        assert!(!equals(
            &Value::BigInt("9007199254740993".parse().unwrap()),
            &Value::Float(9007199254740992.0)
        ));
        assert!(equals(
            &Value::Long(9007199254740993),
            &Value::Float(9007199254740992.0)
        ));
        assert!(!equals(
            &Value::BigDecimal("0.4".parse().unwrap()),
            &Value::Float32(0.4)
        ));
        assert!(!equals(&Value::Float(f64::NAN), &Value::Float(f64::NAN)));
        assert_eq!(
            predicate("gt", &Value::Float(f64::NAN), &Value::Int(0)),
            Value::Null
        );
        assert_eq!(
            numeric_cmp(
                &Value::Float(f64::INFINITY),
                &Value::BigDecimal("1e1000".parse().unwrap())
            ),
            Some(Ordering::Greater)
        );
        assert!(equals(
            &Value::Float(f64::INFINITY),
            &Value::Float(f64::INFINITY)
        ));
        // Shared SQL equality is deliberately unchanged.
        assert_eq!(
            Value::BigDecimal("0.4".parse().unwrap()).three_valued_eq(&Value::Float(0.4)),
            Some(false)
        );
    }
}
