//! typeof_matches predicate.
//!
//! Extracted from `interpreter.rs` lines 3150..3192.

use crate::ir::value::Value;

pub(crate) fn typeof_matches(value: &Value, name: &str) -> bool {
    let normalised = name
        .trim()
        .trim_start_matches("GType.")
        .trim_start_matches("java.lang.")
        .trim_start_matches("java.math.")
        .to_ascii_lowercase();
    match value {
        Value::Null => normalised == "null",
        Value::Bool(_) => matches!(normalised.as_str(), "boolean" | "bool"),
        Value::Byte(_) => normalised == "byte",
        Value::UInt8(_) => matches!(normalised.as_str(), "uint8" | "byte"),
        Value::Short(_) => normalised == "short",
        Value::UInt16(_) => normalised == "uint16",
        Value::Int(_) => matches!(normalised.as_str(), "int" | "integer"),
        Value::UInt32(_) => normalised == "uint32",
        Value::Long(_) => normalised == "long",
        Value::UInt64(_) => normalised == "uint64",
        Value::Float32(_) => normalised == "float",
        Value::Float(_) => normalised == "double",
        // BigInt / BigDecimal carry their typed identity from
        // `asNumber(GType.BIGINT/BIGDECIMAL)`; preserve it on
        // `typeOf` checks. We also accept the unrefined numeric tags
        // so `typeOf(GType.LONG)` still passes for a BigInt that
        // would semantically fit in a 64-bit integer.
        Value::BigInt(_) => matches!(normalised.as_str(), "bigint" | "biginteger"),
        Value::UInt128(_) => normalised == "uint128",
        Value::BigDecimal(_) => matches!(normalised.as_str(), "bigdecimal" | "decimal"),
        Value::DateTime(_) => matches!(normalised.as_str(), "datetime" | "date"),
        Value::InternalId { .. } => matches!(normalised.as_str(), "internal_id" | "internalid"),
        Value::String(value) => match normalised.as_str() {
            "uuid" => is_uuid_tagged(value),
            "string" | "char" | "character" => !is_uuid_tagged(value),
            _ => false,
        },
        Value::List(_) => matches!(normalised.as_str(), "list" | "set" | "graph"),
        // `tree()` materializes its result as a nested Map, and a
        // `subgraph()` cap surfaces as a Map of edges; accept those
        // type tags as Map-shaped values. `traverser` and `bulkset`
        // are also commonly typed as Map at this layer.
        Value::Map(_) if crate::ir::value::as_gremlin_set(value).is_some() => {
            matches!(normalised.as_str(), "set" | "bulkset" | "collection")
        }
        Value::Map(_) => matches!(
            normalised.as_str(),
            "map" | "tree" | "graph" | "bulkset" | "traverser"
        ),
        Value::Path(_) => normalised == "path",
        Value::Node { .. } => matches!(normalised.as_str(), "vertex" | "node"),
        Value::Edge { .. } => matches!(normalised.as_str(), "edge" | "relationship"),
    }
}

fn is_uuid_tagged(value: &str) -> bool {
    let Some(inner) = value
        .strip_prefix("uuid[")
        .and_then(|s| s.strip_suffix(']'))
    else {
        return false;
    };
    let parts = inner.split('-').collect::<Vec<_>>();
    matches!(
        parts.as_slice(),
        [a, b, c, d, e]
            if a.len() == 8
                && b.len() == 4
                && c.len() == 4
                && d.len() == 4
                && e.len() == 12
                && parts
                    .iter()
                    .all(|part| part.chars().all(|ch| ch.is_ascii_hexdigit()))
    )
}
