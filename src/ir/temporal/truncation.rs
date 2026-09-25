//! Truncation to calendar/clock units with component overrides.
use super::{Result, TemporalValue, construct, parsing::resolve_zone, value::offset_text};
use crate::ir::Value;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike};
pub fn truncate(kind: &str, unit: &str, input: &Value, overrides: &Value) -> Result<Value> {
    if input == &Value::Null {
        return Ok(Value::Null);
    }
    let Value::Temporal(input) = input else {
        return Err("Temporal truncation requires a temporal value".into());
    };
    let Value::Map(overrides) = overrides else {
        return Err("Temporal overrides must be a map".into());
    };
    let unit = unit.to_ascii_lowercase();
    let date = input.date();
    let time = input.time();
    let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let calendar = matches!(
        unit.as_str(),
        "millennium"
            | "century"
            | "decade"
            | "year"
            | "weekyear"
            | "quarter"
            | "month"
            | "week"
            | "day"
    );
    let date = if let Some(d) = date {
        Some(match unit.as_str() {
            "millennium" | "century" | "decade" => {
                let factor = match unit.as_str() {
                    "millennium" => 1000,
                    "century" => 100,
                    _ => 10,
                };
                NaiveDate::from_ymd_opt(d.year().div_euclid(factor) * factor, 1, 1)
                    .ok_or("Truncation outside calendar range")?
            }
            "year" => NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap(),
            "weekyear" => {
                NaiveDate::from_isoywd_opt(d.iso_week().year(), 1, chrono::Weekday::Mon).unwrap()
            }
            "quarter" => {
                NaiveDate::from_ymd_opt(d.year(), ((d.month() - 1) / 3) * 3 + 1, 1).unwrap()
            }
            "month" => NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap(),
            "week" => d
                .checked_sub_signed(Duration::days(d.weekday().num_days_from_monday() as i64))
                .ok_or("Week outside calendar range")?,
            _ => d,
        })
    } else {
        None
    };
    let time = if calendar {
        midnight
    } else {
        let t = time.ok_or("Truncation unit requires a time")?;
        match unit.as_str() {
            "hour" => NaiveTime::from_hms_opt(t.hour(), 0, 0).unwrap(),
            "minute" => NaiveTime::from_hms_opt(t.hour(), t.minute(), 0).unwrap(),
            "second" => t.with_nanosecond(0).unwrap(),
            "millisecond" => t
                .with_nanosecond(t.nanosecond() / 1_000_000 * 1_000_000)
                .unwrap(),
            "microsecond" => t.with_nanosecond(t.nanosecond() / 1_000 * 1_000).unwrap(),
            "nanosecond" => t,
            _ => return Err(format!("Unknown truncation unit {unit}")),
        }
    };
    let base = match input {
        TemporalValue::Date(_) => TemporalValue::Date(date.ok_or("Date required")?),
        TemporalValue::LocalTime(_) => TemporalValue::LocalTime(time),
        TemporalValue::Time(_, o) => TemporalValue::Time(time, *o),
        TemporalValue::LocalDateTime(_) => {
            TemporalValue::LocalDateTime(date.ok_or("Date required")?.and_time(time))
        }
        TemporalValue::DateTime(d, zone) => {
            let (dt, zone) = resolve_zone(
                date.ok_or("Date required")?.and_time(time),
                &zone
                    .clone()
                    .unwrap_or_else(|| offset_text(d.offset().local_minus_utc())),
            )?;
            TemporalValue::DateTime(dt, zone)
        }
        TemporalValue::Duration { .. } => {
            return Err("Cannot truncate a duration as a calendar value".into());
        }
    };
    let mut fields = overrides.clone();
    let base_key = if base.date().is_some() {
        "datetime"
    } else {
        "time"
    };
    fields.insert(base_key.into(), Value::Temporal(base));
    construct(kind, &Value::Map(fields))
}
