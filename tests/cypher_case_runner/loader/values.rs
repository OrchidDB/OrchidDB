//! Typed fixture values and collection literal decoding.

use new_graph::ir::value::{STRUCT_ORDER_KEY, Value};
use num_bigint::BigInt;
use std::collections::BTreeMap;
use std::str::FromStr;
use super::KUZU_MAP_ENTRIES_KEY;
use super::schema::{balanced_inner, parse_identifier};
use super::temporal::{format_timestamp_for_type, normalize_interval, parse_date_value};

pub(super) fn parse_typed_value(raw: &str, type_text: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = type_text.trim().to_ascii_uppercase();
    if upper.starts_with("STRUCT") {
        return parse_struct_value(trimmed, type_text);
    }
    if upper.starts_with("MAP") {
        return parse_map_value(trimmed, type_text);
    }
    if upper.starts_with("UNION") {
        return parse_union_value(trimmed, type_text);
    }
    if upper.contains('[') {
        return parse_list_value(trimmed, type_text);
    }
    parse_scalar_value(trimmed, type_text)
}

fn parse_list_value(raw: &str, type_text: &str) -> Option<Value> {
    let (base, depth) = list_type_parts(type_text)?;
    let inner = raw.trim().strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Value::List(Vec::new()));
    }
    let items = split_value_items(inner)
        .into_iter()
        .map(|item| {
            if depth > 1 {
                let nested_type = format!("{base}{}", "[]".repeat(depth - 1));
                parse_list_value(item.trim(), &nested_type)
            } else {
                parse_list_scalar_value(item.trim(), base)
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Value::List(items))
}

fn parse_list_scalar_value(raw: &str, type_text: &str) -> Option<Value> {
    if is_string_type(type_text) {
        return Some(Value::String(unescape_list_string(raw.trim())));
    }
    parse_scalar_value(raw, type_text)
}

fn unescape_list_string(raw: &str) -> String {
    let mut out = raw.to_string();
    while out.contains("\\\"") {
        out = out.replace("\\\"", "\"");
    }
    while out.contains("\\'") {
        out = out.replace("\\'", "'");
    }
    out
}

fn list_type_parts(type_text: &str) -> Option<(&str, usize)> {
    let base_end = type_text.find('[')?;
    let depth = type_text[base_end..].matches('[').count();
    Some((type_text[..base_end].trim(), depth))
}

fn parse_struct_value(raw: &str, type_text: &str) -> Option<Value> {
    let fields = struct_fields(type_text)?;
    let entries = raw_map_entries(raw)?;
    let mut out = BTreeMap::new();
    out.insert(
        STRUCT_ORDER_KEY.to_string(),
        Value::List(
            fields
                .iter()
                .map(|(name, _)| Value::String(name.clone()))
                .collect(),
        ),
    );
    for (name, field_type) in fields {
        let value = entries
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(&name))
            .and_then(|(_, value)| parse_typed_value(value, &field_type))
            .unwrap_or(Value::Null);
        out.insert(name, value);
    }
    Some(Value::Map(out))
}

fn parse_map_value(raw: &str, type_text: &str) -> Option<Value> {
    let (key_type, value_type) = map_type_parts(type_text)?;
    let entries = raw_map_entries(raw)?;
    let mut out = Vec::new();
    for (key, raw_value) in entries {
        let key = parse_list_scalar_value(&key, &key_type)?;
        let value = parse_typed_value(&raw_value, &value_type).unwrap_or(Value::Null);
        out.push(Value::List(vec![key, value]));
    }
    Some(Value::Map(BTreeMap::from([(
        KUZU_MAP_ENTRIES_KEY.to_string(),
        Value::List(out),
    )])))
}

fn parse_union_value(raw: &str, type_text: &str) -> Option<Value> {
    let variants = struct_fields(type_text)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    for (tag, ty) in &variants {
        if let Some(value) = parse_typed_value(trimmed, ty) {
            let mut out = BTreeMap::new();
            out.insert("__tag".to_string(), Value::String(tag.clone()));
            out.insert("__value".to_string(), value);
            return Some(Value::Map(out));
        }
    }
    Some(Value::String(unquote(trimmed).to_string()))
}

fn map_type_parts(type_text: &str) -> Option<(String, String)> {
    let upper = type_text.trim().to_ascii_uppercase();
    let open = upper.find("MAP")?;
    let after = &type_text[open + 3..].trim_start();
    let inner = after.strip_prefix('(').and_then(balanced_inner)?;
    let mut parts = split_value_items(&inner).into_iter();
    let key = parts.next()?.trim().to_string();
    let value = parts.next()?.trim().to_string();
    Some((key, value))
}

fn struct_fields(type_text: &str) -> Option<Vec<(String, String)>> {
    let open = type_text.find('(')?;
    let inner = balanced_inner(&type_text[open + 1..])?;
    split_value_items(&inner)
        .into_iter()
        .map(|part| {
            let (name, after) = parse_identifier(part.trim())?;
            let ty = after.trim();
            (!ty.is_empty()).then(|| (name, ty.to_string()))
        })
        .collect()
}

pub(super) fn raw_map_entries(raw: &str) -> Option<Vec<(String, String)>> {
    let inner = raw.trim().strip_prefix('{')?.strip_suffix('}')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    split_value_items(inner)
        .into_iter()
        .map(|entry| {
            let (key, value) = split_top_level_assignment(&entry)?;
            Some((unquote(key.trim()).to_string(), value.trim().to_string()))
        })
        .collect()
}

fn parse_scalar_value(raw: &str, type_text: &str) -> Option<Value> {
    let upper = type_text.trim().to_ascii_uppercase();
    let head = upper
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("");
    match head {
        "INT128" | "UINT64" | "UINT128" => BigInt::from_str(raw.trim()).ok().map(Value::BigInt),
        "INT" | "INT8" | "INT16" | "INT32" | "INT64" | "BIGINT" | "SERIAL" | "UINT8" | "UINT16"
        | "UINT32" | "BYTE" | "SHORT" | "LONG" => parse_i64(raw).map(Value::Int),
        "FLOAT" | "FLOAT32" | "FLOAT64" | "DOUBLE" | "DECIMAL" => {
            raw.trim().parse::<f64>().ok().map(Value::Float)
        }
        "BOOL" | "BOOLEAN" => parse_bool(raw).map(Value::Bool),
        "DATE" => parse_date_value(raw).map(Value::DateTime),
        "TIMESTAMP" | "DATETIME" | "TIMESTAMP_NS" | "TIMESTAMP_MS" | "TIMESTAMP_SEC"
        | "TIMESTAMP_TZ" => format_timestamp_for_type(raw, head).map(Value::DateTime),
        "UUID" => Some(Value::String(normalize_uuid(raw))),
        "INTERVAL" => Some(Value::String(normalize_interval(raw))),
        _ => Some(Value::String(unquote(raw.trim()).to_string())),
    }
}

fn is_string_type(type_text: &str) -> bool {
    matches!(
        type_text
            .trim()
            .to_ascii_uppercase()
            .split(|c: char| c.is_whitespace() || c == '(')
            .next()
            .unwrap_or(""),
        "STRING" | "VARCHAR" | "CHAR" | "TEXT" | "BLOB" | "BYTEA"
    )
}

pub(super) fn split_value_items(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            ',' if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                out.push(body[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = body[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

fn split_top_level_assignment(input: &str) -> Option<(&str, &str)> {
    split_top_level_char(input, ':').or_else(|| split_top_level_char(input, '='))
}

fn split_top_level_char(input: &str, needle: char) -> Option<(&str, &str)> {
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    for (idx, ch) in input.char_indices() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            ch if ch == needle && depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                return Some((&input[..idx], &input[idx + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

pub(super) fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| text.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(text)
}

pub(super) fn normalize_uuid(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = trimmed.trim_start_matches('{').trim_end_matches('}').trim();
    let hex: String = inner.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return raw.to_string();
    }
    let lower = hex.to_ascii_lowercase();
    format!(
        "{}-{}-{}-{}-{}",
        &lower[..8],
        &lower[8..12],
        &lower[12..16],
        &lower[16..20],
        &lower[20..32]
    )
}

/// Normalize a CSV timestamp string to Kuzu's printer form:
/// `YYYY-MM-DD HH:MM:SS[.fff]`, with timezone offsets folded into UTC.
/// Anything we can't parse passes through untouched so we don't lose
/// information on weird inputs.
pub(super) fn parse_i64(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<i64>().ok()
}

pub(super) fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "1" => Some(true),
        "false" | "f" | "0" => Some(false),
        _ => None,
    }
}
