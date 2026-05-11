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
        Value::Short(_) => normalised == "short",
        Value::Int(_) => matches!(normalised.as_str(), "int" | "integer"),
        Value::Long(_) => normalised == "long",
        Value::Float32(_) => normalised == "float",
        Value::Float(_) => normalised == "double",
        // BigInt / BigDecimal carry their typed identity from
        // `asNumber(GType.BIGINT/BIGDECIMAL)`; preserve it on
        // `typeOf` checks. We also accept the unrefined numeric tags
        // so `typeOf(GType.LONG)` still passes for a BigInt that
        // would semantically fit in a 64-bit integer.
        Value::BigInt(_) => matches!(normalised.as_str(), "bigint" | "biginteger"),
        Value::BigDecimal(_) => matches!(normalised.as_str(), "bigdecimal" | "decimal"),
        Value::DateTime(_) => matches!(normalised.as_str(), "datetime" | "date"),
        Value::String(_) => matches!(
            normalised.as_str(),
            "string" | "char" | "character" | "uuid"
        ),
        Value::List(_) => matches!(normalised.as_str(), "list" | "set" | "graph"),
        // `tree()` materializes its result as a nested Map, and a
        // `subgraph()` cap surfaces as a Map of edges; accept those
        // type tags as Map-shaped values. `traverser` and `bulkset`
        // are also commonly typed as Map at this layer.
        Value::Map(_) => matches!(
            normalised.as_str(),
            "map" | "tree" | "graph" | "bulkset" | "traverser"
        ),
        Value::Path(_) => normalised == "path",
        Value::Node { .. } => matches!(normalised.as_str(), "vertex" | "node"),
        Value::Edge { .. } => matches!(normalised.as_str(), "edge" | "relationship"),
    }
}
