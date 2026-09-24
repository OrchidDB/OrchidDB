//! Arrow property decoding and debug-encoded collection values.

use super::*;

pub(super) fn column_value(batch: &RecordBatch, name: &str, row_id: i64) -> Value {
    let row = row_id as usize;
    let Some(idx) = schema_index(batch.schema().as_ref(), name) else {
        return Value::Null;
    };
    let field = batch.schema().field(idx).clone();
    array_value(batch.column(idx).as_ref(), row, Some(field.as_ref()))
}

fn schema_index(schema: &Schema, name: &str) -> Option<usize> {
    if let Ok(idx) = schema.index_of(name) {
        return Some(idx);
    }
    let mut matches = schema
        .fields()
        .iter()
        .enumerate()
        .filter_map(|(idx, field)| field.name().eq_ignore_ascii_case(name).then_some(idx));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

pub(super) fn map_property_value(props: &BTreeMap<String, Value>, key: &str) -> Value {
    map_property_value_if_present(props, key)
        .cloned()
        .unwrap_or(Value::Null)
}

pub(super) fn map_property_value_if_present<'a>(
    props: &'a BTreeMap<String, Value>,
    key: &str,
) -> Option<&'a Value> {
    if let Some(value) = props.get(key) {
        return Some(value);
    }
    let mut matches = props
        .iter()
        .filter_map(|(name, value)| name.eq_ignore_ascii_case(key).then_some(value));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

pub(crate) fn array_value(array: &dyn Array, row: usize, field: Option<&Field>) -> Value {
    if row >= array.len() || array.is_null(row) {
        return Value::Null;
    }
    match array.data_type() {
        DataType::Int64 => Value::Int(
            array
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(row),
        ),
        DataType::Int32 => Value::Int(
            array
                .as_any()
                .downcast_ref::<arrow::array::Int32Array>()
                .unwrap()
                .value(row) as i64,
        ),
        DataType::Float64 => Value::Float(
            array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(row),
        ),
        DataType::Boolean => Value::Bool(
            array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .unwrap()
                .value(row),
        ),
        DataType::Utf8 => Value::String({
            let value = array
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(row)
                .to_string();
            match field.and_then(|field| field.metadata().get("new_graph.value_type")) {
                Some(kind) if kind == "datetime" => return Value::DateTime(value),
                Some(kind) if kind == "map" || kind == "value" => {
                    return parse_debug_value(&value).unwrap_or(Value::Null);
                }
                _ => {}
            }
            value
        }),
        _ => Value::Null,
    }
}

pub(crate) fn parse_debug_value(input: &str) -> Option<Value> {
    let input = input.trim();
    if input == "Null" {
        return Some(Value::Null);
    }
    if input == "Bool(true)" {
        return Some(Value::Bool(true));
    }
    if input == "Bool(false)" {
        return Some(Value::Bool(false));
    }
    if let Some(inner) = input.strip_prefix("Int(").and_then(|s| s.strip_suffix(')')) {
        return inner.parse::<i64>().ok().map(Value::Int);
    }
    if let Some(inner) = input
        .strip_prefix("Byte(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<i8>().ok().map(Value::Byte);
    }
    if let Some(inner) = input
        .strip_prefix("Short(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<i16>().ok().map(Value::Short);
    }
    if let Some(inner) = input
        .strip_prefix("Long(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<i64>().ok().map(Value::Long);
    }
    if let Some(inner) = input
        .strip_prefix("Float(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<f64>().ok().map(Value::Float);
    }
    if let Some(inner) = input
        .strip_prefix("Float32(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<f32>().ok().map(Value::Float32);
    }
    if let Some(inner) = input
        .strip_prefix("BigInt(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return BigInt::from_str(inner).ok().map(Value::BigInt);
    }
    if let Some(inner) = input
        .strip_prefix("BigDecimal(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return BigDecimal::from_str(inner).ok().map(Value::BigDecimal);
    }
    if let Some(inner) = input
        .strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return Some(Value::String(unescape_debug_string(inner)));
    }
    if let Some(inner) = input
        .strip_prefix("DateTime(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return Some(Value::DateTime(unescape_debug_string(inner)));
    }
    if let Some(inner) = input
        .strip_prefix("List([")
        .and_then(|s| s.strip_suffix("])"))
    {
        if inner.trim().is_empty() {
            return Some(Value::List(Vec::new()));
        }
        return split_debug_items(inner)
            .into_iter()
            .map(|item| parse_debug_value(&item))
            .collect::<Option<Vec<_>>>()
            .map(Value::List);
    }
    if let Some(inner) = input
        .strip_prefix("Map({")
        .and_then(|s| s.strip_suffix("})"))
    {
        let mut map = std::collections::BTreeMap::new();
        if inner.trim().is_empty() {
            return Some(Value::Map(map));
        }
        for entry in split_debug_items(inner) {
            let (key, value) = split_debug_map_entry(&entry)?;
            map.insert(key, parse_debug_value(value.trim())?);
        }
        return Some(Value::Map(map));
    }
    None
}

fn split_debug_map_entry(entry: &str) -> Option<(String, &str)> {
    let (key, value) = split_top_level_once(entry, ':')?;
    let key = key
        .trim()
        .strip_prefix('"')?
        .strip_suffix('"')
        .map(unescape_debug_string)?;
    Some((key, value))
}

fn unescape_debug_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('0') => out.push('\0'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn split_debug_items(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut quote = false;
    let mut escape = false;
    let mut depth = 0usize;
    for ch in input.chars() {
        if quote {
            buf.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                quote = false;
            }
            continue;
        }
        match ch {
            '"' => {
                quote = true;
                buf.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                buf.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                buf.push(ch);
            }
            ',' if depth == 0 => {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn split_top_level_once(input: &str, needle: char) -> Option<(&str, &str)> {
    let mut quote = false;
    let mut escape = false;
    let mut depth = 0usize;
    for (idx, ch) in input.char_indices() {
        if quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                quote = false;
            }
            continue;
        }
        match ch {
            '"' => quote = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ch if ch == needle && depth == 0 => {
                return Some((&input[..idx], &input[idx + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}
