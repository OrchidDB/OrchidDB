//! List comparison, literals, display, and indexing.

use super::datetime::normalize_interval_spec;
use super::graph::graph_element_property;
use super::maps::{
    kuzu_map_entries, kuzu_map_entry, kuzu_map_first, runtime_list, union_display_value,
    visible_map_keys, visible_map_len,
};
use super::numeric::{value_as_bigint, value_as_f64};
use crate::ir::catalog::PropertyGraph;
use crate::ir::interpreter::expr::compare_values;
use crate::ir::interpreter::{InterpretError, IrResult};
use crate::ir::value::Value;

pub(super) fn list_semantic_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::String(left), Value::String(right)) => {
            normalize_interval_spec(left) == normalize_interval_spec(right)
        }
        (Value::DateTime(left), Value::String(right))
        | (Value::String(right), Value::DateTime(left)) => {
            left == right || normalize_interval_spec(left) == normalize_interval_spec(right)
        }
        (Value::List(left), Value::List(right)) | (Value::Path(left), Value::Path(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| list_semantic_eq(left, right))
        }
        (Value::Map(left), Value::Map(right)) => {
            visible_map_len(left) == visible_map_len(right)
                && visible_map_keys(left).into_iter().all(|key| {
                    let Some(value) = left.get(&key) else {
                        return false;
                    };
                    right
                        .get(&key)
                        .is_some_and(|right_value| list_semantic_eq(value, right_value))
                })
        }
        _ => left.three_valued_eq(right) == Some(true),
    }
}

pub(super) fn ensure_list_comparable(left: &Value, right: &Value) -> IrResult<()> {
    if matches!(left, Value::Null) || matches!(right, Value::Null) {
        return Ok(());
    }
    if list_comparable(left, right) {
        return Ok(());
    }
    if !matches!(left, Value::Map(_)) && !matches!(right, Value::Map(_)) {
        return Ok(());
    }
    Err(InterpretError::Runtime(format!(
        "Binder exception: Cannot compare {} and {} in list_contains function.",
        cypher_list_type_name(right),
        cypher_list_type_name(left)
    )))
}

fn list_comparable(left: &Value, right: &Value) -> bool {
    if numeric_type(left) && numeric_type(right) {
        return true;
    }
    matches!(
        (left, right),
        (Value::String(_), Value::String(_))
            | (Value::String(_), Value::DateTime(_))
            | (Value::DateTime(_), Value::String(_))
            | (Value::DateTime(_), Value::DateTime(_))
            | (Value::Bool(_), Value::Bool(_))
            | (Value::InternalId { .. }, Value::InternalId { .. })
            | (Value::List(_), Value::List(_))
            | (Value::Map(_), Value::Map(_))
            | (Value::Node { .. }, Value::Node { .. })
            | (Value::Edge { .. }, Value::Edge { .. })
            | (Value::Path(_), Value::Path(_))
    )
}

fn numeric_type(value: &Value) -> bool {
    matches!(
        value,
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
            | Value::BigDecimal(_)
    )
}

pub(super) fn cypher_list_type_name(value: &Value) -> String {
    match value {
        Value::VertexProperty {..} => "VERTEXPROPERTY".into(),
        Value::Property {..} => "PROPERTY".into(),
        Value::MapEntry(_) => "MAP_ENTRY".into(),
        Value::CardinalityValue {..} => "CARDINALITY_VALUE".into(),
        Value::TypedMap(_) => "MAP".into(),
        Value::BulkSet(_) => "BULKSET".into(),
        Value::Set(_) => "SET".into(),
        Value::Token(_) => "TOKEN".into(),
        Value::Direction(_) => "DIRECTION".into(),
        Value::Null => "NULL".to_string(),
        Value::Bool(_) => "BOOL".to_string(),
        Value::Byte(_) => "INT8".to_string(),
        Value::UInt8(_) => "UINT8".to_string(),
        Value::Short(_) => "INT16".to_string(),
        Value::UInt16(_) => "UINT16".to_string(),
        Value::Int(_) | Value::Long(_) => "INT64".to_string(),
        Value::UInt32(_) => "UINT32".to_string(),
        Value::UInt64(_) => "UINT64".to_string(),
        Value::Float32(_) => "FLOAT".to_string(),
        Value::Float(_) => "DOUBLE".to_string(),
        Value::BigInt(_) => "INT128".to_string(),
        Value::UInt128(_) => "UINT128".to_string(),
        Value::BigDecimal(_) => "DECIMAL".to_string(),
        Value::Temporal(t) => t.kind().to_ascii_uppercase(),
        Value::DateTime(_) => "DATE".to_string(),
        Value::InternalId { .. } => "INTERNAL_ID".to_string(),
        Value::String(_) => "STRING".to_string(),
        Value::Node { .. } => "NODE".to_string(),
        Value::Edge { .. } => "REL".to_string(),
        Value::List(items) => items
            .first()
            .map(|item| format!("{}[]", cypher_list_type_name(item)))
            .unwrap_or_else(|| "ANY[]".to_string()),
        Value::Map(map) => {
            let fields = visible_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key)
                        .map(|value| format!("{key} {}", cypher_list_type_name(value)))
                })
                .collect::<Vec<_>>();
            format!("STRUCT({})", fields.join(", "))
        }
        Value::Path(_) => "PATH".to_string(),
    }
}

pub(super) fn list_distinct_values(items: &[Value], include_null: bool) -> Vec<Value> {
    let mut seen = Vec::new();
    for item in items {
        if !include_null && matches!(item, Value::Null) {
            continue;
        }
        if !seen.iter().any(|seen| list_semantic_eq(seen, item)) {
            seen.push(item.clone());
        }
    }
    seen
}

pub(super) fn first_non_null_list_value(items: &[Value]) -> Value {
    items
        .iter()
        .find(|item| !matches!(item, Value::Null))
        .cloned()
        .unwrap_or(Value::Null)
}

pub(super) fn sort_list_values(items: &[Value], descending: bool, nulls_last: bool) -> Vec<Value> {
    let null_count = items
        .iter()
        .filter(|item| matches!(item, Value::Null))
        .count();
    let mut sorted: Vec<Value> = items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .cloned()
        .collect();
    sorted.sort_by(compare_values);
    if descending {
        sorted.reverse();
    }

    let nulls = std::iter::repeat(Value::Null).take(null_count);
    if nulls_last {
        sorted.extend(nulls);
        sorted
    } else {
        nulls.chain(sorted).collect()
    }
}

pub(super) fn parse_integer_runtime_literal(text: &str) -> Value {
    use num_bigint::BigInt;
    use std::str::FromStr;

    let cleaned = text.trim().replace('_', "");
    if let Ok(value) = cleaned.parse::<i64>() {
        Value::Long(value)
    } else {
        BigInt::from_str(&cleaned)
            .map(Value::BigInt)
            .unwrap_or(Value::Null)
    }
}

pub(super) fn parse_runtime_list_literal(text: &str) -> Option<Vec<Value>> {
    let trimmed = text.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for part in split_top_level_commas(inner) {
        out.push(parse_runtime_list_item(part.trim()));
    }
    Some(out)
}

pub(super) fn split_top_level_commas(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn parse_runtime_list_item(text: &str) -> Value {
    let trimmed = text.trim();
    if let Some(items) = parse_runtime_list_literal(trimmed) {
        return Value::List(items);
    }
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return Value::String(trimmed[1..trimmed.len() - 1].to_string());
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "null" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => parse_runtime_atom(trimmed),
    }
}

fn parse_runtime_atom(trimmed: &str) -> Value {
    use num_bigint::BigInt;
    use std::str::FromStr;

    let integer = trimmed
        .strip_prefix('+')
        .unwrap_or(trimmed)
        .strip_prefix('-')
        .unwrap_or_else(|| trimmed.strip_prefix('+').unwrap_or(trimmed));
    if !integer.is_empty() && integer.chars().all(|ch| ch.is_ascii_digit() || ch == '_') {
        let cleaned = trimmed.replace('_', "");
        return cleaned
            .parse::<i64>()
            .map(Value::Long)
            .or_else(|_| BigInt::from_str(&cleaned).map(Value::BigInt))
            .unwrap_or_else(|_| Value::String(trimmed.to_string()));
    }

    if let Ok(value) = trimmed.parse::<f64>() {
        Value::Float(value)
    } else {
        Value::String(trimmed.to_string())
    }
}

pub(super) fn list_product_value(items: &[Value]) -> Value {
    use num_traits::ToPrimitive;

    if items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .any(|item| value_as_bigint(item).is_none() && value_as_f64(item).is_none())
    {
        return Value::String(
            "Binder exception: Unsupported inner data type for LIST_PRODUCT: STRING".to_string(),
        );
    }

    if items.iter().any(|item| matches!(item, Value::Float(_))) {
        return Value::Float(
            items
                .iter()
                .filter_map(value_as_f64)
                .fold(1.0, |product, value| product * value),
        );
    }

    if items.iter().any(|item| matches!(item, Value::Float32(_))) {
        let product = items
            .iter()
            .filter_map(value_as_f64)
            .fold(1.0f32, |product, value| product * value as f32);
        return Value::Float32(product);
    }

    let product = items
        .iter()
        .filter_map(value_as_bigint)
        .fold(num_bigint::BigInt::from(1), |product, value| {
            product * value
        });
    product
        .to_i64()
        .map(Value::Long)
        .unwrap_or(Value::BigInt(product))
}

pub(super) fn display_for_list_to_string(value: &Value) -> String {
    match value {
        Value::VertexProperty {..}|Value::Property {..} => super::strings::display_for_concat(value),
        Value::CardinalityValue {cardinality,value} => format!("[{cardinality}, {}]", display_for_list_to_string(value)),
        Value::MapEntry(entry) => format!("{}={}", display_for_list_to_string(&entry.0), display_for_list_to_string(&entry.1)),
        Value::Token(name) => format!("t[{name}]"),
        Value::Direction(name) => format!("D[{name}]"),
        Value::TypedMap(entries) => format!(
            "{{{}}}",
            entries
                .iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    display_for_list_to_string(key),
                    display_for_list_to_string(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),

        Value::Null => String::new(),
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
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::Temporal(t) => t.to_string(),
        Value::DateTime(s) | Value::String(s) => normalize_list_to_string_text(s),
        Value::List(items) | Value::Set(items) | Value::BulkSet(items) | Value::Path(items) => {
            let parts = items
                .iter()
                .map(display_for_list_to_string)
                .collect::<Vec<_>>();
            format!("[{}]", parts.join(","))
        }
        Value::Map(map) => {
            if let Some(value) = union_display_value(map) {
                return display_for_list_to_string(value);
            }
            if let Some(entries) = kuzu_map_entries(map) {
                let parts = entries
                    .iter()
                    .filter_map(kuzu_map_entry)
                    .map(|(key, value)| {
                        format!(
                            "{}={}",
                            display_for_list_to_string(key),
                            display_for_list_to_string(value)
                        )
                    })
                    .collect::<Vec<_>>();
                return format!("{{{}}}", parts.join(", "));
            }
            let parts = visible_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key)
                        .map(|value| format!("{key}: {}", display_for_list_to_string(value)))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Node { label, id } => format!("{label}#{id}"),
        Value::Edge { rel_type, id, .. } => format!("{rel_type}#{id}"),
    }
}

fn normalize_list_to_string_text(text: &str) -> String {
    let text = unescape_display_quotes(text);
    let trimmed = text.trim();
    if !(trimmed.starts_with('[') || trimmed.starts_with('{')) {
        return text;
    }
    normalize_collection_spacing(&strip_quoted_map_keys(trimmed))
}

fn unescape_display_quotes(text: &str) -> String {
    let mut out = text.to_string();
    while out.contains("\\\"") {
        out = out.replace("\\\"", "\"");
    }
    while out.contains("\\'") {
        out = out.replace("\\'", "'");
    }
    out
}

fn strip_quoted_map_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (idx, ch) = chars[i];
        if ch != '\'' && ch != '"' {
            out.push(ch);
            i += 1;
            continue;
        }

        let quote = ch;
        let content_start = idx + ch.len_utf8();
        let mut j = i + 1;
        while j < chars.len() && chars[j].1 != quote {
            j += 1;
        }
        if j >= chars.len() {
            out.push(ch);
            i += 1;
            continue;
        }
        let content_end = chars[j].0;
        let mut k = j + 1;
        while k < chars.len() && chars[k].1.is_whitespace() {
            k += 1;
        }
        if k < chars.len() && chars[k].1 == ':' {
            out.push_str(&text[content_start..content_end]);
            i = j + 1;
        } else {
            out.push_str(&text[idx..chars[j].0 + quote.len_utf8()]);
            i = j + 1;
        }
    }
    out
}

fn normalize_collection_spacing(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut stack = Vec::new();
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(active) = quote {
            out.push(ch);
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                out.push(ch);
            }
            '[' | '{' => {
                stack.push(ch);
                out.push(ch);
            }
            ']' | '}' => {
                stack.pop();
                out.push(ch);
            }
            ',' if stack.last() == Some(&'[') => {
                out.push(',');
                while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
                    chars.next();
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn list_index(items: &[Value], index: i64) -> Value {
    let len = items.len() as i64;
    if len == 0 {
        return Value::Null;
    }
    let i = if index < 0 { len + index } else { index };
    if i < 0 || i >= len {
        Value::Null
    } else {
        items[i as usize].clone()
    }
}

fn list_index_1_based(items: &[Value], index: i64) -> Value {
    list_element_1_based(items, index).unwrap_or(Value::Null)
}

pub(super) fn list_extract_value(items: &[Value], index: i64) -> IrResult<Value> {
    if index == 0 {
        return Err(InterpretError::Runtime(
            "Runtime exception: List extract takes 1-based position.".to_string(),
        ));
    }
    list_element_1_based(items, index).ok_or_else(|| {
        InterpretError::Runtime(format!(
            "Runtime exception: list_extract(list, index): index={index} is out of range."
        ))
    })
}

pub(super) fn list_element_1_based(items: &[Value], index: i64) -> Option<Value> {
    if index == 0 {
        return None;
    }
    let zero_based = if index < 0 {
        items.len() as i64 + index
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= items.len() as i64 {
        None
    } else {
        Some(items[zero_based as usize].clone())
    }
}

pub(super) fn cypher_subscript(
    target: &Value,
    index: &Value,
    graph: &PropertyGraph,
) -> IrResult<Value> {
    use crate::ir::diagnostics::RuntimeDiagnosis;
    if matches!(target, Value::Null) || matches!(index, Value::Null) {
        return Ok(Value::Null);
    }
    if let Value::List(items) = target {
        return match range_integer_arg(index) {
            Some(index) => Ok(list_index(&items, index)),
            None if matches!(index, Value::Null) => Ok(Value::Null),
            None => Err(InterpretError::Diagnosed {
                code: RuntimeDiagnosis::InvalidType,
                message: "A list index must be an integer".into(),
            }),
        };
    }
    match (target, index) {
        (Value::Temporal(value), Value::String(key)) => Ok(value.component(key)),
        (Value::Map(map), key) if kuzu_map_entries(map).is_some() => {
            Ok(kuzu_map_first(map, key).unwrap_or(Value::Null))
        }
        (Value::Map(map), Value::String(key)) => Ok(map.get(key).cloned().unwrap_or(Value::Null)),
        (
            Value::Node { .. } | Value::Edge { .. } | Value::InternalId { .. },
            Value::String(key),
        ) => Ok(graph_element_property(graph, target, key)),
        (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
        (Value::Map(_), _) => Err(InterpretError::Diagnosed {
            code: RuntimeDiagnosis::MapKeyType,
            message: "A map key must be a string".into(),
        }),
        _ => Err(InterpretError::Diagnosed {
            code: RuntimeDiagnosis::InvalidType,
            message: "Subscript requires a list with an integer index or a property container with a string key".into(),
        }),
    }
}

pub(super) fn list_extract_type_error() -> InterpretError {
    InterpretError::Runtime(
        "Binder exception: Function LIST_EXTRACT did not receive correct arguments:".into(),
    )
}

fn slice_bounds(len: usize, start: &Value, end: &Value) -> (usize, usize) {
    let len_i = len as i64;
    let resolve_start = |value: &Value| -> i64 {
        match value {
            Value::Null => 0,
            _ => match value.as_i64() {
                Some(v) if v < 0 => len_i + v,
                Some(v) => v - 1,
                None => 0,
            },
        }
    };
    let resolve_end = |value: &Value| -> i64 {
        match value {
            Value::Null => len_i,
            _ => match value.as_i64() {
                Some(v) if v < 0 => len_i + v + 1,
                Some(v) => v,
                None => len_i,
            },
        }
    };
    let s = resolve_start(start).clamp(0, len_i) as usize;
    let e = resolve_end(end).clamp(0, len_i) as usize;
    (s.min(e), e)
}

pub(super) fn list_slice_range(items: &[Value], start: &Value, end: &Value) -> Vec<Value> {
    let (s, e) = slice_bounds(items.len(), start, end);
    items[s..e].to_vec()
}

pub(super) fn string_slice_range(text: &str, start: &Value, end: &Value) -> String {
    let chars: Vec<char> = text.chars().collect();
    let (s, e) = slice_bounds(chars.len(), start, end);
    chars[s..e].iter().collect()
}

pub(super) fn cypher_range(start: &Value, end: &Value, step: &Value) -> IrResult<Value> {
    use crate::ir::diagnostics::RuntimeDiagnosis;
    if [start, end, step].iter().any(|value| matches!(value, Value::Null)) {
        return Ok(Value::Null);
    }
    if [start, end, step].iter().any(|value| range_integer_arg(value).is_none()) {
        return Err(InterpretError::Diagnosed {
            code: RuntimeDiagnosis::ArgumentType,
            message: "range requires integer arguments".into(),
        });
    }
    if range_integer_arg(step) == Some(0) {
        return Err(InterpretError::Diagnosed {
            code: RuntimeDiagnosis::NumberOutOfRange,
            message: "range step cannot be zero".into(),
        });
    }
    make_range(start, end, step)
}

pub(super) fn make_range(start: &Value, end: &Value, step: &Value) -> IrResult<Value> {
    if matches!(start, Value::Null) || matches!(end, Value::Null) || matches!(step, Value::Null) {
        return Ok(Value::Null);
    }
    let (Some(s), Some(e), Some(step)) = (
        range_integer_arg(start),
        range_integer_arg(end),
        range_integer_arg(step),
    ) else {
        return Err(InterpretError::Runtime(
            "Binder exception: Function RANGE did not receive correct arguments:".to_string(),
        ));
    };
    if step == 0 {
        return Err(InterpretError::Runtime(
            "Runtime exception: Step of range cannot be 0.".to_string(),
        ));
    }
    let mut out = Vec::new();
    let mut cursor = s;
    if step > 0 {
        while cursor <= e {
            out.push(Value::Long(cursor));
            cursor = match cursor.checked_add(step) {
                Some(next) => next,
                None => break,
            };
        }
    } else {
        while cursor >= e {
            out.push(Value::Long(cursor));
            cursor = match cursor.checked_add(step) {
                Some(next) => next,
                None => break,
            };
        }
    }
    Ok(Value::List(out))
}

pub(super) fn range_integer_arg(value: &Value) -> Option<i64> {
    use num_traits::ToPrimitive;
    match value {
        Value::Byte(value) => Some(*value as i64),
        Value::UInt8(value) => Some(*value as i64),
        Value::Short(value) => Some(*value as i64),
        Value::UInt16(value) => Some(*value as i64),
        Value::Int(value) | Value::Long(value) => Some(*value),
        Value::UInt32(value) => Some(*value as i64),
        Value::UInt64(value) => i64::try_from(*value).ok(),
        Value::BigInt(value) | Value::UInt128(value) => value.to_i64(),
        _ => None,
    }
}

/// TinkerPop RangeLocalStep: a requested width of one emits a scalar for
/// iterables, while maps always remain maps. A missing scalar is unproductive.
pub(super) fn gremlin_local_range(value: &Value, low: i64, high: i64) -> Value {
    let start = low.max(0) as usize;
    let count = if high == -1 { usize::MAX } else { (high.max(0) as usize).saturating_sub(start) };
    let set = crate::ir::value::as_gremlin_set(value);
    if let Some(items) = set.or_else(|| match value {
        Value::List(items) | Value::Set(items) | Value::BulkSet(items) | Value::Path(items) => Some(items.as_slice()),
        _ => None,
    }) {
        if high != -1 && high - low == 1 {
            return items.get(start).cloned().unwrap_or(Value::Null);
        }
        let selected = items.iter().skip(start).take(count).cloned().collect();
        return if set.is_some() { crate::ir::value::gremlin_set(selected) } else { Value::List(selected) };
    }
    match value {
        Value::TypedMap(items) => Value::TypedMap(items.iter().skip(start).take(count).cloned().collect()),
        Value::Map(items) => Value::Map(items.iter().skip(start).take(count).map(|(k,v)|(k.clone(),v.clone())).collect()),
        other => other.clone(),
    }
}

pub(super) fn gremlin_local_tail(value: &Value, count: i64) -> Value {
    let len = match value {
        Value::List(items) | Value::Set(items) | Value::BulkSet(items) | Value::Path(items) => items.len(),
        Value::TypedMap(items) => items.len(),
        Value::Map(items) => crate::ir::value::as_gremlin_set(value).map_or(items.len(), |items| items.len()),
        _ => return value.clone(),
    } as i64;
    gremlin_local_range(value, len.saturating_sub(count.max(0)), len)
}

pub(super) fn gremlin_merge(lhs: &Value, rhs: &Value, traversal: bool) -> IrResult<Value> {
    let is_map = |value: &Value| matches!(value, Value::TypedMap(_) | Value::Map(_)) && crate::ir::value::as_gremlin_set(value).is_none();
    if is_map(lhs) {
        if !is_map(rhs) {
            return Err(InterpretError::Runtime(format!("merge step expected provided argument to evaluate to a Map, encountered {}", rhs.type_name())));
        }
        let entries = |value: &Value| match value {
            Value::TypedMap(items) => items.clone(),
            Value::Map(items) => items.iter().filter(|(key,_)| key.as_str() != crate::ir::value::STRUCT_ORDER_KEY && key.as_str() != crate::ir::value::STRUCT_TYPES_KEY).map(|(key,value)|(Value::String(key.clone()),value.clone())).collect(),
            _ => unreachable!(),
        };
        let mut merged = entries(lhs);
        for (key,value) in entries(rhs) {
            if let Some((_,existing)) = merged.iter_mut().find(|(existing,_)| *existing == key) { *existing = value; }
            else { merged.push((key,value)); }
        }
        return Ok(Value::TypedMap(merged));
    }
    let iterable = |value: &Value| match value {
        Value::List(items) | Value::Path(items) | Value::BulkSet(items) => Some(items.clone()),
        other => crate::ir::value::as_gremlin_set(other).map(|items|items.to_vec()),
    };
    let lhs = iterable(lhs).ok_or_else(|| InterpretError::Runtime(if matches!(lhs,Value::Null) {
        "Incoming traverser for merge step can't be null".into()
    } else { format!("merge step can only take an array or an Iterable type for incoming traversers, encountered {}",lhs.type_name()) }))?;
    if !traversal && is_map(rhs) {
        return Err(InterpretError::Runtime("merge step type mismatch: expected argument to be Iterable but got Map".into()));
    }
    let rhs = iterable(rhs).ok_or_else(|| InterpretError::Runtime(if traversal {
        if matches!(rhs,Value::Null) {"traversal argument for merge step must yield an iterable type, not null".into()}
        else {format!("traversal argument for merge step must yield an iterable type, encountered {}",rhs.type_name())}
    } else if matches!(rhs,Value::Null) {"Argument provided for merge step can't be null".into()}
    else {format!("merge step can only take an array or an Iterable as an argument, encountered {}",rhs.type_name())}))?;
    let mut out=Vec::new();
    for item in lhs.into_iter().chain(rhs) {if !out.contains(&item) {out.push(item);}}
    Ok(crate::ir::value::gremlin_set(out))
}
