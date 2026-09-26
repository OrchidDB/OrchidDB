//! Java HashMap's iteration order for value keys with a defined Java hash.
//!
//! TinkerPop evaluates grouped child traversals in bucket order. This matters
//! when those children share a seeded RNG. Stable bucket sorting preserves
//! insertion order within collision chains, including low/high resize splits.

use crate::ir::value::Value;

/// Order existing distinct entries as a default Java HashMap (capacity 16,
/// load factor .75). Unknown object hash codes retain the original order.
/// Treeified collision bins are deliberately not modeled: their order may
/// depend on JVM object identity and is not a portable seeded guarantee.
pub(crate) fn java_hashmap_order<T>(entries: &mut [(Value, T)]) {
    let Some(hashes) = entries
        .iter()
        .map(|(key, _)| java_hash(key).map(|hash| hash ^ (hash >> 16)))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let mut capacity = 16usize;
    let mut bins = std::collections::BTreeMap::<usize, usize>::new();
    for (index, hash) in hashes.iter().enumerate() {
        let count = bins.entry(*hash as usize & (capacity - 1)).or_default();
        *count += 1;
        // HashMap grows a crowded bin before considering treeification.
        let crowded = *count >= 9 && capacity < 64;
        if capacity < (1 << 30) && (crowded || index + 1 > capacity - capacity / 4) {
            capacity *= 2;
            bins.clear();
            for hash in &hashes[..=index] {
                *bins.entry(*hash as usize & (capacity - 1)).or_default() += 1;
            }
        }
    }
    entries.sort_by_key(|(key, _)| {
        let hash = java_hash(key).expect("hashability checked");
        (hash ^ (hash >> 16)) as usize & (capacity - 1)
    });
}

fn java_hash(value: &Value) -> Option<u32> {
    let long_hash = |v: u64| (v ^ (v >> 32)) as u32;
    Some(match value {
        Value::Null => 0,
        Value::Bool(v) => {
            if *v {
                1231
            } else {
                1237
            }
        }
        Value::Byte(v) => *v as u32,
        Value::Short(v) => *v as u32,
        Value::Int(v) => *v as u32,
        Value::Long(v) => long_hash(*v as u64),
        Value::Float32(v) => {
            if v.is_nan() {
                0x7fc00000
            } else {
                v.to_bits()
            }
        }
        Value::Float(v) => long_hash(if v.is_nan() {
            0x7ff8000000000000
        } else {
            v.to_bits()
        }),
        Value::String(v) => v
            .encode_utf16()
            .fold(0u32, |hash, c| hash.wrapping_mul(31).wrapping_add(c as u32)),
        Value::BigInt(v) => bigint_hash(v),
        Value::BigDecimal(v) => {
            let (unscaled, scale) = v.as_bigint_and_exponent();
            bigint_hash(&unscaled)
                .wrapping_mul(31)
                .wrapping_add(scale as u32)
        }
        Value::List(items) => {
            let mut hash = 1u32;
            for item in items {
                hash = hash.wrapping_mul(31).wrapping_add(java_hash(item)?);
            }
            hash
        }
        Value::TypedMap(entries) => {
            let mut hash = 0u32;
            for (key, value) in entries {
                hash = hash.wrapping_add(java_hash(key)? ^ java_hash(value)?);
            }
            hash
        }
        Value::Map(entries) => {
            let mut hash = 0u32;
            for (key, value) in entries {
                hash =
                    hash.wrapping_add(java_hash(&Value::String(key.clone()))? ^ java_hash(value)?);
            }
            hash
        }
        _ => return None,
    })
}

fn bigint_hash(value: &num_bigint::BigInt) -> u32 {
    let (sign, words) = value.to_u32_digits();
    let hash = words
        .iter()
        .rev()
        .fold(0u32, |hash, word| hash.wrapping_mul(31).wrapping_add(*word));
    if sign == num_bigint::Sign::Minus {
        hash.wrapping_neg()
    } else {
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_hash_uses_java_utf16_and_collision_insertion_order() {
        assert_eq!(java_hash(&Value::String("😀".into())), Some(1772899));
        let mut entries = ["person", "software"].map(|s| (Value::String(s.into()), ()));
        java_hashmap_order(&mut entries);
        assert_eq!(entries[0].0, Value::String("software".into()));
        // These strings have identical Java String.hashCode values.
        let mut collisions = ["BB", "Aa"].map(|s| (Value::String(s.into()), ()));
        java_hashmap_order(&mut collisions);
        assert_eq!(collisions[0].0, Value::String("BB".into()));
    }

    #[test]
    fn table_resizes_at_default_load_factor() {
        let mut entries = (0..13)
            .rev()
            .map(|i| (Value::Int(i * 16), ()))
            .collect::<Vec<_>>();
        java_hashmap_order(&mut entries);
        let keys: Vec<_> = entries
            .iter()
            .map(|(key, _)| key.as_i64().unwrap())
            .collect();
        // At capacity 32 the even multiples occupy bucket 0, odd bucket 16.
        assert_eq!(
            keys,
            vec![192, 160, 128, 96, 64, 32, 0, 176, 144, 112, 80, 48, 16]
        );
    }
}
