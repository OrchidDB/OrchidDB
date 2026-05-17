//! GraphDistinct + key-encoding signature helpers.
//!
//! Extracted from `interpreter.rs` lines 1363..1474.

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
            buf.extend_from_slice(&id.to_be_bytes());
        }
        Value::Edge { rel_type, id, .. } => {
            buf.push(6);
            buf.extend_from_slice(rel_type.as_bytes());
            buf.push(0);
            buf.extend_from_slice(&id.to_be_bytes());
        }
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
    let mut out = Vec::new();
    for mut row in rows {
        let signature: Vec<u8> = if keys.is_empty() {
            row_signature(&row)
        } else {
            keys.iter()
                .flat_map(|k| {
                    let v = row.bindings.get(k).cloned().unwrap_or(Value::Null);
                    let mut bytes = encode_value(&v);
                    bytes.push(0);
                    bytes
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
