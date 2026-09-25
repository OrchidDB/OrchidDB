//! Calendar, clock, timezone, and duration component access.
use super::{TemporalValue, value::offset_text};
use crate::ir::Value;
use chrono::{Datelike, NaiveDate, Timelike};
impl TemporalValue {
    pub fn component(&self, key: &str) -> Value {
        if let Self::WideDate(date) | Self::WideLocalDateTime(date, _) = self {
            let representative = date.representative();
            let proxy = match self {
                Self::WideLocalDateTime(_, time) => Self::LocalDateTime(representative.and_time(*time)),
                _ => Self::Date(representative),
            };
            let value = proxy.component(key);
            return if matches!(key.to_ascii_lowercase().as_str(), "year" | "weekyear") {
                match value { Value::Int(year) => Value::Int(year + i64::from(date.year - representative.year())), other => other }
            } else { value };
        }
        if let Self::Duration {
            months,
            days,
            seconds,
            nanos,
        } = self
        {
            let n = match key.to_ascii_lowercase().as_str() {
                "years" => *months / 12,
                "quarters" => *months / 3,
                "months" => *months,
                "monthsofyear" => *months % 12,
                "monthsofquarter" => *months % 3,
                "quartersofyear" => *months / 3 % 4,
                "weeks" => *days / 7,
                "days" => *days,
                "daysofweek" => *days % 7,
                "hours" => *seconds / 3600,
                "minutes" => *seconds / 60,
                "seconds" => *seconds,
                "minutesofhour" => *seconds / 60 % 60,
                "secondsofminute" => *seconds % 60,
                "secondsofhour" => *seconds % 3600,
                "millisecondsofsecond" => *nanos as i64 / 1_000_000,
                "microsecondsofsecond" => *nanos as i64 / 1000,
                "nanosecondsofsecond" => *nanos as i64,
                "milliseconds" | "microseconds" | "nanoseconds" => {
                    let divisor = match key.to_ascii_lowercase().as_str() {
                        "milliseconds" => 1_000_000,
                        "microseconds" => 1000,
                        _ => 1,
                    };
                    let total = (*seconds as i128 * 1_000_000_000 + *nanos as i128) / divisor;
                    return i64::try_from(total)
                        .map(Value::Int)
                        .unwrap_or_else(|_| Value::BigInt(total.into()));
                }
                _ => return Value::Null,
            };
            return Value::Int(n);
        }
        let date = self.date();
        let time = self.time();
        let n = match key.to_ascii_lowercase().as_str() {
            "year" => date.map(|d| d.year() as i64),
            "month" => date.map(|d| d.month() as i64),
            "day" | "dayofmonth" => date.map(|d| d.day() as i64),
            "ordinalday" | "dayofyear" => date.map(|d| d.ordinal() as i64),
            "week" => date.map(|d| d.iso_week().week() as i64),
            "weekyear" => date.map(|d| d.iso_week().year() as i64),
            "dayofweek" | "weekday" => date.map(|d| d.weekday().number_from_monday() as i64),
            "quarter" => date.map(|d| ((d.month() - 1) / 3 + 1) as i64),
            "dayofquarter" => date.map(|d| {
                (d - NaiveDate::from_ymd_opt(d.year(), ((d.month() - 1) / 3) * 3 + 1, 1).unwrap())
                    .num_days()
                    + 1
            }),
            "hour" => time.map(|t| t.hour() as i64),
            "minute" => time.map(|t| t.minute() as i64),
            "second" => time.map(|t| t.second() as i64),
            "millisecond" => time.map(|t| t.nanosecond() as i64 / 1_000_000),
            "microsecond" => time.map(|t| t.nanosecond() as i64 / 1_000),
            "nanosecond" => time.map(|t| t.nanosecond() as i64),
            "epochseconds" => {
                if let Self::DateTime(d, _) = self {
                    Some(d.timestamp())
                } else {
                    None
                }
            }
            "epochmillis" => {
                if let Self::DateTime(d, _) = self {
                    Some(d.timestamp_millis())
                } else {
                    None
                }
            }
            "offsetminutes" => match self {
                Self::Time(_, o) => Some(*o as i64 / 60),
                Self::DateTime(d, _) => Some(d.offset().local_minus_utc() as i64 / 60),
                _ => None,
            },
            "offsetseconds" => match self {
                Self::Time(_, o) => Some(*o as i64),
                Self::DateTime(d, _) => Some(d.offset().local_minus_utc() as i64),
                _ => None,
            },
            _ => None,
        };
        if let Some(n) = n {
            return Value::Int(n);
        }
        if key.eq_ignore_ascii_case("timezone") {
            return match self {
                Self::DateTime(d, z) => Value::String(
                    z.clone()
                        .unwrap_or_else(|| offset_text(d.offset().local_minus_utc())),
                ),
                Self::Time(_, o) => Value::String(offset_text(*o)),
                _ => Value::Null,
            };
        }
        if key.eq_ignore_ascii_case("offset") {
            return match self {
                Self::DateTime(d, _) => Value::String(offset_text(d.offset().local_minus_utc())),
                Self::Time(_, o) => Value::String(offset_text(*o)),
                _ => Value::Null,
            };
        }
        Value::Null
    }
}
