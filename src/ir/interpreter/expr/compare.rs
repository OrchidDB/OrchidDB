//! Total ordering for sort/distinct.
//!
//! Extracted from `interpreter.rs` lines 2847..2890.

use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY, Value};

use super::super::runtime::temporal;

/// Tag for TinkerPop "orderability" cross-type total order.
/// Lower tag = sorts earlier. Types not represented in `Value` today
/// would slot into this table when their runtime variants are added.
fn orderability_tag(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Byte(_)
        | Value::UInt8(_)
        | Value::Short(_)
        | Value::UInt16(_)
        | Value::Int(_)
        | Value::UInt32(_)
        | Value::Long(_)
        | Value::UInt64(_)
        | Value::Float32(_)
        | Value::Float(_)
        | Value::BigInt(_)
        | Value::UInt128(_)
        | Value::BigDecimal(_) => 2,
        Value::DateTime(_) => 3,
        Value::InternalId { .. } => 4,
        Value::String(_) => 5,
        Value::List(_) => 6,
        Value::Map(_) => 7,
        Value::Node { .. } => 8,
        Value::Edge { .. } => 9,
        Value::Path(_) => 10,
    }
}

pub(crate) fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if let Some(ordering) = temporal_value_ordering(a, b) {
        return ordering;
    }
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
        (Value::String(x), Value::String(y)) => {
            blob_string_ordering(x, y).unwrap_or_else(|| x.cmp(y))
        }
        (Value::DateTime(x), Value::DateTime(y)) => x.cmp(y),
        (
            Value::InternalId {
                table: tx,
                offset: ox,
            },
            Value::InternalId {
                table: ty,
                offset: oy,
            },
        ) => (tx, ox).cmp(&(ty, oy)),
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
                ..
            },
            Value::Edge {
                rel_type: ty,
                id: iy,
                src_label: sly,
                src_id: siy,
                dst_label: dly,
                dst_id: diy,
                ..
            },
        ) => (tx, ix, slx, six, dlx, dix).cmp(&(ty, iy, sly, siy, dly, diy)),
        (Value::Path(x), Value::Path(y)) => compare_slices(x, y),
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

fn blob_string_ordering(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    if !left.contains("\\x") && !right.contains("\\x") {
        return None;
    }
    Some(blob_sort_bytes(left)?.cmp(&blob_sort_bytes(right)?))
}

fn blob_sort_bytes(text: &str) -> Option<Vec<u8>> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut bytes = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if !ch.is_ascii() {
            return None;
        }
        if ch == '\\' && matches!(chars.get(index + 1), Some('x' | 'X')) {
            let first = chars.get(index + 2).copied()?;
            let second = chars.get(index + 3).copied()?;
            if !first.is_ascii_hexdigit() || !second.is_ascii_hexdigit() {
                return None;
            }
            let hex = format!("{first}{second}");
            bytes.push(u8::from_str_radix(&hex, 16).ok()?);
            index += 4;
            continue;
        }
        bytes.push(ch as u8);
        index += 1;
    }
    Some(bytes)
}

fn temporal_value_ordering(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::DateTime(left), Value::DateTime(right))
        | (Value::DateTime(left), Value::String(right))
        | (Value::String(left), Value::DateTime(right))
        | (Value::String(left), Value::String(right)) => {
            if let (Some(left), Some(right)) = (
                temporal::temporal_sort_key(left),
                temporal::temporal_sort_key(right),
            ) {
                return Some(left.cmp(&right));
            }
            if let (Some(left), Some(right)) = (
                temporal::interval_sort_key(left),
                temporal::interval_sort_key(right),
            ) {
                return Some(left.cmp(&right));
            }
            None
        }
        _ => None,
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
            Value::UInt8(n) => Some(BigDecimal::from(*n)),
            Value::Short(n) => Some(BigDecimal::from(*n)),
            Value::UInt16(n) => Some(BigDecimal::from(*n)),
            Value::Int(n) => Some(BigDecimal::from(*n)),
            Value::UInt32(n) => Some(BigDecimal::from(*n)),
            Value::Long(n) => Some(BigDecimal::from(*n)),
            Value::UInt64(n) => Some(BigDecimal::from(*n)),
            Value::Float32(n) => BigDecimal::from_f32(*n),
            Value::Float(n) => BigDecimal::from_f64(*n),
            Value::BigInt(n) => Some(BigDecimal::from(n.clone())),
            Value::UInt128(n) => Some(BigDecimal::from(n.clone())),
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
    let a_keys = ordered_map_keys(a);
    let b_keys = ordered_map_keys(b);
    for (ak, bk) in a_keys.iter().zip(b_keys.iter()) {
        let key_cmp = ak.cmp(bk);
        if key_cmp != std::cmp::Ordering::Equal {
            return key_cmp;
        }
        let Some(av) = a.get(ak) else {
            return std::cmp::Ordering::Less;
        };
        let Some(bv) = b.get(bk) else {
            return std::cmp::Ordering::Greater;
        };
        let value_cmp = compare_values(av, bv);
        if value_cmp != std::cmp::Ordering::Equal {
            return value_cmp;
        }
    }
    a_keys.len().cmp(&b_keys.len())
}

fn ordered_map_keys(map: &std::collections::BTreeMap<String, Value>) -> Vec<String> {
    if let Some(Value::List(order)) = map.get(STRUCT_ORDER_KEY) {
        let keys = order
            .iter()
            .filter_map(|item| match item {
                Value::String(key) if map.contains_key(key) => Some(key.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !keys.is_empty() {
            return keys;
        }
    }
    map.keys()
        .filter(|key| key.as_str() != STRUCT_ORDER_KEY && key.as_str() != STRUCT_TYPES_KEY)
        .cloned()
        .collect()
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
