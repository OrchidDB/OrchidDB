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
    if let TemporalValue::WideDate(date) | TemporalValue::WideLocalDateTime(date, _) = input {
        let mut representative = date.representative();
        let mut year_delta = date.year - representative.year();
        if unit.eq_ignore_ascii_case("millennium") {
            let year = date.year.div_euclid(1000) * 1000;
            let base = super::CalendarDate::new(year, 1, 1)?;
            representative = base.representative();
            year_delta = year - representative.year();
        }
        let proxy = if let Some(time) = input.time() { TemporalValue::LocalDateTime(representative.and_time(time)) }
            else { TemporalValue::Date(representative) };
        let actual_unit = if unit.eq_ignore_ascii_case("millennium") { "year" } else { unit };
        let Value::Temporal(result) = truncate(kind, actual_unit, &Value::Temporal(proxy), &Value::Map(overrides.clone()))?
            else { return Err("Expected temporal truncation result".into()); };
        let projected = result.date().ok_or("Expected calendar result")?;
        let date = super::CalendarDate::new(projected.year() + year_delta, projected.month(), projected.day())?;
        return Ok(Value::Temporal(if let Some(time) = result.time() { date.datetime_value(time) }
            else { date.date_value() }));
    }
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
        TemporalValue::WideDate(_) | TemporalValue::WideLocalDateTime(..) => unreachable!("wide calendar handled above"),
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
    // Truncation overrides replace the zone on the truncated local value,
    // unlike constructor projection, which preserves an existing instant.
    let base = if fields.contains_key("timezone") {
        match base {
            TemporalValue::DateTime(d, _) => TemporalValue::LocalDateTime(d.naive_local()),
            TemporalValue::Time(t, _) => TemporalValue::LocalTime(t),
            other => other,
        }
    } else {
        base
    };
    // Subsecond overrides fill the discarded part, preserving the retained
    // millisecond/microsecond prefix from truncation.
    if matches!(unit.as_str(), "millisecond" | "microsecond")
        && ["nanosecond", "microsecond", "millisecond"]
            .iter()
            .any(|key| fields.contains_key(*key))
    {
        if let Some(time) = base.time() {
            let retained = time.nanosecond() as i64;
            let addition = match fields.get("nanosecond") {
                None => 0,
                Some(Value::Int(v) | Value::Long(v)) => *v,
                _ => return Err("Nanosecond must be an integer".into()),
            };
            fields.insert(
                "nanosecond".into(),
                Value::Int(
                    retained
                        .checked_add(addition)
                        .ok_or("Nanosecond overflow")?,
                ),
            );
        }
    }

    let base_key = if base.date().is_some() {
        "datetime"
    } else {
        "time"
    };
    fields.insert(base_key.into(), Value::Temporal(base));
    construct(kind, &Value::Map(fields))
}
