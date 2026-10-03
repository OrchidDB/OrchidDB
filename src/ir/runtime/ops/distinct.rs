//! GraphDistinct + key-encoding signature helpers.

use std::collections::BTreeSet;

use crate::ir::plan::DistinctMode;
use crate::ir::value::Value;

use super::super::{IrResult, Row};

pub(crate) fn encode_key(values: &[Value]) -> Vec<u8> {
    let mut buf = Vec::new();
    for v in values {
        buf.extend(encode_value(v));
        buf.push(0);
    }
    buf
}

pub(crate) fn encode_value(v: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    match v {
        Value::Scalar(_) => return crate::ir::value::set_member_key(v),
        Value::Null => buf.push(0),
        Value::Bool(b) => {
            buf.push(1);
            buf.push(*b as u8);
        }
        Value::Byte(n) => {
            buf.push(12);
            buf.push(*n as u8);
        }
        Value::UInt8(n) => {
            buf.push(18);
            buf.push(*n);
        }
        Value::Short(n) => {
            buf.push(13);
            buf.extend_from_slice(&n.to_be_bytes());
        }
        Value::UInt16(n) => {
            buf.push(19);
            buf.extend_from_slice(&n.to_be_bytes());
        }
        Value::Int(n) => {
            buf.push(2);
            buf.extend_from_slice(&n.to_be_bytes());
        }
        Value::UInt32(n) => {
            buf.push(20);
            buf.extend_from_slice(&n.to_be_bytes());
        }
        Value::Long(n) => {
            buf.push(14);
            buf.extend_from_slice(&n.to_be_bytes());
        }
        Value::UInt64(n) => {
            buf.push(21);
            buf.extend_from_slice(&n.to_be_bytes());
        }
        Value::Float32(f) => {
            buf.push(15);
            buf.extend_from_slice(&f.to_be_bytes());
        }
        Value::Float(f) => {
            buf.push(3);
            buf.extend_from_slice(&f.to_be_bytes());
        }
        Value::Temporal(t) => { buf.push(30); buf.extend_from_slice(t.encode().as_bytes()); }
        Value::DateTime(s) => {
            buf.push(16);
            buf.extend_from_slice(s.as_bytes());
        }
        Value::String(s) => {
            buf.push(4);
            buf.extend_from_slice(s.as_bytes());
        }
        Value::InternalId { table, offset } => {
            buf.push(17);
            buf.extend_from_slice(&table.to_be_bytes());
            buf.push(0);
            buf.extend_from_slice(&offset.to_be_bytes());
        }
        Value::Node { label, id } => {
            buf.push(5);
            buf.extend_from_slice(label.as_bytes());
            buf.push(0);
            buf.extend_from_slice(&id.encode());
        }
        Value::Edge { rel_type, id, .. } => {
            buf.push(6);
            buf.extend_from_slice(rel_type.as_bytes());
            buf.push(0);
            buf.extend_from_slice(&id.encode());
        }
        Value::VertexProperty { id, .. } => {
            buf.push(0x40);
            buf.extend_from_slice(&id.to_be_bytes());
        }
        Value::Property { .. } | Value::CardinalityValue {..} => return crate::ir::value::set_member_key(v),
        Value::List(items) => {
            buf.push(7);
            for item in items {
                buf.extend(encode_value(item));
                buf.push(0xff);
            }
        }
        Value::Map(items) => {
            buf.push(8);
            for (k, val) in items {
                buf.extend_from_slice(k.as_bytes());
                buf.push(0);
                buf.extend(encode_value(val));
                buf.push(0xff);
            }
        }
        Value::Token(value) | Value::Direction(value) => {
            buf.push(if matches!(v, Value::Token(_)) { 24 } else { 25 });
            buf.extend_from_slice(&(value.len() as u64).to_be_bytes());
            buf.extend_from_slice(value.as_bytes());
        }
        Value::MapEntry(pair) => {
            buf.push(27);
            for value in [&pair.0, &pair.1] {
                let encoded = encode_value(value);
                buf.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
                buf.extend(encoded);
            }
        }
        Value::TypedMap(entries) => {
            let strings: Option<std::collections::BTreeMap<String, Value>> = entries
                .iter()
                .map(|(key, value)| match key {
                    Value::String(key) => Some((key.clone(), value.clone())),
                    _ => None,
                })
                .collect();
            if let Some(map) = strings.filter(|m| m.len() == entries.len()) {
                return encode_value(&Value::Map(map));
            }
            buf.push(23);
            let mut encoded = entries
                .iter()
                .map(|(key, value)| (encode_value(key), encode_value(value)))
                .collect::<Vec<_>>();
            encoded.sort();
            buf.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
            for (key, value) in encoded {
                buf.extend_from_slice(&(key.len() as u64).to_be_bytes());
                buf.extend(key);
                buf.extend_from_slice(&(value.len() as u64).to_be_bytes());
                buf.extend(value);
            }
        }
        Value::Set(_) => return crate::ir::value::set_member_key(v),
        Value::BulkSet(items) => {
            buf.push(26);
            let mut encoded = items.iter().map(encode_value).collect::<Vec<_>>();
            encoded.sort();
            buf.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
            for item in encoded {
                buf.extend_from_slice(&(item.len() as u64).to_be_bytes());
                buf.extend(item);
            }
        }
        Value::Path(items) => {
            buf.push(9);
            for item in items {
                buf.extend(encode_value(item));
                buf.push(0xff);
            }
        }
        Value::BigInt(n) => {
            buf.push(10);
            buf.extend_from_slice(n.to_string().as_bytes());
        }
        Value::UInt128(n) => {
            buf.push(22);
            buf.extend_from_slice(n.to_string().as_bytes());
        }
        Value::BigDecimal(d) => {
            buf.push(11);
            buf.extend_from_slice(d.to_string().as_bytes());
        }
    }
    buf
}

/// Cypher `DISTINCT`, grouping and UNION use value equivalence: numeric
/// widths are ignored and that equivalence applies recursively to lists/maps.
/// Gremlin deliberately keeps its native boxed-type identity in `encode_value`.
pub(crate) fn encode_cypher_equivalence(value: &Value) -> Vec<u8> {
    use bigdecimal::{BigDecimal, FromPrimitive};

    fn framed(tag: u8, parts: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
        let mut result = vec![tag];
        for part in parts {
            result.extend_from_slice(&(part.len() as u64).to_be_bytes());
            result.extend(part);
        }
        result
    }
    fn number(value: &Value) -> Option<Vec<u8>> {
        let decimal = match value {
            Value::Byte(v) => Some(BigDecimal::from(*v)),
            Value::UInt8(v) => Some(BigDecimal::from(*v)),
            Value::Short(v) => Some(BigDecimal::from(*v)),
            Value::UInt16(v) => Some(BigDecimal::from(*v)),
            Value::Int(v) | Value::Long(v) => Some(BigDecimal::from(*v)),
            Value::UInt32(v) => Some(BigDecimal::from(*v)),
            Value::UInt64(v) => Some(BigDecimal::from(*v)),
            Value::BigInt(v) | Value::UInt128(v) => Some(BigDecimal::from(v.clone())),
            Value::BigDecimal(v) => Some(v.clone()),
            Value::Float32(v) if v.is_nan() => return Some(b"NaN".to_vec()),
            Value::Float(v) if v.is_nan() => return Some(b"NaN".to_vec()),
            Value::Float32(v) if v.is_infinite() => {
                return Some(if v.is_sign_negative() {
                    b"-Infinity".to_vec()
                } else {
                    b"Infinity".to_vec()
                });
            }
            Value::Float(v) if v.is_infinite() => {
                return Some(if v.is_sign_negative() {
                    b"-Infinity".to_vec()
                } else {
                    b"Infinity".to_vec()
                });
            }
            Value::Float32(v) => BigDecimal::from_f32(*v),
            Value::Float(v) => BigDecimal::from_f64(*v),
            _ => None,
        }?;
        Some(framed(1, [decimal.normalized().to_string().into_bytes()]))
    }

    if let Value::Scalar(scalar) = value
        && let Some(value) = crate::ir::value::scalar_semantic_value(scalar)
    {
        return encode_cypher_equivalence(&value);
    }
    if let Some(number) = number(value) {
        return number;
    }
    match value {
        Value::Null => vec![0],
        Value::Bool(value) => framed(2, [vec![u8::from(*value)]]),
        Value::List(values) | Value::Path(values) => framed(
            if matches!(value, Value::List(_)) {
                3
            } else {
                4
            },
            values.iter().map(encode_cypher_equivalence),
        ),
        Value::Map(values) => {
            let mut entries = values
                .iter()
                .filter(|(key, _)| !key.starts_with("__"))
                .map(|(key, value)| {
                    framed(
                        5,
                        [key.as_bytes().to_vec(), encode_cypher_equivalence(value)],
                    )
                })
                .collect::<Vec<_>>();
            entries.sort();
            framed(6, entries)
        }
        // Keep type and value boundaries unambiguous for strings, graph
        // identities and other values outside Cypher's recursive containers.
        other => framed(255, [encode_value(other)]),
    }
}

pub(crate) fn distinct_op(
    keys: &[String],
    mode: DistinctMode,
    rows: Vec<Row>,
) -> IrResult<Vec<Row>> {
    let mut seen = BTreeSet::new();
    distinct_op_with_seen(keys, mode, rows, &mut seen)
}

pub(crate) fn distinct_op_with_seen(
    keys: &[String],
    mode: DistinctMode,
    rows: Vec<Row>,
    seen: &mut BTreeSet<Vec<u8>>,
) -> IrResult<Vec<Row>> {
    distinct_op_with_seen_and_cypher_equivalence(keys, mode, rows, seen, false)
}

pub(crate) fn distinct_op_with_seen_and_cypher_equivalence(
    keys: &[String],
    mode: DistinctMode,
    rows: Vec<Row>,
    seen: &mut BTreeSet<Vec<u8>>,
    cypher_equivalence: bool,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for mut row in rows {
        let signature: Vec<u8> = if keys.is_empty() {
            if cypher_equivalence {
                row_signature_with_cypher_equivalence(&row)
            } else {
                row_signature(&row)
            }
        } else {
            keys.iter()
                .flat_map(|k| {
                    let v = row.bindings.get(k).cloned().unwrap_or(Value::Null);
                    if cypher_equivalence {
                        let encoded = encode_cypher_equivalence(&v);
                        (encoded.len() as u64)
                            .to_be_bytes()
                            .into_iter()
                            .chain(encoded)
                            .collect()
                    } else {
                        let mut bytes = encode_value(&v);
                        bytes.push(0);
                        bytes
                    }
                })
                .collect()
        };
        if seen.insert(signature) {
            if matches!(mode, DistinctMode::Traverser) {
                row.bulk = 1;
            }
            out.push(row);
        }
    }
    Ok(out)
}

pub(crate) fn row_signature(row: &Row) -> Vec<u8> {
    let mut sig = Vec::new();
    for (k, v) in &row.bindings {
        sig.extend_from_slice(k.as_bytes());
        sig.push(0);
        sig.extend(encode_value(v));
        sig.push(0xff);
    }
    sig
}

pub(crate) fn row_signature_with_cypher_equivalence(row: &Row) -> Vec<u8> {
    let mut signature = Vec::new();
    for (key, value) in &row.bindings {
        signature.extend_from_slice(&(key.len() as u64).to_be_bytes());
        signature.extend_from_slice(key.as_bytes());
        let encoded = encode_cypher_equivalence(value);
        signature.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
        signature.extend(encoded);
    }
    signature
}

#[cfg(test)]
mod typed_map_tests {
    use super::*;
    #[test]
    fn typed_map_distinct_is_order_independent_and_key_typed() {
        let entries = vec![
            (Value::Token("id".into()), Value::Int(1)),
            (Value::String("id".into()), Value::Int(2)),
        ];
        let mut reverse = entries.clone();
        reverse.reverse();
        assert_eq!(
            encode_value(&Value::TypedMap(entries)),
            encode_value(&Value::TypedMap(reverse))
        );
        assert_ne!(
            encode_value(&Value::Token("id".into())),
            encode_value(&Value::String("id".into()))
        );
        assert_ne!(
            encode_value(&Value::TypedMap(vec![(Value::Int(1), Value::Null)])),
            encode_value(&Value::TypedMap(vec![(Value::Long(1), Value::Null)]))
        );
        assert_eq!(
            encode_value(&Value::TypedMap(vec![(
                Value::String("a".into()),
                Value::Int(1)
            )])),
            encode_value(&Value::Map(std::collections::BTreeMap::from([(
                "a".into(),
                Value::Int(1)
            )])))
        );
    }
}

#[cfg(test)]
mod cypher_equivalence_tests {
    use super::*;

    #[test]
    fn canonicalizes_numeric_widths_and_recursive_values() {
        let one = Value::Int(1);
        let one_float = Value::Float(1.0);
        assert_eq!(
            encode_cypher_equivalence(&one),
            encode_cypher_equivalence(&one_float)
        );
        assert_ne!(encode_value(&one), encode_value(&one_float));
        assert_eq!(
            encode_cypher_equivalence(&Value::List(vec![one.clone()])),
            encode_cypher_equivalence(&Value::List(vec![one_float.clone()]))
        );
        assert_eq!(
            encode_cypher_equivalence(&Value::Map(std::collections::BTreeMap::from([(
                "n".into(),
                one,
            )]))),
            encode_cypher_equivalence(&Value::Map(std::collections::BTreeMap::from([(
                "n".into(),
                one_float,
            )])))
        );
        assert_eq!(
            encode_cypher_equivalence(&Value::Float(-0.0)),
            encode_cypher_equivalence(&Value::Int(0))
        );
    }

    #[test]
    fn canonicalizes_nan_payloads() {
        let left = Value::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        let right = Value::Float(f64::from_bits(0x7ff8_0000_0000_0002));
        assert_eq!(
            encode_cypher_equivalence(&left),
            encode_cypher_equivalence(&right)
        );
    }
}

#[cfg(test)]
mod bulkset_tests {
    use super::*;
    #[test]
    fn native_set_distinct_is_unordered_and_keeps_member_widths() {
        let a = crate::ir::value::gremlin_set(vec![Value::Int(1), Value::Long(1)]);
        let b = crate::ir::value::gremlin_set(vec![Value::Long(1), Value::Int(1)]);
        assert_eq!(encode_value(&a), encode_value(&b));
        assert_ne!(encode_value(&a), encode_value(&Value::List(vec![Value::Int(1), Value::Long(1)])));
        assert_ne!(encode_value(&a), encode_value(&Value::BulkSet(vec![Value::Int(1), Value::Long(1)])));
        assert_ne!(encode_value(&crate::ir::value::gremlin_set(vec![Value::Int(1)])), encode_value(&crate::ir::value::gremlin_set(vec![Value::Long(1)])));
        assert_eq!(super::super::super::expr::compare_values(&a, &b), std::cmp::Ordering::Equal);
    }
    #[test]
    fn native_set_distinct_canonicalizes_nested_nan_bits() {
        let a = Value::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        let b = Value::Float(f64::from_bits(0x7ff8_0000_0000_0002));
        assert_ne!(encode_value(&a), encode_value(&b)); // Scalar keys are unchanged.
        for (a, b) in [(a.clone(), b.clone()), (Value::List(vec![a]), Value::List(vec![b]))] {
            let a = crate::ir::value::gremlin_set(vec![a]);
            let b = crate::ir::value::gremlin_set(vec![b]);
            assert_eq!(a, b);
            assert_eq!(encode_value(&a), encode_value(&b));
            assert_eq!(super::super::super::expr::compare_values(&a, &b), std::cmp::Ordering::Equal);
        }
    }
    #[test]
    fn bulkset_distinct_retains_multiplicity_and_ignores_order() {
        let a = Value::BulkSet(vec![Value::Int(1), Value::Int(2), Value::Int(1)]);
        let b = Value::BulkSet(vec![Value::Int(2), Value::Int(1), Value::Int(1)]);
        assert_eq!(encode_value(&a), encode_value(&b));
        assert_eq!(a.three_valued_eq(&b), Some(true));
        assert_ne!(
            encode_value(&a),
            encode_value(&Value::BulkSet(vec![Value::Int(1), Value::Int(2)]))
        );
        assert!(
            !crate::ir::runtime::compare_values(
                &Value::BulkSet(vec![Value::Int(1)]),
                &Value::BulkSet(vec![Value::Long(1)])
            )
            .is_eq()
        );
    }
}
