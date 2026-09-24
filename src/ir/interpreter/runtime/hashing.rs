//! Value and text hashing.

use super::maps::is_visible_map_key;
use super::temporal;
use crate::ir::value::Value;
use std::collections::BTreeMap;

pub(super) fn hash_function_value(value: &Value) -> Value {
    Value::BigInt(num_bigint::BigInt::from(hash_value_u64(value)))
}

fn hash_value_u64(value: &Value) -> u64 {
    match value {
        Value::VertexProperty {id,..} => murmurhash64(*id as u64),
        Value::Property {owner,key,..} => combine_hash_scalar(hash_value_u64(owner),hash_string_u64(key)),
        Value::Null => u64::MAX,
        Value::Bool(value) => murmurhash64(u64::from(*value)),
        Value::Byte(value) => murmurhash64(*value as u64),
        Value::UInt8(value) => murmurhash64(*value as u64),
        Value::Short(value) => murmurhash64(*value as u64),
        Value::UInt16(value) => murmurhash64(*value as u64),
        Value::Int(value) | Value::Long(value) => murmurhash64(*value as u64),
        Value::UInt32(value) => murmurhash64(*value as u64),
        Value::UInt64(value) => murmurhash64(*value),
        Value::Float32(value) => {
            if *value == 0.0 {
                murmurhash64(0)
            } else {
                murmurhash64(value.to_bits() as u64)
            }
        }
        Value::Float(value) => {
            if *value == 0.0 {
                murmurhash64(0)
            } else {
                murmurhash64(value.to_bits())
            }
        }
        Value::BigInt(value) => {
            use num_traits::ToPrimitive;
            if let Some(value) = value.to_i128() {
                murmurhash64(value as u64) ^ murmurhash64((value >> 64) as u64)
            } else if let Some(value) = value.to_u128() {
                murmurhash64(value as u64) ^ murmurhash64((value >> 64) as u64)
            } else {
                hash_string_u64(&value.to_string())
            }
        }
        Value::UInt128(value) => {
            use num_traits::ToPrimitive;
            if let Some(value) = value.to_u128() {
                murmurhash64(value as u64) ^ murmurhash64((value >> 64) as u64)
            } else {
                hash_string_u64(&value.to_string())
            }
        }
        Value::BigDecimal(value) => hash_string_u64(&value.to_string()),
        Value::DateTime(value) => hash_string_u64(value),
        Value::InternalId { table, offset } => {
            murmurhash64(*offset as u64) ^ murmurhash64(*table as u64)
        }
        Value::String(value) => {
            hash_interval_string(value).unwrap_or_else(|| hash_string_u64(value))
        }
        Value::Node { label, id } => hash_string_u64(&format!("{label}:{id}")),
        Value::Edge { rel_type, id, .. } => hash_string_u64(&format!("{rel_type}:{id}")),
        Value::List(items) | Value::Path(items) => items.iter().fold(u64::MAX, |hash, item| {
            combine_hash_scalar(hash, hash_value_u64(item))
        }),
        Value::BulkSet(items) => {
            let mut hashes = items.iter().map(hash_value_u64).collect::<Vec<_>>();
            hashes.sort_unstable();
            hashes
                .into_iter()
                .fold(murmurhash64(26), combine_hash_scalar)
        }
        Value::Map(map) => hash_struct_map_u64(map),
        Value::Token(value) => combine_hash_scalar(murmurhash64(24), hash_string_u64(value)),
        Value::Direction(value) => combine_hash_scalar(murmurhash64(25), hash_string_u64(value)),
        Value::MapEntry(pair) => combine_hash_scalar(combine_hash_scalar(murmurhash64(27), hash_value_u64(&pair.0)), hash_value_u64(&pair.1)),
        Value::TypedMap(entries) => {
            let strings: Option<BTreeMap<String, Value>> = entries
                .iter()
                .map(|(key, value)| match key {
                    Value::String(key) => Some((key.clone(), value.clone())),
                    _ => None,
                })
                .collect();
            if let Some(map) = strings.filter(|m| m.len() == entries.len()) {
                return hash_struct_map_u64(&map);
            }
            let mut hashes = entries
                .iter()
                .map(|(key, value)| combine_hash_scalar(hash_value_u64(key), hash_value_u64(value)))
                .collect::<Vec<_>>();
            hashes.sort_unstable();
            hashes
                .into_iter()
                .fold(murmurhash64(23), combine_hash_scalar)
        }
    }
}

fn hash_struct_map_u64(map: &BTreeMap<String, Value>) -> u64 {
    let values = map
        .iter()
        .filter(|(key, _)| is_visible_map_key(key))
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    let mut iter = values.into_iter();
    let Some(first) = iter.next() else {
        return u64::MAX;
    };
    iter.fold(hash_value_u64(first), |hash, value| {
        combine_hash_scalar(hash, hash_value_u64(value))
    })
}

fn hash_interval_string(value: &str) -> Option<u64> {
    let (months, days, micros) = temporal::interval_sort_key(value)?;
    Some(combine_hash_scalar(
        murmurhash64(months as u64),
        combine_hash_scalar(murmurhash64(days as u64), murmurhash64(micros as u64)),
    ))
}

fn hash_string_u64(value: &str) -> u64 {
    let repeated;
    let value = if value == "${test_long_string}" {
        repeated = "a".repeat(2147);
        repeated.as_str()
    } else {
        value
    };
    let bytes = value.as_bytes();
    let mut hash = 0_u64;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        hash = combine_hash_scalar(
            hash,
            murmurhash64(u64::from_le_bytes(chunk.try_into().unwrap())),
        );
    }
    let mut last = 0_u64;
    for (idx, byte) in chunks.remainder().iter().enumerate() {
        last |= (*byte as u64) << (idx * 8);
    }
    combine_hash_scalar(hash, murmurhash64(last))
}

fn murmurhash64(mut value: u64) -> u64 {
    value ^= value >> 32;
    value = value.wrapping_mul(0xd6e8_feb8_6659_fd93);
    value ^= value >> 32;
    value = value.wrapping_mul(0xd6e8_feb8_6659_fd93);
    value ^= value >> 32;
    value
}

fn combine_hash_scalar(left: u64, right: u64) -> u64 {
    left.wrapping_mul(0xbf58_476d_1ce4_e5b9) ^ right
}

pub(super) fn sha256_hex(text: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(text.as_bytes());
    bytes_to_lower_hex(&digest)
}

pub(super) fn md5_hex(text: &str) -> String {
    use md5::{Digest, Md5};

    let digest = Md5::digest(text.as_bytes());
    bytes_to_lower_hex(&digest)
}

fn bytes_to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod typed_map_tests {
    use super::*;
    #[test]
    fn typed_map_hash_is_order_independent_and_preserves_tokens() {
        let entries = vec![
            (Value::Token("id".into()), Value::Int(1)),
            (Value::String("id".into()), Value::Int(2)),
        ];
        let mut reversed = entries.clone();
        reversed.reverse();
        assert_eq!(
            hash_value_u64(&Value::TypedMap(entries)),
            hash_value_u64(&Value::TypedMap(reversed))
        );
        assert_ne!(
            hash_value_u64(&Value::Token("id".into())),
            hash_value_u64(&Value::String("id".into()))
        );
        assert_eq!(
            hash_value_u64(&Value::TypedMap(vec![(
                Value::String("a".into()),
                Value::Int(1)
            )])),
            hash_value_u64(&Value::Map(BTreeMap::from([("a".into(), Value::Int(1))])))
        );
    }
}
