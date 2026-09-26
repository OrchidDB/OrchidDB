//! Date arithmetic and interval normalization.

use crate::ir::value::Value;
use super::temporal;
use super::casts::{
    datetime_offset_seconds, datetime_to_epoch_millis, epoch_millis_to_datetime_with_offset,
};

pub(super) fn date_add_value(input: &str, unit: &str, amount: &Value) -> Value {
    let Some(amount) = amount.as_i64() else {
        return Value::Null;
    };
    let Some(base_ms) = datetime_to_epoch_millis(input) else {
        return Value::Null;
    };
    let unit_ms = match unit.trim_start_matches("dt.").to_ascii_lowercase().as_str() {
        "second" => 1_000,
        "minute" => 60_000,
        "hour" => 3_600_000,
        "day" => 86_400_000,
        _ => return Value::Null,
    };
    let Some(delta) = amount.checked_mul(unit_ms) else {
        return Value::Null;
    };
    let Some(result_ms) = base_ms.checked_add(delta) else {
        return Value::Null;
    };
    let offset = datetime_offset_seconds(input).unwrap_or(0);
    epoch_millis_to_datetime_with_offset(result_ms, offset)
        .map(Value::DateTime)
        .unwrap_or(Value::Null)
}

pub(super) fn date_diff_value(lhs: &str, rhs: &Value) -> Value {
    let Some(lhs_ms) = datetime_to_epoch_millis(lhs) else {
        return Value::Null;
    };
    let rhs_ms = match rhs {
        Value::Null => 0,
        Value::DateTime(s) => match datetime_to_epoch_millis(s) {
            Some(ms) => ms,
            None => return Value::Null,
        },
        other => match other.as_i64() {
            Some(ms) => ms,
            None => return Value::Null,
        },
    };
    lhs_ms
        .checked_sub(rhs_ms)
        .map(|milliseconds| Value::Long(milliseconds / 1_000))
        .unwrap_or(Value::Null)
}

pub(super) fn normalize_interval_spec(spec: &str) -> String {
    let trimmed = spec.trim();
    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 || parts.len() % 2 != 0 {
        return spec.to_string();
    }

    let mut prefix: Vec<String> = Vec::new();
    let mut hours = 0i64;
    let mut minutes = 0i64;
    let mut seconds = 0i64;
    let mut micros = 0i64;
    let mut saw_time_component = false;
    for chunk in parts.chunks_exact(2) {
        let Ok(value) = chunk[0].parse::<i64>() else {
            return spec.to_string();
        };
        match chunk[1].to_ascii_lowercase().as_str() {
            "year" | "years" | "y" | "yr" | "yrs" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "year" } else { "years" }
                ));
            }
            "month" | "months" | "mon" | "mons" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "month" } else { "months" }
                ));
            }
            "week" | "weeks" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "week" } else { "weeks" }
                ));
            }
            "day" | "days" | "d" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "day" } else { "days" }
                ));
            }
            "hour" | "hours" | "h" | "hr" | "hrs" => {
                hours += value;
                saw_time_component = true;
            }
            "minute" | "minutes" | "m" | "min" | "mins" => {
                minutes += value;
                saw_time_component = true;
            }
            "second" | "seconds" | "s" | "sec" | "secs" => {
                seconds += value;
                saw_time_component = true;
            }
            "millisecond" | "milliseconds" | "ms" => {
                micros += value * 1_000;
                saw_time_component = true;
            }
            "microsecond" | "microseconds" | "us" | "µs" => {
                micros += value;
                saw_time_component = true;
            }
            _ => return spec.to_string(),
        }
    }

    if !saw_time_component {
        return if prefix.is_empty() {
            "0:00:00".to_string()
        } else {
            prefix.join(" ")
        };
    }

    let time_tail = if micros != 0 {
        format!(
            "{hours:02}:{minutes:02}:{seconds:02}.{}",
            trim_interval_fraction(micros)
        )
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    };
    if prefix.is_empty() {
        time_tail
    } else {
        format!("{} {time_tail}", prefix.join(" "))
    }
}

fn trim_interval_fraction(micros: i64) -> String {
    let mut fraction = format!("{:06}", micros.abs());
    while fraction.ends_with('0') {
        fraction.pop();
    }
    if fraction.is_empty() {
        "0".to_string()
    } else {
        fraction
    }
}

pub(super) fn kuzu_datetime_display(value: &str) -> String {
    let mut out = value.trim_end_matches('Z').replace('T', " ");
    if let Some(dot) = out.rfind('.') {
        while out.ends_with('0') {
            out.pop();
        }
        if out.len() == dot + 1 {
            out.truncate(dot);
        }
    }
    out
}

/// `date_part("year", "2024-06-15T...")` style extraction. The format the
/// runtime stores is the ISO 8601 string from `cast_to_date`, so we
/// pull components out by splitting on `-`/`T`/`:`/`.`. Unknown units
/// yield `null`.
pub(super) fn date_part(unit: &str, value: &str) -> Value {
    temporal::temporal_part(unit, value).unwrap_or(Value::Null)
}

pub(super) fn strip_utc_suffix(value: Value) -> Value {
    match value {
        Value::DateTime(text) => Value::DateTime(
            text.strip_suffix("+00")
                .unwrap_or(text.as_str())
                .to_string(),
        ),
        Value::String(text) => Value::String(
            text.strip_suffix("+00")
                .unwrap_or(text.as_str())
                .to_string(),
        ),
        other => other,
    }
}
