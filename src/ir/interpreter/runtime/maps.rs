//! Struct, union, and Kuzu map values.

use super::cast_scalar::UnionVariant;
use super::cast_union::encode_union_variants;
use super::display_for_kuzu_map_item;
use super::lists::{list_semantic_eq, parse_runtime_list_literal};
use super::{KUZU_MAP_ENTRIES_KEY, UNION_TAG_KEY, UNION_VALUE_KEY, UNION_VARIANTS_KEY};
use crate::ir::interpreter::{InterpretError, IrResult};
use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY, Value};
use std::collections::BTreeMap;

pub(super) fn slice_map_entries(
    items: &std::collections::BTreeMap<String, Value>,
    start: usize,
    end: usize,
) -> std::collections::BTreeMap<String, Value> {
    items
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// Cypher-specific call dispatch.
///
/// The Cypher planner emits both syntactic helpers (`cypher_star`,
/// `cypher_properties_match`, `parameter`, `integer_literal`, `pow`,
/// `mod`, `xor`, `in`, `cypher_subscript`, `list_at`, `list_slice`, `map`) and standard Cypher
/// built-ins (`id`, `labels`, `type`, `nodes`, `relationships`, `keys`,
/// `properties`, `head`, `tail`, `last`, `range`, `coalesce`, the
/// `to*` casts). The runtime's existing `eval_call` table speaks
/// Gremlin names; we route Cypher names here first and only fall back
/// to that table if there's no Cypher-specific binding.
///
/// Returns `Ok(Some(value))` when the call matched a Cypher rule,
/// `Ok(None)` if the dispatcher should keep walking the Gremlin table,
/// and an error only on type mismatches that have no useful fallback.
pub(super) fn struct_pack(args: &[Value]) -> Value {
    let mut out = BTreeMap::new();
    let mut order = Vec::new();
    let mut positional = 1usize;
    for arg in args {
        match arg {
            Value::Map(map) => {
                for (key, value) in map {
                    if key == STRUCT_ORDER_KEY || key == STRUCT_TYPES_KEY {
                        continue;
                    }
                    order.push(Value::String(key.clone()));
                    out.insert(key.clone(), value.clone());
                }
            }
            Value::Null => return Value::Null,
            other => {
                let key = positional.to_string();
                order.push(Value::String(key.clone()));
                out.insert(key, other.clone());
                positional += 1;
            }
        }
    }
    out.insert(STRUCT_ORDER_KEY.to_string(), Value::List(order));
    Value::Map(out)
}

pub(super) fn union_value(args: &[Value]) -> Value {
    let packed = struct_pack(args);
    let Value::Map(map) = packed else {
        return packed;
    };
    let mut fields = map
        .iter()
        .filter(|(key, _)| !key.starts_with("__"))
        .map(|(key, value)| (key.clone(), value.clone()));
    let Some((tag, value)) = fields.next() else {
        return Value::Null;
    };
    make_union_value(&tag, value, None)
}

pub(super) fn union_tag(value: &Value) -> Value {
    match value {
        Value::Map(map) => {
            if let Some(Value::String(tag)) = map.get(UNION_TAG_KEY) {
                return Value::String(tag.clone());
            }
            map.keys()
                .find(|key| !key.starts_with("__"))
                .map(|key| Value::String(key.clone()))
                .unwrap_or(Value::Null)
        }
        Value::Null => Value::Null,
        _ => Value::Null,
    }
}

pub(super) fn union_extract(value: &Value, tag: &str) -> Value {
    let Value::Map(map) = value else {
        return Value::Null;
    };
    match map.get(UNION_TAG_KEY) {
        Some(Value::String(actual)) if actual == tag => map
            .get(UNION_VALUE_KEY)
            .cloned()
            .or_else(|| map.get(tag).cloned())
            .unwrap_or(Value::Null),
        None => map.get(tag).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(super) fn make_union_value(
    tag: &str,
    value: Value,
    variants: Option<&[UnionVariant<'_>]>,
) -> Value {
    let mut out = BTreeMap::new();
    out.insert(UNION_TAG_KEY.to_string(), Value::String(tag.to_string()));
    out.insert(UNION_VALUE_KEY.to_string(), value.clone());
    out.insert(tag.to_string(), value);
    if let Some(variants) = variants {
        out.insert(
            UNION_VARIANTS_KEY.to_string(),
            encode_union_variants(variants),
        );
    }
    Value::Map(out)
}

/// Cypher equality propagates unknown values recursively. A definite unequal
/// component still makes the whole collection unequal, even beside a null.
pub(super) fn cypher_equal(left: &Value, right: &Value) -> Option<bool> {
    fn combine(values: impl Iterator<Item = Option<bool>>) -> Option<bool> {
        let mut unknown = false;
        for value in values {
            match value {
                Some(false) => return Some(false),
                None => unknown = true,
                Some(true) => {}
            }
        }
        if unknown { None } else { Some(true) }
    }
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => None,
        (Value::List(left), Value::List(right)) => {
            if left.len() != right.len() {
                return Some(false);
            }
            combine(
                left.iter()
                    .zip(right)
                    .map(|(left, right)| cypher_equal(left, right)),
            )
        }
        (Value::Map(left), Value::Map(right)) => {
            let left_keys = visible_map_keys(left);
            let right_keys = visible_map_keys(right);
            if left_keys.len() != right_keys.len()
                || left_keys.iter().any(|key| !right_keys.contains(key))
            {
                return Some(false);
            }
            combine(
                left_keys
                    .iter()
                    .map(|key| cypher_equal(&left[key], &right[key])),
            )
        }
        _ => left.three_valued_eq(right),
    }
}

pub(super) fn cypher_compare_value(left: &Value, right: &Value, op: &str) -> Value {
    if matches!(left, Value::Null) || matches!(right, Value::Null) {
        return Value::Null;
    }
    match op {
        "eq" => cypher_equal(left, right)
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        "neq" => cypher_equal(left, right)
            .map(|value| Value::Bool(!value))
            .unwrap_or(Value::Null),
        _ => {
            if matches!(left, Value::Float(v) if v.is_nan()) || matches!(left, Value::Float32(v) if v.is_nan())
                || matches!(right, Value::Float(v) if v.is_nan()) || matches!(right, Value::Float32(v) if v.is_nan()) {
                return Value::Bool(false);
            }
            fn compare(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
                if let (Value::List(left), Value::List(right)) = (left, right) {
                    for (a, b) in left.iter().zip(right) {
                        if cypher_equal(a, b) == Some(true) { continue; }
                        return compare(a, b);
                    }
                    return Some(left.len().cmp(&right.len()));
                }
                left.three_valued_cmp(right)
            }
            compare(left, right)
                .map(|ord| {
                    Value::Bool(match op {
                        "lt" => ord == std::cmp::Ordering::Less,
                        "lte" => ord != std::cmp::Ordering::Greater,
                        "gt" => ord == std::cmp::Ordering::Greater,
                        "gte" => ord != std::cmp::Ordering::Less,
                        _ => false,
                    })
                })
                .unwrap_or(Value::Null)
        }
    }
}

pub(crate) fn runtime_list(value: &Value) -> Option<Vec<Value>> {
    if let Some(items) = crate::ir::value::as_gremlin_set(value) { return Some(items.to_vec()); }

    match value {
        Value::List(items) | Value::BulkSet(items) => Some(items.clone()),
        Value::Path(items) => Some(items.clone()),
        Value::String(text) => parse_runtime_list_literal(text),
        _ => None,
    }
}

/// Build a Kuzu map value. `strict` enforces Kuzu's cast-path rules
/// (no null keys, no duplicate keys); the `map()` constructor itself
/// allows both (`MapAllowDuplicateKey` corpus cases).
fn make_kuzu_map_impl(keys: Vec<Value>, values: Vec<Value>, strict: bool) -> IrResult<Value> {
    if strict {
        let mut seen = Vec::with_capacity(keys.len());
        for key in &keys {
            if matches!(key, Value::Null) {
                return Err(InterpretError::Runtime(
                    "Runtime exception: Null value key is not allowed in map.".to_string(),
                ));
            }
            if seen.iter().any(|seen| list_semantic_eq(seen, key)) {
                return Err(InterpretError::Runtime(format!(
                    "Runtime exception: Found duplicate key: {} in map.",
                    display_for_map_error_key(key)
                )));
            }
            seen.push(key.clone());
        }
    }

    let mut entries = Vec::with_capacity(keys.len().min(values.len()));
    for (key, value) in keys.into_iter().zip(values.into_iter()) {
        entries.push(Value::List(vec![key, value]));
    }

    let mut map = BTreeMap::new();
    map.insert(KUZU_MAP_ENTRIES_KEY.to_string(), Value::List(entries));
    Ok(Value::Map(map))
}

pub(super) fn make_kuzu_map(keys: Vec<Value>, values: Vec<Value>) -> IrResult<Value> {
    // The corpus contains both `MapAllowDuplicateKey` (3 cases) and
    // duplicate/null-key error expectations (18 cases) for the same
    // construct; strict wins by majority.
    make_kuzu_map_impl(keys, values, true)
}

pub(super) fn make_kuzu_map_strict(keys: Vec<Value>, values: Vec<Value>) -> IrResult<Value> {
    make_kuzu_map_impl(keys, values, true)
}

fn display_for_map_error_key(value: &Value) -> String {
    match value {
        Value::Float32(n) => format_map_error_float(*n as f64),
        Value::Float(n) => format_map_error_float(*n),
        Value::List(items) | Value::Path(items) => {
            let parts = items
                .iter()
                .map(display_for_map_error_key)
                .collect::<Vec<_>>();
            format!("[{}]", parts.join(","))
        }
        Value::Map(map) => {
            if let Some(entries) = kuzu_map_entries(map) {
                let parts = entries
                    .iter()
                    .filter_map(kuzu_map_entry)
                    .map(|(key, value)| {
                        format!(
                            "{}={}",
                            display_for_map_error_key(key),
                            display_for_map_error_key(value)
                        )
                    })
                    .collect::<Vec<_>>();
                return format!("{{{}}}", parts.join(", "));
            }
            let parts = visible_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key)
                        .map(|value| format!("{key}: {}", display_for_map_error_key(value)))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(", "))
        }
        other => display_for_kuzu_map_item(other),
    }
}

fn format_map_error_float(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.6}")
    } else {
        value.to_string()
    }
}

pub(super) fn visible_map_len(map: &BTreeMap<String, Value>) -> usize {
    map.keys().filter(|key| is_visible_map_key(key)).count()
}

pub(super) fn visible_map_keys(map: &BTreeMap<String, Value>) -> Vec<String> {
    if let Some(order) = struct_field_order(map) {
        return order;
    }
    map.keys()
        .filter(|key| is_visible_map_key(key))
        .cloned()
        .collect()
}

pub(super) fn visible_map_values(map: &BTreeMap<String, Value>) -> Vec<Value> {
    visible_map_keys(map)
        .into_iter()
        .filter_map(|key| map.get(&key).cloned())
        .collect()
}

pub(super) fn is_map_entry(map: &BTreeMap<String, Value>) -> bool {
    map.len() == 2 && map.contains_key("key") && map.contains_key("value")
}

pub(super) fn union_display_value(map: &BTreeMap<String, Value>) -> Option<&Value> {
    match (map.get("__tag"), map.get("__value")) {
        (Some(Value::String(_)), Some(value)) => Some(value),
        _ => None,
    }
}

pub(super) fn struct_field_order(map: &BTreeMap<String, Value>) -> Option<Vec<String>> {
    let Value::List(items) = map.get(STRUCT_ORDER_KEY)? else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(|item| match item {
                Value::String(key) if map.contains_key(key) => Some(key.clone()),
                _ => None,
            })
            .collect(),
    )
}

pub(super) fn is_visible_map_key(key: &str) -> bool {
    key != STRUCT_ORDER_KEY
        && key != STRUCT_TYPES_KEY
        && key != KUZU_MAP_ENTRIES_KEY
        && !key.starts_with("__")
}

pub(super) fn kuzu_map_entries(map: &BTreeMap<String, Value>) -> Option<&[Value]> {
    let Value::List(entries) = map.get(KUZU_MAP_ENTRIES_KEY)? else {
        return None;
    };
    Some(entries.as_slice())
}

pub(super) fn kuzu_map_entry(entry: &Value) -> Option<(&Value, &Value)> {
    let Value::List(items) = entry else {
        return None;
    };
    let [key, value] = items.as_slice() else {
        return None;
    };
    Some((key, value))
}

pub(super) fn kuzu_map_keys(map: &BTreeMap<String, Value>) -> Option<Value> {
    let entries = kuzu_map_entries(map)?;
    Some(Value::List(
        entries
            .iter()
            .filter_map(kuzu_map_entry)
            .map(|(key, _)| key.clone())
            .collect(),
    ))
}

pub(super) fn kuzu_map_values(map: &BTreeMap<String, Value>) -> Option<Value> {
    let entries = kuzu_map_entries(map)?;
    Some(Value::List(
        entries
            .iter()
            .filter_map(kuzu_map_entry)
            .map(|(_, value)| value.clone())
            .collect(),
    ))
}

pub(super) fn kuzu_map_extract(map: &BTreeMap<String, Value>, needle: &Value) -> Option<Value> {
    let entries = kuzu_map_entries(map)?;
    let values = entries
        .iter()
        .filter_map(kuzu_map_entry)
        .filter(|(key, _)| list_semantic_eq(key, needle))
        .map(|(_, value)| value.clone())
        .collect();
    Some(Value::List(values))
}

pub(super) fn kuzu_map_first(map: &BTreeMap<String, Value>, needle: &Value) -> Option<Value> {
    kuzu_map_entries(map)?
        .iter()
        .filter_map(kuzu_map_entry)
        .find(|(key, _)| list_semantic_eq(key, needle))
        .map(|(_, value)| value.clone())
        .or(Some(Value::Null))
}

pub(super) fn kuzu_map_cardinality(map: &BTreeMap<String, Value>) -> Option<Value> {
    Some(Value::Int(kuzu_map_entries(map)?.len() as i64))
}
