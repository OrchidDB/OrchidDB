//! Total ordering for sort/distinct.
//!
//! Extracted from `interpreter.rs` lines 2847..2890.

use crate::ir::value::Value;

/// Tag for TinkerPop "orderability" cross-type total order.
/// Lower tag = sorts earlier. Types not represented in `Value` today
/// would slot into this table when their runtime variants are added.
fn orderability_tag(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Byte(_)
        | Value::Short(_)
        | Value::Int(_)
        | Value::Long(_)
        | Value::Float32(_)
        | Value::Float(_)
        | Value::BigInt(_)
        | Value::BigDecimal(_) => 2,
        Value::DateTime(_) => 3,
        Value::String(_) => 4,
        Value::List(_) => 5,
        Value::Map(_) => 6,
        Value::Node { .. } => 7,
        Value::Edge { .. } => 8,
        Value::Path(_) => 9,
    }
}

pub(crate) fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    // Cross-type: fall back to the tag-based total order. Same-tag pairs
    // are handled below; anything that falls through to the catch-all has
    // matching tags but no intra-type comparator (e.g. List vs List with
    // incomparable elements) and gets `Equal`.
    let (ta, tb) = (orderability_tag(a), orderability_tag(b));
    if ta != tb {
        return ta.cmp(&tb);
    }
    if ta == 2 {
        return compare_numeric_values(a, b);
    }
    match (a, b) {
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::DateTime(x), Value::DateTime(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::List(x), Value::List(y)) => compare_slices(x, y),
        (Value::Map(x), Value::Map(y)) => compare_maps(x, y),
        (Value::Node { label: lx, id: ix }, Value::Node { label: ly, id: iy }) => {
            (lx, ix).cmp(&(ly, iy))
        }
        (
            Value::Edge {
                rel_type: tx,
                id: ix,
                src_label: slx,
                src_id: six,
                dst_label: dlx,
                dst_id: dix,
            },
            Value::Edge {
                rel_type: ty,
                id: iy,
                src_label: sly,
                src_id: siy,
                dst_label: dly,
                dst_id: diy,
            },
        ) => (tx, ix, slx, six, dlx, dix).cmp(&(ty, iy, sly, siy, dly, diy)),
        (Value::Path(x), Value::Path(y)) => compare_slices(x, y),
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

fn compare_numeric_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use bigdecimal::{BigDecimal, FromPrimitive};
    use std::cmp::Ordering;

    fn non_finite_rank(value: &Value) -> Option<u8> {
        let n = match value {
            Value::Float32(n) => *n as f64,
            Value::Float(n) => *n,
            _ => return None,
        };
        if n.is_finite() {
            None
        } else if n == f64::NEG_INFINITY {
            Some(0)
        } else if n == f64::INFINITY {
            Some(2)
        } else {
            Some(3)
        }
    }

    fn decimal(value: &Value) -> Option<BigDecimal> {
        match value {
            Value::Byte(n) => Some(BigDecimal::from(*n)),
            Value::Short(n) => Some(BigDecimal::from(*n)),
            Value::Int(n) => Some(BigDecimal::from(*n)),
            Value::Long(n) => Some(BigDecimal::from(*n)),
            Value::Float32(n) => BigDecimal::from_f32(*n),
            Value::Float(n) => BigDecimal::from_f64(*n),
            Value::BigInt(n) => Some(BigDecimal::from(n.clone())),
            Value::BigDecimal(n) => Some(n.clone()),
            _ => None,
        }
    }

    match (non_finite_rank(a), non_finite_rank(b)) {
        (Some(a), Some(b)) => return a.cmp(&b),
        (Some(a), None) => {
            return if a == 0 {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        (None, Some(b)) => {
            return if b == 0 {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }
        (None, None) => {}
    }

    match (decimal(a), decimal(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => Ordering::Equal,
    }
}

fn compare_slices(a: &[Value], b: &[Value]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let cmp = compare_values(left, right);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
    }
    a.len().cmp(&b.len())
}

fn compare_maps(
    a: &std::collections::BTreeMap<String, Value>,
    b: &std::collections::BTreeMap<String, Value>,
) -> std::cmp::Ordering {
    for ((ak, av), (bk, bv)) in a.iter().zip(b.iter()) {
        let key_cmp = ak.cmp(bk);
        if key_cmp != std::cmp::Ordering::Equal {
            return key_cmp;
        }
        let value_cmp = compare_values(av, bv);
        if value_cmp != std::cmp::Ordering::Equal {
            return value_cmp;
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::compare_values;
    use crate::ir::value::Value;

    #[test]
    fn orderability_places_datetime_before_string() {
        assert!(
            compare_values(
                &Value::DateTime("2023-01-01T00:00:00Z".into()),
                &Value::String("bar".into())
            )
            .is_lt()
        );
    }

    #[test]
    fn orderability_sorts_non_finite_numbers() {
        let mut values = vec![
            Value::Float(f64::INFINITY),
            Value::Float(f64::NAN),
            Value::Float(f64::NEG_INFINITY),
            Value::Int(1),
        ];
        values.sort_by(compare_values);
        assert!(matches!(values[0], Value::Float(n) if n == f64::NEG_INFINITY));
        assert_eq!(values[1], Value::Int(1));
        assert!(matches!(values[2], Value::Float(n) if n == f64::INFINITY));
        assert!(matches!(values[3], Value::Float(n) if n.is_nan()));
    }

    #[test]
    fn orderability_compares_lists_and_maps_lexicographically() {
        assert!(
            compare_values(
                &Value::List(vec![Value::String("a".into())]),
                &Value::List(vec![Value::String("a".into()), Value::String("b".into())])
            )
            .is_lt()
        );

        let mut left = BTreeMap::new();
        left.insert("a".into(), Value::String("a".into()));
        left.insert("b".into(), Value::Bool(false));
        let mut right = BTreeMap::new();
        right.insert("a".into(), Value::String("a".into()));
        right.insert("b".into(), Value::String("b".into()));
        assert!(compare_values(&Value::Map(left), &Value::Map(right)).is_lt());
    }
}
