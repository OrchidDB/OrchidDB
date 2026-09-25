//! Scalar casts, timestamp parsing, and cast modes.

use super::cast_conversion::{
    cast_bigint_range_error, cast_conversion_error, cast_conversion_to_type_error,
    cast_negative_int128_to_uint128_error, cast_negative_unsigned_error, cast_overflow_error,
    cast_range_error, parse_strict_integerish,
};
use super::casts::{cast_to_date, cast_to_string};
use crate::ir::interpreter::{InterpretError, IrResult};
use crate::ir::value::Value;

pub(super) fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::VertexProperty {..} => "VERTEXPROPERTY".into(),
        Value::Property {..} => "PROPERTY".into(),
        Value::Null => "NULL",
        Value::String(s) if is_uuid_text(s) => "UUID",
        Value::String(_) => "STRING",
        Value::Bool(_) => "BOOL",
        Value::Byte(_) => "INT8",
        Value::UInt8(_) => "UINT8",
        Value::Short(_) => "INT16",
        Value::UInt16(_) => "UINT16",
        Value::Int(_) | Value::Long(_) => "INT64",
        Value::UInt32(_) => "UINT32",
        Value::UInt64(_) => "UINT64",
        Value::Float32(_) => "FLOAT",
        Value::Float(_) => "DOUBLE",
        Value::BigInt(_) => "INT128",
        Value::UInt128(_) => "UINT128",
        Value::BigDecimal(_) => "DECIMAL",
        Value::Temporal(t) => t.kind(),
        Value::DateTime(_) => "TIMESTAMP",
        Value::InternalId { .. } => "INTERNAL_ID",
        Value::Node { .. } => "NODE",
        Value::Edge { .. } => "REL",
        Value::List(_) => "LIST",
        Value::BulkSet(_) => "BULKSET",
        Value::Set(_) => "SET",
        Value::MapEntry(_) => "MAP_ENTRY",
        Value::CardinalityValue {..} => "CARDINALITY_VALUE",
        Value::Map(_) | Value::TypedMap(_) => "STRUCT",
        Value::Token(_) => "TOKEN",
        Value::Direction(_) => "DIRECTION",
        Value::Path(_) => "RECURSIVE_REL",
    }
}

pub(super) fn is_uuid_text(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(idx, byte)| {
        matches!(idx, 8 | 13 | 18 | 23) && *byte == b'-'
            || !matches!(idx, 8 | 13 | 18 | 23) && byte.is_ascii_hexdigit()
    })
}

/// Dispatch Kuzu-style `cast(value, "type-name")` to the per-target
/// helper. Type names follow the case-files convention: SQL primitives
/// (`INT64`, `UINT8`, `FLOAT`, `STRING`, …) plus list suffixes (`INT64[]`).
/// Unknown names fall through to a string cast as a best-effort.
/// Caller-selectable cast semantics. The unified `cast_value` engine
/// branches on this for null/error handling; everything else (target
/// type parsing, numeric coercion, string-to-list lifting) is shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CastMode {
    /// `CAST(...)` / Kuzu `to_*` functions: invalid conversions raise a
    /// runtime error so the harness sees Kuzu-style failure messages.
    ExplicitStrict,
    /// Cypher `toFloat` / `toInteger` / `toBoolean` style helpers:
    /// invalid conversions return null rather than raising.
    #[allow(dead_code)]
    TryOrLenient,
    /// Element-wise recursion inside list / struct / union conversion.
    /// Null elements are passed through unchanged and incompatible
    /// elements degrade to `Value::Null` instead of being promoted to a
    /// top-level conversion error. Top-level "wrong shape" inputs (e.g.
    /// casting a scalar to a list target) still error because that
    /// represents a structural mismatch, not a per-element issue.
    NestedElement,
}

#[derive(Clone, Debug)]
pub(super) struct UnionVariant<'a> {
    pub(super) tag: String,
    pub(super) ty: &'a str,
}

pub(super) fn timestamp_function_value(v: &Value) -> IrResult<Value> {
    match v {
        Value::String(raw) | Value::DateTime(raw) => format_timestamp_for_type(raw, "TIMESTAMP")
            .map(Value::DateTime)
            .ok_or_else(|| timestamp_parse_error(raw)),
        Value::Null => Ok(Value::Null),
        other => Ok(cast_to_timestamp_type(other, "TIMESTAMP")),
    }
}

fn timestamp_parse_error(raw: &str) -> InterpretError {
    InterpretError::Runtime(format!(
        "Conversion exception: Error occurred during parsing TIMESTAMP. Given: \"{raw}\". Expected format: (YYYY-MM-DD hh:mm:ss[.zzzzzz][+-TT[:tt]])"
    ))
}

fn invalid_uuid_error(raw: &str) -> InterpretError {
    InterpretError::Runtime(format!("Conversion exception: Invalid UUID: {raw}"))
}

pub(super) fn cast_to_uuid(v: &Value) -> IrResult<Value> {
    match v {
        Value::String(s) => {
            let inner = s.trim().trim_start_matches('{').trim_end_matches('}');
            let hex: String = inner.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if hex.len() == 32 {
                let lower = hex.to_ascii_lowercase();
                Ok(Value::String(format!(
                    "{}-{}-{}-{}-{}",
                    &lower[..8],
                    &lower[8..12],
                    &lower[12..16],
                    &lower[16..20],
                    &lower[20..32]
                )))
            } else {
                Err(invalid_uuid_error(s))
            }
        }
        Value::Null => Ok(Value::Null),
        other => match cast_to_string(other) {
            Value::String(text) => Err(invalid_uuid_error(&text)),
            _ => Err(cast_conversion_error()),
        },
    }
}

pub(super) fn cast_to_blob(v: &Value) -> IrResult<Value> {
    let text = match v {
        Value::String(text) => text.clone(),
        other => match cast_to_string(other) {
            Value::String(text) => text,
            _ => return Err(cast_conversion_error()),
        },
    };
    blob_bytes(&text)?;
    Ok(Value::String(text))
}

pub(super) fn blob_bytes_for_value(value: &Value) -> IrResult<Vec<u8>> {
    let Value::String(text) = cast_to_blob(value)? else {
        return Err(cast_conversion_error());
    };
    blob_bytes(&text)
}

fn blob_bytes(text: &str) -> IrResult<Vec<u8>> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut bytes = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if !ch.is_ascii() {
            return Err(InterpretError::Runtime(
                "Conversion exception: Invalid byte encountered in STRING -> BLOB conversion. All non-ascii characters must be escaped with hex codes (e.g. \\xAA)"
                    .to_string(),
            ));
        }
        if ch == '\\' && matches!(chars.get(index + 1), Some('x' | 'X')) {
            let first = chars.get(index + 2).copied();
            let second = chars.get(index + 3).copied();
            let (Some(first), Some(second)) = (first, second) else {
                return Err(InterpretError::Runtime(
                    "Conversion exception: Invalid hex escape code encountered in string -> blob conversion: unterminated escape code at end of string"
                        .to_string(),
                ));
            };
            if !first.is_ascii_hexdigit() || !second.is_ascii_hexdigit() {
                return Err(InterpretError::Runtime(format!(
                    "Conversion exception: Invalid hex escape code encountered in string -> blob conversion: \\x{first}{second}"
                )));
            }
            let hex = format!("{first}{second}");
            let byte = u8::from_str_radix(&hex, 16).map_err(|_| cast_conversion_error())?;
            bytes.push(byte);
            index += 4;
            continue;
        }
        bytes.push(ch as u8);
        index += 1;
    }
    Ok(bytes)
}

pub(super) fn encode_blob_text(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii() {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("\\x{byte:02X}"));
        }
    }
    out
}

pub(super) fn cast_to_timestamp_type(v: &Value, type_name: &str) -> Value {
    match v {
        Value::String(raw) | Value::DateTime(raw) => format_timestamp_for_type(raw, type_name)
            .map(Value::DateTime)
            .unwrap_or(Value::Null),
        _ => {
            let Value::DateTime(raw) = cast_to_date(v) else {
                return Value::Null;
            };
            format_timestamp_for_type(&raw, type_name)
                .map(Value::DateTime)
                .unwrap_or(Value::Null)
        }
    }
}

fn format_timestamp_for_type(raw: &str, type_name: &str) -> Option<String> {
    let parsed = parse_timestamp_for_cast(raw)?;
    let max_fraction_digits = match type_name {
        "TIMESTAMP_MS" => Some(3),
        "TIMESTAMP_SEC" | "TIMESTAMP_S" => Some(0),
        _ => Some(6),
    };
    let fraction = format_timestamp_fraction(&parsed.fraction, max_fraction_digits);
    let body = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}{}",
        parsed.year, parsed.month, parsed.day, parsed.hour, parsed.minute, parsed.second, fraction
    );
    if type_name == "TIMESTAMP_TZ" {
        Some(format!("{body}+00"))
    } else {
        Some(body)
    }
}

struct CastTimestampParts {
    year: i64,
    month: u32,
    day: u32,
    hour: i64,
    minute: i64,
    second: i64,
    fraction: String,
}

fn parse_timestamp_for_cast(raw: &str) -> Option<CastTimestampParts> {
    let inner = raw
        .trim()
        .strip_prefix("dt[")
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(raw.trim());
    let normalized = inner.replace('T', " ");
    let (date, time_and_zone) = normalized
        .split_once(' ')
        .map(|(date, time)| (date, time.trim()))
        .unwrap_or((normalized.as_str(), "00:00:00"));

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let (time, offset_minutes) = split_timestamp_cast_offset(time_and_zone)?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second_raw = time_parts.next().unwrap_or("0");
    if time_parts.next().is_some() {
        return None;
    }
    let (second_raw, fraction) = second_raw.split_once('.').unwrap_or((second_raw, ""));
    let second: i64 = second_raw.parse().ok()?;
    if !fraction.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }

    let days = timestamp_days_from_civil(year, month, day)?;
    let local_seconds = days
        .checked_mul(86_400)?
        .checked_add(hour.checked_mul(3600)?)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)?;
    let utc_seconds = local_seconds.checked_sub((offset_minutes as i64).checked_mul(60)?)?;
    let utc_days = utc_seconds.div_euclid(86_400);
    let seconds_of_day = utc_seconds.rem_euclid(86_400);
    let (year, month, day) = timestamp_civil_from_days(utc_days)?;
    Some(CastTimestampParts {
        year,
        month,
        day,
        hour: seconds_of_day / 3600,
        minute: (seconds_of_day % 3600) / 60,
        second: seconds_of_day % 60,
        fraction: fraction
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect(),
    })
}

fn split_timestamp_cast_offset(raw: &str) -> Option<(&str, i32)> {
    if let Some(time) = raw.strip_suffix('Z') {
        return Some((time, 0));
    }
    let Some(offset_idx) = raw
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| (idx > 0 && (ch == '+' || ch == '-')).then_some(idx))
    else {
        return Some((raw, 0));
    };
    let (time, offset) = raw.split_at(offset_idx);
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let offset = &offset[1..];
    let (hours, minutes) = if let Some((hours, minutes)) = offset.split_once(':') {
        (hours.parse::<i32>().ok()?, minutes.parse::<i32>().ok()?)
    } else {
        (offset.parse::<i32>().ok()?, 0)
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some((time, sign * (hours * 60 + minutes)))
}

fn format_timestamp_fraction(fraction: &str, max_digits: Option<usize>) -> String {
    let Some(max_digits) = max_digits else {
        return String::new();
    };
    if max_digits == 0 {
        return String::new();
    }
    let digits: String = fraction.chars().take(max_digits).collect();
    if digits.is_empty() || digits.chars().all(|ch| ch == '0') {
        String::new()
    } else {
        format!(".{digits}")
    }
}

fn timestamp_days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = month as i64 + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era.checked_mul(146_097)?
        .checked_add(doe)?
        .checked_sub(719_468)
}

fn timestamp_civil_from_days(days_since_epoch: i64) -> Option<(i64, u32, u32)> {
    let z = days_since_epoch.checked_add(719_468)?;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    Some((year, m as u32, d as u32))
}

/// Single entry-point for every named-type cast. Routes to the same
/// scalar / list / struct / union machinery regardless of which surface
/// syntax (`CAST(... AS ...)`, `cast(v, "type")`, `to_int32(v)`, ...)
/// produced the call.
pub(super) fn strict_cast_i64(
    value: &Value,
    min: i128,
    max: i128,
    target_type: &str,
) -> IrResult<i64> {
    let string_input = matches!(value, Value::String(_));
    let int128_input = matches!(value, Value::BigInt(_));
    let unsigned_target = min == 0 && target_type.starts_with("UINT");
    let parsed = match value {
        Value::Float32(f) => strict_float_to_i128(*f as f64)?,
        Value::Float(f) => strict_float_to_i128(*f)?,
        other => strict_value_to_i128(other)?,
    };
    if parsed < min || parsed > max {
        return if string_input {
            Err(cast_conversion_to_type_error(value, target_type))
        } else if unsigned_target && int128_input && parsed < 0 {
            Err(cast_negative_unsigned_error(&num_bigint::BigInt::from(
                parsed,
            )))
        } else {
            Err(cast_range_error(parsed, target_type))
        };
    }
    i64::try_from(parsed).map_err(|_| cast_overflow_error())
}

fn strict_value_to_i128(value: &Value) -> IrResult<i128> {
    use num_traits::ToPrimitive;
    match value {
        Value::Byte(n) => Ok(*n as i128),
        Value::UInt8(n) => Ok(*n as i128),
        Value::Short(n) => Ok(*n as i128),
        Value::UInt16(n) => Ok(*n as i128),
        Value::Int(n) | Value::Long(n) => Ok(*n as i128),
        Value::UInt32(n) => Ok(*n as i128),
        Value::UInt64(n) => Ok(*n as i128),
        Value::BigInt(n) => n.to_i128().ok_or_else(cast_overflow_error),
        Value::UInt128(n) => n.to_i128().ok_or_else(cast_overflow_error),
        Value::BigDecimal(n) => n.to_i128().ok_or_else(cast_overflow_error),
        Value::Bool(true) => Ok(1),
        Value::Bool(false) => Ok(0),
        Value::String(s) => parse_strict_integerish(s).ok_or_else(cast_conversion_error),
        _ => Err(cast_conversion_error()),
    }
}

fn strict_float_to_i128(value: f64) -> IrResult<i128> {
    if !value.is_finite() {
        return Err(cast_conversion_error());
    }
    if value < i128::MIN as f64 || value > i128::MAX as f64 {
        return Err(cast_overflow_error());
    }
    Ok(value.round() as i128)
}

fn strict_float_to_bigint(value: f64) -> IrResult<num_bigint::BigInt> {
    use bigdecimal::{BigDecimal, FromPrimitive, RoundingMode};
    if !value.is_finite() {
        return Err(cast_conversion_error());
    }
    let decimal = BigDecimal::from_f64(value).ok_or_else(cast_conversion_error)?;
    let rounded = decimal.with_scale_round(0, RoundingMode::HalfUp);
    Ok(rounded.as_bigint_and_exponent().0)
}

pub(super) fn strict_cast_f64(value: &Value) -> IrResult<f64> {
    use num_traits::ToPrimitive;
    let converted = match value {
        Value::Byte(n) => Ok(*n as f64),
        Value::UInt8(n) => Ok(*n as f64),
        Value::Short(n) => Ok(*n as f64),
        Value::UInt16(n) => Ok(*n as f64),
        Value::Int(n) | Value::Long(n) => Ok(*n as f64),
        Value::UInt32(n) => Ok(*n as f64),
        Value::UInt64(n) => Ok(*n as f64),
        Value::Float32(n) => Ok(*n as f64),
        Value::Float(n) => Ok(*n),
        Value::BigInt(n) => n.to_f64().ok_or_else(cast_overflow_error),
        Value::UInt128(n) => n.to_f64().ok_or_else(cast_overflow_error),
        Value::BigDecimal(n) => n.to_f64().ok_or_else(cast_overflow_error),
        Value::Bool(true) => Ok(1.0),
        Value::Bool(false) => Ok(0.0),
        Value::String(s) => s
            .trim()
            .replace('_', "")
            .parse::<f64>()
            .map_err(|_| cast_conversion_error())
            .and_then(|f| {
                if f.is_finite() {
                    Ok(f)
                } else {
                    Err(cast_overflow_error())
                }
            }),
        _ => Err(cast_conversion_error()),
    }?;
    Ok(converted)
}

pub(super) fn strict_cast_int128(value: &Value) -> IrResult<Value> {
    use num_bigint::BigInt;
    let string_input = matches!(value, Value::String(_));
    let min = BigInt::from(i128::MIN);
    let max = BigInt::from(i128::MAX);
    let converted = strict_cast_unbounded_bigint(value)?;
    let Value::BigInt(n) = &converted else {
        return Err(cast_conversion_error());
    };
    if n < &min || n > &max {
        // Kuzu reports "Cast failed. Could not convert ..." for string
        // inputs that don't fit; out-of-range arithmetic uses
        // "Overflow exception". Match the source-side convention.
        if string_input {
            Err(cast_conversion_to_type_error(value, "INT128"))
        } else {
            Err(cast_bigint_range_error(n, "INT128"))
        }
    } else {
        Ok(converted)
    }
}

pub(super) fn strict_cast_uint64(value: &Value) -> IrResult<Value> {
    use num_bigint::BigInt;
    use num_traits::ToPrimitive;
    let string_input = matches!(value, Value::String(_));
    let int128_input = matches!(value, Value::BigInt(_));
    let max = BigInt::from(u64::MAX);
    let converted = strict_cast_unbounded_bigint(value)?;
    let Value::BigInt(n) = &converted else {
        return Err(cast_conversion_error());
    };
    if n < &BigInt::from(0) {
        if string_input {
            Err(cast_conversion_to_type_error(value, "UINT64"))
        } else if int128_input {
            Err(cast_negative_unsigned_error(n))
        } else {
            Err(cast_bigint_range_error(n, "UINT64"))
        }
    } else if n > &max {
        if string_input {
            Err(cast_conversion_to_type_error(value, "UINT64"))
        } else {
            Err(cast_bigint_range_error(n, "UINT64"))
        }
    } else {
        Ok(Value::UInt64(n.to_u64().ok_or_else(cast_overflow_error)?))
    }
}

pub(super) fn strict_cast_uint128(value: &Value) -> IrResult<Value> {
    use num_bigint::BigInt;
    if matches!(value, Value::Float32(n) if n.is_infinite())
        || matches!(value, Value::Float(n) if n.is_infinite())
    {
        return Ok(Value::BigInt(BigInt::from(0)));
    }
    let string_input = matches!(value, Value::String(_));
    let int128_input = matches!(value, Value::BigInt(_));
    let max = (BigInt::from(1u8) << 128) - BigInt::from(1u8);
    let converted = strict_cast_unbounded_bigint(value)?;
    let Value::BigInt(n) = &converted else {
        return Err(cast_conversion_error());
    };
    if n < &BigInt::from(0) {
        if string_input {
            Err(cast_conversion_to_type_error(value, "UINT128"))
        } else if int128_input {
            Err(cast_negative_int128_to_uint128_error(n))
        } else {
            Err(cast_bigint_range_error(n, "UINT128"))
        }
    } else if n > &max {
        if string_input {
            Err(cast_conversion_to_type_error(value, "UINT128"))
        } else {
            Err(cast_bigint_range_error(n, "UINT128"))
        }
    } else {
        Ok(Value::UInt128(n.clone()))
    }
}

fn strict_cast_unbounded_bigint(value: &Value) -> IrResult<Value> {
    use num_bigint::BigInt;
    use std::str::FromStr;
    Ok(match value {
        Value::BigInt(_) => value.clone(),
        Value::Byte(n) => Value::BigInt(BigInt::from(*n)),
        Value::UInt8(n) => Value::BigInt(BigInt::from(*n)),
        Value::Short(n) => Value::BigInt(BigInt::from(*n)),
        Value::UInt16(n) => Value::BigInt(BigInt::from(*n)),
        Value::Int(n) | Value::Long(n) => Value::BigInt(BigInt::from(*n)),
        Value::UInt32(n) => Value::BigInt(BigInt::from(*n)),
        Value::UInt64(n) => Value::BigInt(BigInt::from(*n)),
        Value::UInt128(n) => Value::BigInt(n.clone()),
        Value::Float32(n) => Value::BigInt(strict_float_to_bigint(*n as f64)?),
        Value::Float(n) => Value::BigInt(strict_float_to_bigint(*n)?),
        Value::BigDecimal(n) => Value::BigInt(BigInt::from(strict_value_to_i128(
            &Value::BigDecimal(n.clone()),
        )?)),
        Value::Bool(true) => Value::BigInt(BigInt::from(1)),
        Value::Bool(false) => Value::BigInt(BigInt::from(0)),
        Value::String(s) => BigInt::from_str(&s.trim().replace('_', ""))
            .ok()
            .map(Value::BigInt)
            .ok_or_else(cast_conversion_error)?,
        _ => return Err(cast_conversion_error()),
    })
}
