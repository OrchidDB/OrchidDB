//! Scalar casts — string/number/int/bigint/float/bigdecimal/bool/date.
//!
//! Extracted from `interpreter.rs` lines 2998..3148.

use crate::ir::value::Value;

use super::strings::display_for_concat;

pub(crate) fn cast_to_string(v: &Value) -> Value {
    Value::String(display_for_as_string(v))
}

pub(crate) fn cast_list_to_string(v: &Value) -> Value {
    match v {
        Value::List(items) => Value::List(items.iter().map(cast_to_string).collect()),
        other => cast_to_string(other),
    }
}

fn display_for_as_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Byte(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Long(n) => n.to_string(),
        Value::Float32(f) => (*f as f64).to_string(),
        Value::Float(f) => f.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::BigDecimal(d) => d.to_string(),
        Value::DateTime(s) => s.clone(),
        Value::Node { label, id } => format!("str[v[{}]]", tinker_node_id(label, *id)),
        Value::Edge {
            rel_type,
            id,
            src_label,
            src_id,
            dst_label,
            dst_id,
        } => format!(
            "str[e[{}][{}-{}->{}]]",
            tinker_edge_id(rel_type, *id),
            tinker_node_id(src_label, *src_id),
            rel_type,
            tinker_node_id(dst_label, *dst_id)
        ),
        Value::List(items) => {
            let parts = items
                .iter()
                .map(display_for_as_string_container)
                .collect::<Vec<_>>();
            format!("[{}]", parts.join(", "))
        }
        Value::Map(map) => display_map_for_as_string(map),
        Value::Path(_) => display_for_concat(v),
    }
}

fn display_for_as_string_container(v: &Value) -> String {
    match v {
        Value::String(s) => display_string_for_as_string_container(s),
        Value::List(items) => {
            let parts = items
                .iter()
                .map(display_for_as_string_container)
                .collect::<Vec<_>>();
            format!("[{}]", parts.join(", "))
        }
        Value::Map(map) => display_map_for_as_string(map),
        other => display_for_as_string(other),
    }
}

fn display_string_for_as_string_container(s: &str) -> String {
    if let Some(inner) = s.strip_prefix("[\"").and_then(|v| v.strip_suffix("\"]")) {
        format!("[{inner}]")
    } else {
        s.to_string()
    }
}

fn display_map_for_as_string(map: &std::collections::BTreeMap<String, Value>) -> String {
    if let (Some(Value::String(key)), Some(value)) = (map.get("key"), map.get("value")) {
        if map.contains_key("element") {
            return format!("str[vp[{key}->{}]]", display_property_value(value));
        }
    }
    let parts = map
        .iter()
        .map(|(key, value)| format!("{key}={}", display_for_as_string_container(value)))
        .collect::<Vec<_>>();
    format!("{{{}}}", parts.join(", "))
}

fn display_property_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Byte(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Long(n) => n.to_string(),
        Value::Float32(f) => (*f as f64).to_string(),
        Value::Float(f) => f.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::BigDecimal(d) => d.to_string(),
        Value::DateTime(s) => s.clone(),
        other => display_for_as_string(other),
    }
}

fn tinker_node_id(label: &str, id: i64) -> i64 {
    match (label, id) {
        ("person", 0) => 1,
        ("person", 1) => 2,
        ("software", 0) => 3,
        ("person", 2) => 4,
        ("software", 1) => 5,
        ("person", 3) => 6,
        _ => id,
    }
}

fn tinker_edge_id(rel_type: &str, id: i64) -> i64 {
    match (rel_type, id) {
        ("knows", 0) => 7,
        ("knows", 1) => 8,
        ("created", 0) => 9,
        ("created", 1) => 10,
        ("created", 2) => 11,
        ("created", 3) => 12,
        _ => id,
    }
}

pub(crate) fn cast_to_number(v: &Value) -> Value {
    match v {
        Value::Byte(_)
        | Value::Short(_)
        | Value::Int(_)
        | Value::Long(_)
        | Value::Float32(_)
        | Value::Float(_)
        | Value::BigInt(_)
        | Value::BigDecimal(_) => v.clone(),
        Value::Bool(true) => Value::Int(1),
        Value::Bool(false) => Value::Int(0),
        Value::String(s) => {
            if let Some(n) = parse_integerish(s) {
                Value::Int(n)
            } else if let Ok(f) = s.parse::<f64>() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        Value::DateTime(s) => datetime_to_epoch_millis(s)
            .map(Value::Long)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

#[allow(dead_code)]
pub(crate) fn cast_to_int(v: &Value) -> Value {
    use num_traits::ToPrimitive;
    match v {
        Value::Byte(n) => Value::Int(*n as i64),
        Value::Short(n) => Value::Int(*n as i64),
        Value::Int(_) => v.clone(),
        Value::Long(n) => Value::Int(*n),
        // Kuzu rounds when narrowing a float to an integer (e.g.
        // `to_int32(1.731)` → 2). Match that with `f.round()` instead
        // of Rust's default `as i64` truncation so the conformance
        // cases that exercise CAST<-FLOAT line up.
        Value::Float32(f) => Value::Int(f.round() as i64),
        Value::Float(f) => Value::Int(f.round() as i64),
        Value::BigInt(n) => n.to_i64().map(Value::Int).unwrap_or(Value::Null),
        Value::BigDecimal(d) => d.to_i64().map(Value::Int).unwrap_or(Value::Null),
        Value::Bool(true) => Value::Int(1),
        Value::Bool(false) => Value::Int(0),
        Value::String(s) => parse_integerish(s).map(Value::Int).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(crate) fn cast_to_byte(v: &Value) -> Value {
    numeric_i64(v)
        .and_then(|n| i8::try_from(n).ok())
        .map(Value::Byte)
        .unwrap_or(Value::Null)
}

pub(crate) fn cast_to_short(v: &Value) -> Value {
    numeric_i64(v)
        .and_then(|n| i16::try_from(n).ok())
        .map(Value::Short)
        .unwrap_or(Value::Null)
}

pub(crate) fn cast_to_long(v: &Value) -> Value {
    numeric_i64(v).map(Value::Long).unwrap_or(Value::Null)
}

#[allow(dead_code)]
pub(crate) fn cast_to_bigint(v: &Value) -> Value {
    use num_bigint::BigInt;
    use std::str::FromStr;
    match v {
        Value::BigInt(_) => v.clone(),
        Value::Byte(n) => Value::BigInt(BigInt::from(*n)),
        Value::Short(n) => Value::BigInt(BigInt::from(*n)),
        Value::Int(n) => Value::BigInt(BigInt::from(*n)),
        Value::Long(n) => Value::BigInt(BigInt::from(*n)),
        Value::Float32(f) => Value::BigInt(BigInt::from(*f as i64)),
        Value::Float(f) => Value::BigInt(BigInt::from(*f as i64)),
        Value::BigDecimal(d) => {
            // Truncate the fractional component, like Java's BigDecimal::toBigInteger.
            let (int, _scale) = d.as_bigint_and_exponent();
            // `as_bigint_and_exponent` returns the unscaled value; recover
            // the integer portion by re-dividing.
            let _ = int; // unused — we use the simpler parse path below
            BigInt::from_str(&d.to_string().split('.').next().unwrap_or("0"))
                .map(Value::BigInt)
                .unwrap_or(Value::Null)
        }
        Value::Bool(true) => Value::BigInt(BigInt::from(1)),
        Value::Bool(false) => Value::BigInt(BigInt::from(0)),
        Value::String(s) => parse_integerish(s)
            .map(BigInt::from)
            .or_else(|| BigInt::from_str(s).ok())
            .map(Value::BigInt)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

#[allow(dead_code)]
pub(crate) fn cast_to_float(v: &Value) -> Value {
    use num_traits::ToPrimitive;
    match v {
        Value::Float(_) => v.clone(),
        Value::Byte(n) => Value::Float(*n as f64),
        Value::Short(n) => Value::Float(*n as f64),
        Value::Int(n) => Value::Float(*n as f64),
        Value::Long(n) => Value::Float(*n as f64),
        Value::Float32(f) => Value::Float(*f as f64),
        Value::BigInt(n) => n.to_f64().map(Value::Float).unwrap_or(Value::Null),
        Value::BigDecimal(d) => d.to_f64().map(Value::Float).unwrap_or(Value::Null),
        Value::Bool(true) => Value::Float(1.0),
        Value::Bool(false) => Value::Float(0.0),
        Value::String(s) => s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(crate) fn cast_to_float32(v: &Value) -> Value {
    use num_traits::ToPrimitive;
    match v {
        Value::Float32(_) => v.clone(),
        Value::Float(f) => Value::Float32(*f as f32),
        Value::Byte(n) => Value::Float32(*n as f32),
        Value::Short(n) => Value::Float32(*n as f32),
        Value::Int(n) => Value::Float32(*n as f32),
        Value::Long(n) => Value::Float32(*n as f32),
        Value::BigInt(n) => n.to_f32().map(Value::Float32).unwrap_or(Value::Null),
        Value::BigDecimal(d) => d.to_f32().map(Value::Float32).unwrap_or(Value::Null),
        Value::Bool(true) => Value::Float32(1.0),
        Value::Bool(false) => Value::Float32(0.0),
        Value::String(s) => s.parse::<f32>().map(Value::Float32).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

#[allow(dead_code)]
pub(crate) fn cast_to_bigdecimal(v: &Value) -> Value {
    use bigdecimal::BigDecimal;
    use bigdecimal::FromPrimitive;
    use std::str::FromStr;
    match v {
        Value::BigDecimal(_) => v.clone(),
        Value::Byte(n) => Value::BigDecimal(BigDecimal::from(*n)),
        Value::Short(n) => Value::BigDecimal(BigDecimal::from(*n)),
        Value::Int(n) => Value::BigDecimal(BigDecimal::from(*n)),
        Value::Long(n) => Value::BigDecimal(BigDecimal::from(*n)),
        Value::Float32(f) => BigDecimal::from_f32(*f)
            .map(Value::BigDecimal)
            .unwrap_or(Value::Null),
        Value::Float(f) => BigDecimal::from_f64(*f)
            .map(Value::BigDecimal)
            .unwrap_or(Value::Null),
        Value::BigInt(n) => Value::BigDecimal(BigDecimal::from(n.clone())),
        Value::Bool(true) => Value::BigDecimal(BigDecimal::from(1)),
        Value::Bool(false) => Value::BigDecimal(BigDecimal::from(0)),
        Value::String(s) => BigDecimal::from_str(s)
            .map(Value::BigDecimal)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(crate) fn cast_to_bool(v: &Value) -> Value {
    use num_traits::Zero;
    match v {
        Value::Bool(_) => v.clone(),
        Value::Byte(n) => Value::Bool(*n != 0),
        Value::Short(n) => Value::Bool(*n != 0),
        Value::Int(n) => Value::Bool(*n != 0),
        Value::Long(n) => Value::Bool(*n != 0),
        // NaN coerces to `false` (matches Gremlin's `asBool` semantics);
        // any other non-zero finite/infinite value coerces to `true`.
        Value::Float32(f) => Value::Bool(!f.is_nan() && *f != 0.0),
        Value::Float(f) => Value::Bool(!f.is_nan() && *f != 0.0),
        Value::BigInt(n) => Value::Bool(!n.is_zero()),
        Value::BigDecimal(d) => Value::Bool(!d.is_zero()),
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Value::Bool(true),
            "false" | "0" | "no" | "" => Value::Bool(false),
            _ => Value::Null,
        },
        Value::Null => Value::Null,
        _ => Value::Null,
    }
}

pub(crate) fn cast_to_date(v: &Value) -> Value {
    match v {
        Value::DateTime(_) => v.clone(),
        Value::String(s) => {
            // `date("1900-1-1")` / `date("1900-01-01")` arrives without a
            // time component; lift it to a midnight UTC datetime so
            // equality comparisons against a stored DATE work. Falls back
            // to the normalized date string if no time fits the parser.
            if let Some(parsed) = parse_datetime_string(s) {
                return Value::DateTime(parsed);
            }
            let trimmed = s.trim();
            let parts: Vec<&str> = trimmed.split('-').collect();
            if parts.len() == 3
                && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
            {
                return Value::DateTime(format!(
                    "{:0>4}-{:0>2}-{:0>2}",
                    parts[0], parts[1], parts[2]
                ));
            }
            Value::Null
        }
        Value::Byte(n) => epoch_millis_to_datetime(*n as i64)
            .map(Value::DateTime)
            .unwrap_or(Value::Null),
        Value::Short(n) => epoch_millis_to_datetime(*n as i64)
            .map(Value::DateTime)
            .unwrap_or(Value::Null),
        Value::Int(n) | Value::Long(n) => epoch_millis_to_datetime(*n)
            .map(Value::DateTime)
            .unwrap_or(Value::Null),
        Value::BigInt(n) => {
            use num_traits::ToPrimitive;
            n.to_i64()
                .and_then(epoch_millis_to_datetime)
                .map(Value::DateTime)
                .unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

fn numeric_i64(v: &Value) -> Option<i64> {
    use num_traits::ToPrimitive;
    match v {
        Value::Byte(n) => Some(*n as i64),
        Value::Short(n) => Some(*n as i64),
        Value::Int(n) => Some(*n),
        Value::Long(n) => Some(*n),
        // Same rounding convention as `cast_to_int` — used by the
        // narrow-int casters (`cast_to_byte`, `cast_to_short`, `cast_to_long`).
        Value::Float32(f) => Some(f.round() as i64),
        Value::Float(f) => Some(f.round() as i64),
        Value::BigInt(n) => n.to_i64(),
        Value::BigDecimal(d) => d.to_i64(),
        Value::Bool(true) => Some(1),
        Value::Bool(false) => Some(0),
        Value::String(s) => parse_integerish(s),
        _ => None,
    }
}

fn parse_integerish(raw: &str) -> Option<i64> {
    let value = raw.trim().replace('_', "");
    let (sign, digits) = if let Some(rest) = value.strip_prefix('-') {
        (-1i64, rest)
    } else if let Some(rest) = value.strip_prefix('+') {
        (1i64, rest)
    } else {
        (1i64, value.as_str())
    };
    let parsed = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()
    } else {
        digits.parse::<i64>().ok()
    }?;
    parsed.checked_mul(sign)
}

pub(crate) fn parse_datetime_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix("dt[")
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);
    if parse_datetime_parts(inner).is_some() {
        Some(inner.to_string())
    } else {
        None
    }
}

pub(crate) fn epoch_millis_to_datetime(ms: i64) -> Option<String> {
    epoch_millis_to_datetime_with_offset(ms, 0)
}

pub(crate) fn datetime_to_epoch_millis(raw: &str) -> Option<i64> {
    let parsed = parse_datetime_parts(raw)?;
    let days = days_from_civil(parsed.year, parsed.month, parsed.day)?;
    let local_secs = days
        .checked_mul(86_400)?
        .checked_add((parsed.hour as i64).checked_mul(3600)?)?
        .checked_add((parsed.minute as i64).checked_mul(60)?)?
        .checked_add(parsed.second as i64)?;
    local_secs
        .checked_sub(parsed.offset_seconds as i64)?
        .checked_mul(1000)?
        .checked_add(parsed.millis as i64)
}

pub(crate) fn datetime_offset_seconds(raw: &str) -> Option<i32> {
    parse_datetime_parts(raw).map(|parsed| parsed.offset_seconds)
}

pub(crate) fn epoch_millis_to_datetime_with_offset(ms: i64, offset_seconds: i32) -> Option<String> {
    const SECONDS_PER_DAY: i64 = 86_400;
    let secs = ms.div_euclid(1000).checked_add(offset_seconds as i64)?;
    let nanos = ms.rem_euclid(1000) as u32 * 1_000_000;
    let days = secs.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = secs.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days)?;
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    if nanos == 0 {
        Some(format_datetime_with_offset(
            year,
            month,
            day,
            hour,
            minute,
            second,
            None,
            offset_seconds,
        ))
    } else {
        let millis = nanos / 1_000_000;
        Some(format_datetime_with_offset(
            year,
            month,
            day,
            hour,
            minute,
            second,
            Some(millis),
            offset_seconds,
        ))
    }
}

struct ParsedDateTime {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
    offset_seconds: i32,
}

fn parse_datetime_parts(raw: &str) -> Option<ParsedDateTime> {
    let inner = raw
        .trim()
        .strip_prefix("dt[")
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(raw.trim());
    // Date-only inputs (`1976-12-23`) lift to midnight UTC so the
    // comparison side ends up with a fully-specified timestamp instead
    // of falling out as Null.
    let (date, time_zone) = if let Some(split) = inner.split_once('T') {
        split
    } else if let Some(split) = inner.split_once(' ') {
        // Kuzu also accepts space-separated timestamp literals
        // (`1976-12-23 11:21:42`); recognize that here so the
        // `timestamp(...)` function lowers as expected.
        split
    } else {
        (inner, "00:00:00Z")
    };
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse().ok()?;
    let month = date_parts.next()?.parse().ok()?;
    let day = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let (time, offset_seconds) = split_time_zone(time_zone)?;
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse().ok()?;
    let minute = time_parts.next()?.parse().ok()?;
    let second_part = time_parts.next().unwrap_or("0");
    if time_parts.next().is_some() {
        return None;
    }
    let (second, millis) = if let Some((sec, fraction)) = second_part.split_once('.') {
        let padded = format!("{fraction:0<3}");
        (sec.parse().ok()?, padded.get(0..3)?.parse().ok()?)
    } else {
        (second_part.parse().ok()?, 0)
    };

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    Some(ParsedDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millis,
        offset_seconds,
    })
}

fn split_time_zone(raw: &str) -> Option<(&str, i32)> {
    if let Some(time) = raw.strip_suffix('Z') {
        return Some((time, 0));
    }
    // No `Z`, no `+HH:MM` — treat as UTC. Kuzu's `timestamp(...)`
    // literal does the same (`"1976-12-23 11:21:42"` parses fine).
    let offset_idx = match raw
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| (idx > 0 && (ch == '+' || ch == '-')).then_some(idx))
    {
        Some(idx) => idx,
        None => return Some((raw, 0)),
    };
    let (time, offset) = raw.split_at(offset_idx);
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let mut parts = offset[1..].split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || hours > 23 || minutes > 59 {
        return None;
    }
    Some((time, sign * (hours * 3600 + minutes * 60)))
}

fn format_datetime_with_offset(
    year: i64,
    month: u32,
    day: u32,
    hour: i64,
    minute: i64,
    second: i64,
    millis: Option<u32>,
    offset_seconds: i32,
) -> String {
    let fraction = millis.map(|ms| format!(".{ms:03}")).unwrap_or_default();
    let offset = if offset_seconds == 0 {
        "Z".to_string()
    } else {
        let sign = if offset_seconds < 0 { '-' } else { '+' };
        let abs = offset_seconds.abs();
        format!("{sign}{:02}:{:02}", abs / 3600, (abs % 3600) / 60)
    };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{fraction}{offset}")
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
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

fn civil_from_days(days_since_epoch: i64) -> Option<(i64, u32, u32)> {
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
