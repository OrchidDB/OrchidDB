//! Named type conversion and composite type coercion.

use crate::ir::runtime::{RuntimeError, IrResult};
use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY, Value};
use std::collections::BTreeMap;
use super::temporal;
use super::CastMode;
use super::cast_scalar::{
    cast_to_blob, cast_to_timestamp_type, cast_to_uuid, strict_cast_f64, strict_cast_i64,
    strict_cast_int128, strict_cast_uint128, strict_cast_uint64,
};
use super::cast_union::cast_to_union_unified;
use super::casts::{cast_to_bigdecimal, cast_to_bool, cast_to_date, cast_to_string};
use super::lists::{parse_runtime_list_literal, split_top_level_commas};
use super::maps::{kuzu_map_entries, kuzu_map_entry, make_kuzu_map_strict, struct_field_order};

pub(super) fn cast_value(v: &Value, type_name: &str, mode: CastMode) -> IrResult<Value> {
    let cleaned = type_name.trim();
    let upper = cleaned.to_ascii_uppercase();
    if matches!(v, Value::Null) {
        return Ok(Value::Null);
    }
    if let Some((elem_type, expected_len)) = array_suffix(cleaned) {
        let items = coerce_to_list(v, mode)?;
        if expected_len.is_some_and(|len| len != items.len()) {
            return mode_conversion_error(mode);
        }
        return Ok(Value::List(
            items
                .iter()
                .map(|item| cast_value(item, elem_type, CastMode::ExplicitStrict))
                .collect::<IrResult<Vec<_>>>()?,
        ));
    }
    if let Some(elem_type) =
        type_argument(cleaned, "LIST").or_else(|| type_argument(cleaned, "ARRAY"))
    {
        let items = coerce_to_list(v, mode)?;
        return Ok(Value::List(
            items
                .iter()
                .map(|item| cast_value(item, elem_type, CastMode::ExplicitStrict))
                .collect::<IrResult<Vec<_>>>()?,
        ));
    }
    if let Some(fields) = type_argument(cleaned, "STRUCT") {
        return cast_to_struct_unified(v, fields, mode);
    }
    if let Some(fields) = type_argument(cleaned, "UNION") {
        return cast_to_union_unified(v, fields, cleaned, mode);
    }
    if let Some(args) = type_argument(cleaned, "MAP") {
        return cast_to_map_unified(v, args, mode);
    }
    if let Some((precision, scale)) = decimal_precision_scale(cleaned) {
        return cast_to_parametric_decimal(v, precision, scale, mode);
    }
    // Strip parametric tails like `DECIMAL(5, 2)` so the scalar match
    // below sees the base type name.
    let upper: std::borrow::Cow<'_, str> = if upper.contains('(') {
        std::borrow::Cow::Owned(upper.split('(').next().unwrap_or("").trim().to_string())
    } else {
        std::borrow::Cow::Borrowed(&upper)
    };
    match upper.as_ref() {
        "STRING" | "VARCHAR" | "CHAR" | "TEXT" => Ok(cast_to_string(v)),
        "BLOB" | "BYTEA" => cast_to_blob(v),
        "INTERVAL" => Ok(match cast_to_string(v) {
            Value::String(text) => Value::String(
                temporal::parse_interval_strict(&text)
                    .map(temporal::format_interval)
                    .map_err(|err| RuntimeError::Runtime(err.message().to_string()))?,
            ),
            other => other,
        }),
        "INT8" | "TINYINT" => {
            strict_or_lenient_i64(v, i8::MIN as i128, i8::MAX as i128, "INT8", mode)
                .map(|opt| opt.map(|n| Value::Byte(n as i8)).unwrap_or(Value::Null))
        }
        "UINT8" => strict_or_lenient_i64(v, 0, u8::MAX as i128, "UINT8", mode)
            .map(|opt| opt.map(|n| Value::UInt8(n as u8)).unwrap_or(Value::Null)),
        "INT16" | "SMALLINT" => {
            strict_or_lenient_i64(v, i16::MIN as i128, i16::MAX as i128, "INT16", mode)
                .map(|opt| opt.map(|n| Value::Short(n as i16)).unwrap_or(Value::Null))
        }
        "UINT16" => strict_or_lenient_i64(v, 0, u16::MAX as i128, "UINT16", mode)
            .map(|opt| opt.map(|n| Value::UInt16(n as u16)).unwrap_or(Value::Null)),
        "INT32" | "INT" | "INTEGER" => {
            strict_or_lenient_i64(v, i32::MIN as i128, i32::MAX as i128, "INT32", mode)
                .map(|opt| opt.map(Value::Int).unwrap_or(Value::Null))
        }
        "UINT32" => strict_or_lenient_i64(v, 0, u32::MAX as i128, "UINT32", mode)
            .map(|opt| opt.map(|n| Value::UInt32(n as u32)).unwrap_or(Value::Null)),
        "INT64" | "BIGINT" | "LONG" | "SERIAL" => {
            strict_or_lenient_i64(v, i64::MIN as i128, i64::MAX as i128, "INT64", mode)
                .map(|opt| opt.map(Value::Long).unwrap_or(Value::Null))
        }
        "UINT64" => strict_or_lenient_uint64(v, mode),
        "INT128" => match strict_cast_int128(v) {
            Ok(value) => Ok(value),
            Err(err) => downgrade_or_err(err, mode),
        },
        "UINT128" => match strict_cast_uint128(v) {
            Ok(value) => Ok(value),
            Err(err) => downgrade_or_err(err, mode),
        },
        "FLOAT" | "FLOAT32" | "REAL" => match strict_cast_f64(v).and_then(|f| {
            let narrowed = f as f32;
            if !narrowed.is_nan() {
                Ok(Value::Float32(narrowed))
            } else {
                Err(cast_overflow_error())
            }
        }) {
            Ok(value) => Ok(value),
            Err(err) => downgrade_or_err(err, mode),
        },
        "DOUBLE" | "FLOAT64" => match strict_cast_f64(v).map(Value::Float) {
            Ok(value) => Ok(value),
            Err(err) => downgrade_or_err(err, mode),
        },
        // Kuzu's bare `DECIMAL` defaults to DECIMAL(18, 3).
        "DECIMAL" | "NUMERIC" => cast_to_parametric_decimal(v, 18, 3, mode),
        "BOOL" | "BOOLEAN" => match cast_to_bool(v) {
            Value::Null => mode_conversion_error(mode),
            value => Ok(value),
        },
        "DATE" => match cast_to_date(v) {
            Value::Null => mode_conversion_error(mode),
            value => Ok(value),
        },
        "TIMESTAMP" | "DATETIME" | "TIMESTAMP_NS" | "TIMESTAMP_MS" | "TIMESTAMP_SEC"
        | "TIMESTAMP_S" | "TIMESTAMP_TZ" => match cast_to_timestamp_type(v, upper.as_ref()) {
            Value::Null => mode_conversion_error(mode),
            value => Ok(value),
        },
        "UUID" => match cast_to_uuid(v) {
            Ok(Value::Null) => mode_conversion_error(mode),
            Ok(value) => Ok(value),
            Err(err) => downgrade_or_err(err, mode),
        },
        _ => mode_conversion_error(mode),
    }
}

/// Lift the input to a `Vec<Value>` suitable for an element-wise cast.
/// Already-listed values pass through; Kuzu-style string list literals
/// (`"[1, 2, 3]"`) are parsed; everything else either errors (strict)
/// or yields `Null` (lenient/nested).
fn coerce_to_list(v: &Value, _mode: CastMode) -> IrResult<Vec<Value>> {
    match v {
        Value::List(items) => Ok(items.clone()),
        // Ladybug fixtures store list/array properties as raw CSV
        // strings; lift those to structured lists here so the
        // recursive element cast sees real values.
        Value::String(s) => parse_list_literal(s).ok_or_else(cast_conversion_error),
        _ => Err(cast_conversion_error()),
    }
}

/// Parse a Kuzu-style list literal (`"[a, b, [c, d]]"`) into raw
/// `Value::String` elements; element-typed conversion happens in the
/// recursive `cast_value` call. Whitespace and trailing/leading
/// brackets are trimmed; bare `null` / `NULL` tokens become
/// `Value::Null`.
fn parse_list_literal(raw: &str) -> Option<Vec<Value>> {
    let trimmed = raw.trim();
    let body = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))?;
    if body.trim().is_empty() {
        return Some(Vec::new());
    }
    let parts = split_top_level_brackets(body);
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let item = part.trim();
        if item.eq_ignore_ascii_case("null") {
            out.push(Value::Null);
        } else if item.starts_with('[') {
            // Nested list: keep as a raw string so the recursive cast
            // can re-parse it under the inner element type.
            out.push(Value::String(item.to_string()));
        } else {
            // Strip surrounding quotes if present.
            let cleaned = if (item.starts_with('"') && item.ends_with('"') && item.len() >= 2)
                || (item.starts_with('\'') && item.ends_with('\'') && item.len() >= 2)
            {
                &item[1..item.len() - 1]
            } else {
                item
            };
            out.push(Value::String(cleaned.to_string()));
        }
    }
    Some(out)
}

/// Split on commas that sit at bracket / brace depth zero.
fn split_top_level_brackets(text: &str) -> Vec<&str> {
    let mut depth: i32 = 0;
    let mut last = 0usize;
    let mut out = Vec::new();
    for (idx, ch) in text.char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[last..idx]);
                last = idx + 1;
            }
            _ => {}
        }
    }
    out.push(&text[last..]);
    out
}

/// Parse a Kuzu-style struct/map literal (`"{a: 1, b: [2, 3]}"`) into
/// a `BTreeMap<String, Value>`. Each value is left as a `Value::String`
/// (or `Value::Null`) so the recursive cast can re-interpret it under
/// the declared field type. Returns `None` if the input does not look
/// like a brace-delimited map.
fn parse_struct_literal(raw: &str) -> Option<BTreeMap<String, Value>> {
    let trimmed = raw.trim();
    let body = trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))?;
    let mut out = BTreeMap::new();
    if body.trim().is_empty() {
        return Some(out);
    }
    for part in split_top_level_brackets(body) {
        let entry = part.trim();
        if entry.is_empty() {
            continue;
        }
        // `key: value` (struct) or `key = value` (Kuzu map form). Use
        // the first `:` or `=` at depth zero so list/struct values
        // containing punctuation are preserved verbatim.
        let mut depth: i32 = 0;
        let mut sep_idx: Option<usize> = None;
        for (idx, ch) in entry.char_indices() {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth -= 1,
                ':' | '=' if depth == 0 => {
                    sep_idx = Some(idx);
                    break;
                }
                _ => {}
            }
        }
        let sep_idx = sep_idx?;
        let key_raw = entry[..sep_idx].trim();
        let value_raw = entry[sep_idx + 1..].trim();
        let key = key_raw.trim_matches('"').trim_matches('\'').to_string();
        let value = if value_raw.is_empty() || value_raw.eq_ignore_ascii_case("null") {
            Value::Null
        } else if (value_raw.starts_with('"') && value_raw.ends_with('"') && value_raw.len() >= 2)
            || (value_raw.starts_with('\'') && value_raw.ends_with('\'') && value_raw.len() >= 2)
        {
            Value::String(value_raw[1..value_raw.len() - 1].to_string())
        } else {
            Value::String(value_raw.to_string())
        };
        out.insert(key, value);
    }
    Some(out)
}

pub(super) fn mode_conversion_error(mode: CastMode) -> IrResult<Value> {
    match mode {
        CastMode::ExplicitStrict => Err(cast_conversion_error()),
        CastMode::TryOrLenient | CastMode::NestedElement => Ok(Value::Null),
    }
}

fn downgrade_or_err(err: RuntimeError, mode: CastMode) -> IrResult<Value> {
    match mode {
        CastMode::ExplicitStrict => Err(err),
        CastMode::TryOrLenient | CastMode::NestedElement => Ok(Value::Null),
    }
}

/// `strict_cast_i64` but returning `Ok(None)` for the lenient / nested
/// modes instead of raising. Strict mode preserves the original
/// conversion/overflow distinction the harness expects.
fn strict_or_lenient_i64(
    value: &Value,
    min: i128,
    max: i128,
    target_type: &str,
    mode: CastMode,
) -> IrResult<Option<i64>> {
    match strict_cast_i64(value, min, max, target_type) {
        Ok(n) => Ok(Some(n)),
        Err(err) => match mode {
            CastMode::ExplicitStrict => Err(err),
            CastMode::TryOrLenient | CastMode::NestedElement => Ok(None),
        },
    }
}

fn strict_or_lenient_uint64(value: &Value, mode: CastMode) -> IrResult<Value> {
    match strict_cast_uint64(value) {
        Ok(value) => Ok(value),
        Err(err) => downgrade_or_err(err, mode),
    }
}

pub(super) fn strict_cast_to_named_type(v: &Value, type_name: &str) -> IrResult<Value> {
    cast_value(v, type_name, CastMode::ExplicitStrict)
}

fn cast_to_struct_unified(value: &Value, fields: &str, mode: CastMode) -> IrResult<Value> {
    // Allow string-typed struct literals (`"{a: 1, b: 2}"`) since the
    // Ladybug fixtures store map/struct properties as raw CSV text.
    let parsed_map;
    let allow_missing_fields;
    let map_ref: &BTreeMap<String, Value> = match value {
        Value::Map(map) => {
            allow_missing_fields = !matches!(mode, CastMode::ExplicitStrict);
            map
        }
        Value::String(s) => match parse_struct_literal(s) {
            Some(parsed) => {
                allow_missing_fields = true;
                parsed_map = parsed;
                &parsed_map
            }
            None => return mode_conversion_error(mode),
        },
        _ => return mode_conversion_error(mode),
    };
    let map = map_ref;
    let parsed_fields = split_top_level_commas(fields)
        .into_iter()
        .map(|field| {
            let Some((name, ty)) = split_struct_field(field) else {
                return None;
            };
            let key = name.trim().trim_matches('"').trim_matches('\'').to_string();
            (!key.is_empty()).then_some((key, ty.trim()))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(cast_conversion_error)?;

    if !allow_missing_fields {
        if let Some(source_order) = struct_field_order(map) {
            let target_order = parsed_fields
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            if source_order != target_order {
                return Err(cast_conversion_error());
            }
        }
    }

    let mut out = BTreeMap::new();
    let mut order = Vec::new();
    let mut types = BTreeMap::new();
    for (key, ty) in &parsed_fields {
        order.push(Value::String(key.clone()));
        types.insert(key.clone(), Value::String((*ty).to_string()));
        match map.get(key.as_str()) {
            Some(value) => {
                if matches!(value, Value::Null)
                    && !allow_missing_fields
                    && !null_field_cast_compatible(map, key, ty)
                {
                    return Err(cast_conversion_error());
                }
                out.insert(
                    key.clone(),
                    cast_value(value, ty, CastMode::ExplicitStrict)?,
                );
            }
            None if allow_missing_fields => {
                out.insert(key.clone(), Value::Null);
            }
            None => return Err(cast_conversion_error()),
        }
    }
    out.insert(STRUCT_ORDER_KEY.to_string(), Value::List(order));
    out.insert(STRUCT_TYPES_KEY.to_string(), Value::Map(types));
    Ok(Value::Map(out))
}

fn null_field_cast_compatible(
    source: &BTreeMap<String, Value>,
    field: &str,
    target_type: &str,
) -> bool {
    let Some(source_type) = struct_field_type(source, field) else {
        return true;
    };
    type_shape(&source_type) == type_shape(target_type)
}

fn struct_field_type(map: &BTreeMap<String, Value>, field: &str) -> Option<String> {
    let Value::Map(types) = map.get(STRUCT_TYPES_KEY)? else {
        return None;
    };
    let Value::String(ty) = types.get(field)? else {
        return None;
    };
    Some(ty.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypeShape {
    Scalar,
    List,
    Struct,
    Map,
    Union,
}

fn type_shape(type_name: &str) -> TypeShape {
    let cleaned = type_name.trim();
    if array_suffix(cleaned).is_some()
        || type_argument(cleaned, "LIST").is_some()
        || type_argument(cleaned, "ARRAY").is_some()
    {
        TypeShape::List
    } else if type_argument(cleaned, "STRUCT").is_some() {
        TypeShape::Struct
    } else if type_argument(cleaned, "MAP").is_some() {
        TypeShape::Map
    } else if type_argument(cleaned, "UNION").is_some() {
        TypeShape::Union
    } else {
        TypeShape::Scalar
    }
}

fn cast_to_map_unified(value: &Value, args: &str, mode: CastMode) -> IrResult<Value> {
    let Some((key_type, value_type)) = split_map_types(args) else {
        return mode_conversion_error(mode);
    };
    let entries = match coerce_to_map_entries(value) {
        Some(entries) => entries,
        None => return mode_conversion_error(mode),
    };
    let mut keys = Vec::with_capacity(entries.len());
    let mut values = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        keys.push(cast_value(&key, key_type, CastMode::ExplicitStrict)?);
        values.push(cast_value(&value, value_type, CastMode::ExplicitStrict)?);
    }
    make_kuzu_map_strict(keys, values)
}

fn cast_to_parametric_decimal(
    value: &Value,
    precision: u64,
    scale: u64,
    mode: CastMode,
) -> IrResult<Value> {
    use bigdecimal::{BigDecimal, RoundingMode};
    use num_bigint::BigInt;

    if precision == 0 || scale > precision {
        return mode_conversion_error(mode);
    }
    let decimal = match cast_to_bigdecimal(value) {
        Value::BigDecimal(decimal) => decimal,
        Value::Null => return mode_conversion_error(mode),
        _ => return mode_conversion_error(mode),
    };
    let rounded = decimal.with_scale_round(scale as i64, RoundingMode::HalfUp);
    let integer_digits = precision - scale;
    let limit = BigDecimal::from(BigInt::from(10u8).pow(integer_digits as u32));
    if rounded.abs() >= limit {
        return match mode {
            CastMode::ExplicitStrict => Ok(Value::String(decimal_overflow_message(
                value, &decimal, precision, scale,
            ))),
            CastMode::TryOrLenient | CastMode::NestedElement => Ok(Value::Null),
        };
    }
    Ok(Value::BigDecimal(rounded))
}

fn decimal_precision_scale(type_name: &str) -> Option<(u64, u64)> {
    let args =
        type_argument(type_name, "DECIMAL").or_else(|| type_argument(type_name, "NUMERIC"))?;
    let parts = split_top_level_commas(args);
    let [precision, scale] = parts.as_slice() else {
        return None;
    };
    Some((precision.trim().parse().ok()?, scale.trim().parse().ok()?))
}

fn decimal_overflow_message(
    value: &Value,
    decimal: &bigdecimal::BigDecimal,
    precision: u64,
    scale: u64,
) -> String {
    match value {
        Value::String(text) => format!(
            "Conversion exception: Cast failed. {} is not in DECIMAL({}, {}) range.",
            text.trim(),
            precision,
            scale
        ),
        Value::BigDecimal(_) => format!(
            "Overflow exception: Decimal Cast Failed: input {} is not in range of DECIMAL({}, {})",
            decimal, precision, scale
        ),
        other => format!(
            "Overflow exception: To Decimal Cast Failed: {} is not in DECIMAL({}, {}) range",
            decimal_input_display(other, decimal),
            precision,
            scale
        ),
    }
}

fn decimal_input_display(value: &Value, decimal: &bigdecimal::BigDecimal) -> String {
    match value {
        Value::Byte(n) => n.to_string(),
        Value::UInt8(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::UInt16(n) => n.to_string(),
        Value::Int(n) | Value::Long(n) => n.to_string(),
        Value::UInt32(n) => n.to_string(),
        Value::UInt64(n) => n.to_string(),
        Value::Float32(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::UInt128(n) => n.to_string(),
        Value::Bool(true) => "1".to_string(),
        Value::Bool(false) => "0".to_string(),
        _ => decimal.to_string(),
    }
}

fn split_map_types(args: &str) -> Option<(&str, &str)> {
    let parts = split_top_level_commas(args);
    let [key, value] = parts.as_slice() else {
        return None;
    };
    Some((key.trim(), value.trim()))
}

fn coerce_to_map_entries(value: &Value) -> Option<Vec<(Value, Value)>> {
    match value {
        Value::Map(map) => {
            if let Some(entries) = kuzu_map_entries(map) {
                return Some(
                    entries
                        .iter()
                        .filter_map(kuzu_map_entry)
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                );
            }
            if map.contains_key(STRUCT_ORDER_KEY) {
                return None;
            }
            Some(
                map.iter()
                    .map(|(key, value)| (Value::String(key.clone()), value.clone()))
                    .collect(),
            )
        }
        Value::String(text) => parse_map_literal_entries(text),
        _ => None,
    }
}

fn parse_map_literal_entries(raw: &str) -> Option<Vec<(Value, Value)>> {
    let trimmed = raw.trim();
    let body = trimmed.strip_prefix('{')?.strip_suffix('}')?;
    if body.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for part in split_top_level_brackets(body) {
        let entry = part.trim();
        if entry.is_empty() {
            continue;
        }
        let Some(sep_idx) = find_top_level_map_separator(entry) else {
            return None;
        };
        let key = parse_map_cast_atom(entry[..sep_idx].trim());
        let value = parse_map_cast_atom(entry[sep_idx + 1..].trim());
        out.push((key, value));
    }
    Some(out)
}

fn parse_map_cast_atom(trimmed: &str) -> Value {
    if let Some(items) = parse_runtime_list_literal(trimmed) {
        return Value::List(items);
    }
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return Value::String(trimmed[1..trimmed.len() - 1].to_string());
    }
    if trimmed.eq_ignore_ascii_case("null") {
        Value::Null
    } else {
        Value::String(trimmed.to_string())
    }
}

fn find_top_level_map_separator(entry: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (idx, ch) in entry.char_indices() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ':' | '=' if depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
}

pub(super) fn parse_strict_integerish(raw: &str) -> Option<i128> {
    let value = raw.trim().replace('_', "");
    let (sign, digits) = if let Some(rest) = value.strip_prefix('-') {
        (-1i128, rest)
    } else if let Some(rest) = value.strip_prefix('+') {
        (1i128, rest)
    } else {
        (1i128, value.as_str())
    };
    let parsed = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i128::from_str_radix(hex, 16).ok()
    } else {
        digits.parse::<i128>().ok()
    }?;
    parsed.checked_mul(sign)
}

pub(super) fn array_suffix(type_name: &str) -> Option<(&str, Option<usize>)> {
    let trimmed = type_name.trim();
    if !trimmed.ends_with(']') {
        return None;
    }
    let open = trimmed.rfind('[')?;
    let size = trimmed[open + 1..trimmed.len() - 1].trim();
    if size.is_empty() {
        Some((trimmed[..open].trim(), None))
    } else {
        size.parse::<usize>()
            .ok()
            .map(|len| (trimmed[..open].trim(), Some(len)))
    }
}

pub(super) fn cast_conversion_error() -> RuntimeError {
    RuntimeError::Runtime("Conversion exception:".to_string())
}

pub(super) fn cast_overflow_error() -> RuntimeError {
    RuntimeError::Runtime("Overflow exception:".to_string())
}

pub(super) fn cast_conversion_to_type_error(value: &Value, target_type: &str) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Conversion exception: Cast failed. Could not convert \"{}\" to {}.",
        cast_input_text(value),
        target_type
    ))
}

pub(super) fn cast_range_error(value: i128, target_type: &str) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Overflow exception: Value {value} is not within {target_type} range"
    ))
}

pub(super) fn cast_bigint_range_error(value: &num_bigint::BigInt, target_type: &str) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Overflow exception: Value {value} is not within {target_type} range"
    ))
}

pub(super) fn cast_negative_unsigned_error(value: &num_bigint::BigInt) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Overflow exception: Cast failed. Cannot cast {value} to unsigned type."
    ))
}

pub(super) fn cast_negative_int128_to_uint128_error(value: &num_bigint::BigInt) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Overflow exception: Cannot cast negative INT128 value {value} to UINT128"
    ))
}

fn cast_input_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_string(),
        Value::Byte(n) => n.to_string(),
        Value::UInt8(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::UInt16(n) => n.to_string(),
        Value::Int(n) | Value::Long(n) => n.to_string(),
        Value::UInt32(n) => n.to_string(),
        Value::UInt64(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::UInt128(n) => n.to_string(),
        Value::Float32(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::BigDecimal(n) => n.to_string(),
        Value::Bool(value) => value.to_string(),
        other => format!("{other:?}"),
    }
}

fn strip_array_suffix(type_name: &str) -> Option<&str> {
    let trimmed = type_name.trim();
    if !trimmed.ends_with(']') {
        return None;
    }
    let open = trimmed.rfind('[')?;
    if trimmed[open + 1..trimmed.len() - 1].trim().is_empty()
        || trimmed[open + 1..trimmed.len() - 1]
            .trim()
            .chars()
            .all(|ch| ch.is_ascii_digit())
    {
        Some(trimmed[..open].trim())
    } else {
        None
    }
}

pub(super) fn type_argument<'a>(type_name: &'a str, head: &str) -> Option<&'a str> {
    let trimmed = type_name.trim();
    if !trimmed
        .get(..head.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(head))
    {
        return None;
    }
    let rest = trimmed[head.len()..].trim_start();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner.trim())
}

pub(super) fn split_struct_field(field: &str) -> Option<(&str, &str)> {
    let trimmed = field.trim();
    let mut depth = 0i32;
    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ':' if depth == 0 => return Some((&trimmed[..idx], &trimmed[idx + 1..])),
            ch if ch.is_whitespace() && depth == 0 => {
                let name = trimmed[..idx].trim();
                let ty = trimmed[idx..].trim();
                if !name.is_empty() && !ty.is_empty() {
                    return Some((name, ty));
                }
            }
            _ => {}
        }
    }
    None
}
