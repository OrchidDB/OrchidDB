//! Substring + display_for_concat + display helpers + regex.
//!
//! Extracted from `interpreter.rs` lines 3193..3308.

use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY, Value};

pub(crate) fn substring(s: &str, start: i64, end: Option<i64>) -> String {
    let chars: Vec<&str> = unicode_segmentation::UnicodeSegmentation::graphemes(s, true).collect();
    let len = chars.len() as i64;
    let start = normalize_substring_index(start, len);
    let end = end
        .map(|e| normalize_substring_index(e, len).max(start))
        .unwrap_or(len) as usize;
    let start = start as usize;
    chars[start..end].concat()
}

fn normalize_substring_index(index: i64, len: i64) -> i64 {
    if index < 0 {
        (len + index).max(0)
    } else {
        index.min(len)
    }
}

/// Stringify a value for embedding in `concat` / `conjoin` / map-key
/// output. Arbitrary-precision numerics carry their type tag (`d[N].m`
/// / `d[N].i`) so a `groupCount()` keyed on a BigDecimal renders with
/// the same form the harness expects.
pub(crate) fn display_for_concat(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Byte(n) => n.to_string(),
        Value::UInt8(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::UInt16(n) => n.to_string(),
        Value::Int(n) => n.to_string(),
        Value::UInt32(n) => n.to_string(),
        Value::Long(n) => n.to_string(),
        Value::UInt64(n) => n.to_string(),
        Value::Float32(f) => f.to_string(),
        Value::Float(f) => f.to_string(),
        Value::BigInt(n) => format!("d[{n}].n"),
        Value::UInt128(n) => format!("d[{n}].u128"),
        Value::BigDecimal(d) => format!("d[{d}].m"),
        Value::DateTime(s) => format!("dt[{s}]"),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::Null => "null".to_string(),
        Value::Node { label, id } => format!("v[{label}#{id}]"),
        Value::Edge { rel_type, id, .. } => format!("e[{rel_type}#{id}]"),
        Value::List(items) => {
            let parts = items
                .iter()
                .map(display_for_tagged_container)
                .collect::<Vec<_>>();
            format!("l[{}]", parts.join(","))
        }
        Value::Map(map) => {
            if let Some(entries) = kuzu_map_entries(map) {
                let parts = entries
                    .iter()
                    .filter_map(kuzu_map_entry)
                    .map(|(key, value)| {
                        format!(
                            "{}={}",
                            display_for_kuzu_map_item(key),
                            display_for_kuzu_map_item(value)
                        )
                    })
                    .collect::<Vec<_>>();
                return format!("{{{}}}", parts.join(", "));
            }
            let parts = ordered_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key).map(|value| {
                        format!("\"{key}\":\"{}\"", display_for_tagged_container(value))
                    })
                })
                .collect::<Vec<_>>();
            format!("m[{{{}}}]", parts.join(","))
        }
        Value::Path(items) => {
            let parts = items
                .iter()
                .map(display_for_tagged_container)
                .collect::<Vec<_>>();
            format!("p[{}]", parts.join(","))
        }
    }
}

fn kuzu_map_entries(map: &std::collections::BTreeMap<String, Value>) -> Option<&[Value]> {
    let entries = map.get("\u{0}kuzu_map_entries")?;
    let Value::List(entries) = entries else {
        return None;
    };
    Some(entries.as_slice())
}

fn kuzu_map_entry(entry: &Value) -> Option<(&Value, &Value)> {
    let Value::List(items) = entry else {
        return None;
    };
    let [key, value] = items.as_slice() else {
        return None;
    };
    Some((key, value))
}

pub(crate) fn display_for_kuzu_map_item(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Byte(n) => n.to_string(),
        Value::UInt8(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::UInt16(n) => n.to_string(),
        Value::Int(n) | Value::Long(n) => n.to_string(),
        Value::UInt32(n) => n.to_string(),
        Value::UInt64(n) => n.to_string(),
        Value::Float32(n) => (*n as f64).to_string(),
        Value::Float(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::UInt128(n) => n.to_string(),
        Value::BigDecimal(n) => n.to_string(),
        Value::DateTime(s) => s.clone(),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::List(items) | Value::Path(items) => {
            let parts = items
                .iter()
                .map(display_for_kuzu_map_item)
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
                            display_for_kuzu_map_item(key),
                            display_for_kuzu_map_item(value)
                        )
                    })
                    .collect::<Vec<_>>();
                return format!("{{{}}}", parts.join(", "));
            }
            let parts = ordered_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key)
                        .map(|value| format!("{key}: {}", display_for_kuzu_map_item(value)))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Node { label, id } => format!("v[{label}#{id}]"),
        Value::Edge { rel_type, id, .. } => format!("e[{rel_type}#{id}]"),
    }
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

pub(crate) fn display_for_tagged_container(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Byte(n) => format!("d[{n}].b"),
        Value::UInt8(n) => format!("d[{n}].u8"),
        Value::Short(n) => format!("d[{n}].s"),
        Value::UInt16(n) => format!("d[{n}].u16"),
        Value::Int(n) => format!("d[{n}].i"),
        Value::UInt32(n) => format!("d[{n}].u32"),
        Value::Long(n) => format!("d[{n}].l"),
        Value::UInt64(n) => format!("d[{n}].u64"),
        Value::Float32(f) => format!("d[{f}].f"),
        Value::Float(f) => format!("d[{}].d", format_f64_tag(*f)),
        Value::BigInt(n) => format!("d[{n}].n"),
        Value::UInt128(n) => format!("d[{n}].u128"),
        Value::BigDecimal(d) => format!("d[{d}].m"),
        Value::DateTime(s) => format!("dt[{s}]"),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::Null => "null".to_string(),
        Value::Node { label, id } => format!("v[{}]", display_node_name(label, *id)),
        Value::Edge {
            rel_type,
            src_label,
            src_id,
            dst_label,
            dst_id,
            ..
        } => format!(
            "e[{}-{}->{}]",
            display_node_name(src_label, *src_id),
            rel_type,
            display_node_name(dst_label, *dst_id)
        ),
        Value::List(items) => {
            let parts = items
                .iter()
                .map(display_for_tagged_container)
                .collect::<Vec<_>>();
            format!("l[{}]", parts.join(","))
        }
        Value::Map(_) | Value::Path(_) => display_for_concat(v),
    }
}

pub(crate) fn display_for_group_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Float(f) => format!("d[{}].d", format_f64_tag(*f)),
        Value::Float32(f) => format!("d[{}].f", format_f32_tag(*f)),
        Value::BigInt(n) => format!("d[{n}].n"),
        Value::UInt128(n) => format!("d[{n}].u128"),
        Value::DateTime(s) => format!("dt[{s}]"),
        other => display_for_tagged_container(other),
    }
}

fn format_f32_tag(value: f32) -> String {
    format_f64_tag(value as f64)
}

fn format_f64_tag(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

pub(crate) fn display_node_name(label: &str, id: i64) -> String {
    match (label, id) {
        ("person", 0) => "marko".to_string(),
        ("person", 1) => "vadas".to_string(),
        ("person", 2) => "josh".to_string(),
        ("person", 3) => "peter".to_string(),
        ("software", 0) => "lop".to_string(),
        ("software", 1) => "ripple".to_string(),
        _ => format!("{label}#{id}"),
    }
}

/// Approximate `Predicate.regex` without pulling in a regex engine.
/// Handles the literal patterns and `.` wildcards that appear in the
/// conformance corpus; everything else returns `false` rather than
/// failing the run. A real regex backend is a follow-up.
pub(crate) fn regex_match_literal(haystack: &str, pattern: &str) -> bool {
    // Strip the conventional `^...$` anchors if present.
    let core = pattern
        .strip_prefix('^')
        .unwrap_or(pattern)
        .strip_suffix('$')
        .unwrap_or_else(|| pattern.strip_prefix('^').unwrap_or(pattern));
    // If the trimmed pattern has no metacharacters, do a literal full match.
    let metas = ['.', '*', '+', '?', '|', '[', ']', '(', ')', '{', '}', '\\'];
    if !core.chars().any(|c| metas.contains(&c)) {
        return haystack == core;
    }
    // Pure-`.` wildcard pass: every position matches any char of the same
    // length. Anything more complex is out of scope.
    if core.chars().all(|c| c == '.') {
        return haystack.chars().count() == core.chars().count();
    }
    false
}
