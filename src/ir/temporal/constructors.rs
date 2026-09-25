//! Typed temporal constructors and projection from existing values.
use super::{Result, TemporalValue, arithmetic::normalized_duration, parse, parsing::resolve_zone};
use crate::ir::Value;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, TimeZone, Timelike};
use std::collections::BTreeMap;
fn number(fields: &BTreeMap<String, Value>, key: &str, default: i64) -> Result<i64> {
    match fields.get(key) {
        None => Ok(default),
        Some(Value::Int(n) | Value::Long(n)) => Ok(*n),
        Some(Value::BigInt(n)) => n
            .to_string()
            .parse()
            .map_err(|_| format!("{key} out of range")),
        _ => Err(format!("{key} must be an integer")),
    }
}
fn temporal<'a>(fields: &'a BTreeMap<String, Value>, names: &[&str]) -> Option<&'a TemporalValue> {
    names.iter().find_map(|key| match fields.get(*key) {
        Some(Value::Temporal(v)) => Some(v),
        _ => None,
    })
}
pub fn construct(kind: &str, value: &Value) -> Result<Value> {
    if matches!(value, Value::Null) {
        return Ok(Value::Null);
    }
    if let Value::String(text) = value {
        return parse(kind, text).map(Value::Temporal);
    }
    if let Value::Temporal(t) = value {
        return Ok(Value::Temporal(match kind {
            "date" => TemporalValue::Date(t.date().ok_or("Cannot project date")?),
            "localtime" => TemporalValue::LocalTime(t.time().ok_or("Cannot project time")?),
            "localdatetime" => TemporalValue::LocalDateTime(
                t.date()
                    .ok_or("Cannot project date")?
                    .and_time(t.time().ok_or("Cannot project time")?),
            ),
            "time" => TemporalValue::Time(
                t.time().ok_or("Cannot project time")?,
                match t {
                    TemporalValue::Time(_, o) => *o,
                    TemporalValue::DateTime(d, _) => d.offset().local_minus_utc(),
                    _ => 0,
                },
            ),
            "datetime" if t.date().is_some() => {
                let (d, z) = resolve_zone(
                    t.date().unwrap().and_time(
                        t.time()
                            .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
                    ),
                    &match t.component("timezone") {
                        Value::String(z) => z,
                        _ => "Z".into(),
                    },
                )?;
                TemporalValue::DateTime(d, z)
            }
            k if k == t.kind() => t.clone(),
            _ => return Err("Incompatible temporal projection".into()),
        }));
    }
    let Value::Map(fields) = value else {
        return Err("Temporal constructor expects a string, map or temporal value".into());
    };
    let fields = fields
        .iter()
        .filter(|(key, _)| {
            key.as_str() != crate::ir::value::STRUCT_ORDER_KEY
                && key.as_str() != crate::ir::value::STRUCT_TYPES_KEY
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    let fields = &fields;
    if kind == "duration" {
        return duration(fields).map(Value::Temporal);
    }
    const FIELDS: &[&str] = &[
        "year",
        "month",
        "day",
        "week",
        "dayOfWeek",
        "ordinalDay",
        "quarter",
        "dayOfQuarter",
        "hour",
        "minute",
        "second",
        "millisecond",
        "microsecond",
        "nanosecond",
        "timezone",
        "date",
        "time",
        "datetime",
        "epochSeconds",
        "epochMillis",
    ];
    if fields.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return Err("Unknown temporal component".into());
    }
    for key in ["date", "time", "datetime"] {
        if let Some(value) = fields.get(key) {
            let Value::Temporal(value) = value else {
                return Err(format!("{key} requires a temporal value"));
            };
            if (key == "date" && value.date().is_none())
                || (key == "time" && value.time().is_none())
                || (key == "datetime" && value.date().is_none())
            {
                return Err(format!("Cannot project {key}"));
            }
        }
    }
    let date_groups = [
        fields.contains_key("month") || fields.contains_key("day"),
        fields.contains_key("week") || fields.contains_key("dayOfWeek"),
        fields.contains_key("ordinalDay"),
        fields.contains_key("quarter") || fields.contains_key("dayOfQuarter"),
    ];
    if date_groups.iter().filter(|present| **present).count() > 1 {
        return Err("Conflicting calendar components".into());
    }
    if fields.contains_key("epochSeconds") && fields.contains_key("epochMillis") {
        return Err("Conflicting epoch components".into());
    }
    if (fields.contains_key("epochSeconds") || fields.contains_key("epochMillis"))
        && kind != "datetime"
    {
        return Err("Epoch constructor requires datetime".into());
    }

    if fields.contains_key("epochSeconds") || fields.contains_key("epochMillis") {
        let dt = if fields.contains_key("epochMillis") {
            DateTime::from_timestamp_millis(number(fields, "epochMillis", 0)?)
        } else {
            DateTime::from_timestamp(
                number(fields, "epochSeconds", 0)?,
                u32::try_from(number(fields, "nanosecond", 0)?)
                    .map_err(|_| "Invalid nanosecond")?,
            )
        }
        .ok_or("Epoch out of range")?;
        return Ok(Value::Temporal(TemporalValue::DateTime(
            dt.fixed_offset(),
            None,
        )));
    }
    let inherited = temporal(fields, &["datetime", "date"]);
    let base = inherited
        .and_then(TemporalValue::date)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
    let year = i32::try_from(number(fields, "year", base.year() as i64)?)
        .map_err(|_| "Year out of range")?;
    let u = |key, default| {
        number(fields, key, default)
            .and_then(|n| u32::try_from(n).map_err(|_| format!("{key} out of range")))
    };
    let date = if fields.contains_key("week") || fields.contains_key("dayOfWeek") {
        let year = if fields.contains_key("year") {
            year
        } else {
            base.iso_week().year()
        };
        let week = u("week", base.iso_week().week() as i64)?;
        let day = u(
            "dayOfWeek",
            if inherited.is_some() {
                base.weekday().number_from_monday() as i64
            } else {
                1
            },
        )?;
        if !(1..=7).contains(&day) {
            return Err("Day of week out of range".into());
        }
        NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)
            .and_then(|d| d.checked_add_signed(Duration::days(day as i64 - 1)))
    } else if fields.contains_key("ordinalDay") {
        NaiveDate::from_yo_opt(year, u("ordinalDay", 1)?)
    } else if fields.contains_key("quarter") || fields.contains_key("dayOfQuarter") {
        let q = u("quarter", ((base.month() - 1) / 3 + 1) as i64)?;
        if !(1..=4).contains(&q) {
            return Err("Quarter out of range".into());
        }
        NaiveDate::from_ymd_opt(year, (q - 1) * 3 + 1, 1).and_then(|d| {
            let day = number(
                fields,
                "dayOfQuarter",
                if inherited.is_some() {
                    (base
                        - NaiveDate::from_ymd_opt(
                            base.year(),
                            ((base.month() - 1) / 3) * 3 + 1,
                            1,
                        )?)
                    .num_days()
                        + 1
                } else {
                    1
                },
            )
            .ok()?;
            if day < 1 {
                return None;
            }
            let result = d.checked_add_signed(Duration::try_days(day - 1)?)?;
            (result.year() == year && (result.month() - 1) / 3 + 1 == q).then_some(result)
        })
    } else {
        NaiveDate::from_ymd_opt(
            year,
            u(
                "month",
                if inherited.is_some() {
                    base.month() as i64
                } else {
                    1
                },
            )?,
            u(
                "day",
                if inherited.is_some() {
                    base.day() as i64
                } else {
                    1
                },
            )?,
        )
    }
    .ok_or("Invalid calendar date")?;
    if kind == "date" {
        return Ok(Value::Temporal(TemporalValue::Date(date)));
    }
    let base_time = temporal(fields, &["datetime", "time"]).and_then(TemporalValue::time);
    let nano = if fields.contains_key("nanosecond") {
        number(fields, "nanosecond", 0)?
    } else if fields.contains_key("microsecond") {
        number(fields, "microsecond", 0)?
            .checked_mul(1000)
            .ok_or("Microsecond overflow")?
    } else if fields.contains_key("millisecond") {
        number(fields, "millisecond", 0)?
            .checked_mul(1_000_000)
            .ok_or("Millisecond overflow")?
    } else {
        base_time.map(|t| t.nanosecond() as i64).unwrap_or(0)
    };
    if !(0..1_000_000_000).contains(&nano) {
        return Err("Nanosecond out of range".into());
    }
    let time = NaiveTime::from_hms_nano_opt(
        u("hour", base_time.map(|t| t.hour() as i64).unwrap_or(0))?,
        u("minute", base_time.map(|t| t.minute() as i64).unwrap_or(0))?,
        u("second", base_time.map(|t| t.second() as i64).unwrap_or(0))?,
        u32::try_from(nano).map_err(|_| "Nanosecond out of range")?,
    )
    .ok_or("Invalid local time")?;
    let value = match kind {
        "localtime" => TemporalValue::LocalTime(time),
        "localdatetime" => TemporalValue::LocalDateTime(date.and_time(time)),
        "datetime" | "time" => {
            let inherited_zone =
                temporal(fields, &["datetime", "time"]).map(|t| t.component("timezone"));
            let zone = fields.get("timezone").or(inherited_zone.as_ref());
            let zone = match zone {
                Some(Value::String(z)) => z.as_str(),
                None => "Z",
                _ => return Err("Timezone must be a string".into()),
            };
            let (mut dt, name) = resolve_zone(date.and_time(time), zone)?;
            if fields.contains_key("timezone") {
                let source_offset = temporal(fields, &["datetime", "time"]).and_then(|t| match t {
                    TemporalValue::Time(_, o) => Some(*o),
                    TemporalValue::DateTime(d, _) => Some(d.offset().local_minus_utc()),
                    _ => None,
                });
                if let Some(source_offset) = source_offset {
                    let original = FixedOffset::east_opt(source_offset)
                        .and_then(|o| o.from_local_datetime(&date.and_time(time)).single())
                        .ok_or("Invalid source offset")?;
                    dt = if let Some(name) = &name {
                        let tz: chrono_tz::Tz = name.parse().map_err(|_| "Invalid timezone")?;
                        original.with_timezone(&tz).fixed_offset()
                    } else {
                        original.with_timezone(dt.offset())
                    };
                }
            }
            if kind == "time" {
                TemporalValue::Time(dt.time(), dt.offset().local_minus_utc())
            } else {
                TemporalValue::DateTime(dt, name)
            }
        }
        _ => return Err(format!("Unknown temporal constructor {kind}")),
    };
    Ok(Value::Temporal(value))
}
pub(super) fn duration(fields: &BTreeMap<String, Value>) -> Result<TemporalValue> {
    use bigdecimal::BigDecimal;
    use num_traits::ToPrimitive;
    let number = |key: &str| -> Result<BigDecimal> {
        match fields.get(key) {
            None => Ok(BigDecimal::from(0)),
            Some(Value::Int(v) | Value::Long(v)) => Ok(BigDecimal::from(*v)),
            Some(Value::BigInt(v)) => Ok(BigDecimal::from(v.clone())),
            Some(Value::BigDecimal(v)) => Ok(v.clone()),
            Some(Value::Float(v)) if v.is_finite() => v
                .to_string()
                .parse()
                .map_err(|_| "Invalid duration component".into()),
            _ => Err(format!("Invalid duration component {key}")),
        }
    };
    const KEYS: &[&str] = &[
        "years",
        "months",
        "weeks",
        "days",
        "hours",
        "minutes",
        "seconds",
        "milliseconds",
        "microseconds",
        "nanoseconds",
    ];
    if fields.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return Err("Unknown duration component".into());
    }
    let months = number("years")? * BigDecimal::from(12) + number("months")?;
    let whole_months = months.to_i64().ok_or("Duration months overflow")?;
    let calendar_seconds = (number("weeks")? * BigDecimal::from(7) + number("days")?)
        * BigDecimal::from(86400)
        + (months - BigDecimal::from(whole_months)) * BigDecimal::from(2_629_746);
    let whole_days = (&calendar_seconds / BigDecimal::from(86400))
        .to_i64()
        .ok_or("Duration days overflow")?;
    let seconds = calendar_seconds - BigDecimal::from(whole_days) * BigDecimal::from(86400)
        + number("hours")? * BigDecimal::from(3600)
        + number("minutes")? * BigDecimal::from(60)
        + number("seconds")?;
    let nanos = seconds * BigDecimal::from(1_000_000_000)
        + number("milliseconds")? * BigDecimal::from(1_000_000)
        + number("microseconds")? * BigDecimal::from(1000)
        + number("nanoseconds")?;
    normalized_duration(
        whole_months,
        whole_days,
        nanos.to_i128().ok_or("Duration overflow")?,
    )
}
