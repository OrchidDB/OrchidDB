//! Typed openCypher temporal values. Calendar values and elapsed durations retain
//! separate identities; the legacy Gremlin DateTime value is unchanged.
use super::Value;
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone,
    Timelike,
};
use std::{cmp::Ordering, collections::BTreeMap, fmt};

#[derive(Debug, Clone, Eq)]
pub enum TemporalValue {
    Date(NaiveDate),
    LocalTime(NaiveTime),
    Time(NaiveTime, i32),
    LocalDateTime(NaiveDateTime),
    DateTime(DateTime<FixedOffset>, Option<String>),
    Duration {
        months: i64,
        days: i64,
        seconds: i64,
        nanos: i32,
    },
}

impl PartialEq for TemporalValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Date(a), Self::Date(b)) => a == b,
            (Self::LocalTime(a), Self::LocalTime(b)) => a == b,
            (Self::Time(a, ao), Self::Time(b, bo)) => a == b && ao == bo,
            (Self::LocalDateTime(a), Self::LocalDateTime(b)) => a == b,
            (Self::DateTime(a, az), Self::DateTime(b, bz)) => {
                a == b && a.offset() == b.offset() && az == bz
            }
            (
                Self::Duration {
                    months: a,
                    days: b,
                    seconds: c,
                    nanos: d,
                },
                Self::Duration {
                    months: e,
                    days: f,
                    seconds: g,
                    nanos: h,
                },
            ) => (a, b, c, d) == (e, f, g, h),
            _ => false,
        }
    }
}

type Result<T> = std::result::Result<T, String>;
impl TemporalValue {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Date(_) => "date",
            Self::LocalTime(_) => "localtime",
            Self::Time(..) => "time",
            Self::LocalDateTime(_) => "localdatetime",
            Self::DateTime(..) => "datetime",
            Self::Duration { .. } => "duration",
        }
    }
    pub fn date(&self) -> Option<NaiveDate> {
        match self {
            Self::Date(d) => Some(*d),
            Self::LocalDateTime(d) => Some(d.date()),
            Self::DateTime(d, _) => Some(d.date_naive()),
            _ => None,
        }
    }
    pub fn time(&self) -> Option<NaiveTime> {
        match self {
            Self::LocalTime(t) | Self::Time(t, _) => Some(*t),
            Self::LocalDateTime(d) => Some(d.time()),
            Self::DateTime(d, _) => Some(d.time()),
            _ => None,
        }
    }
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Date(a), Self::Date(b)) => Some(a.cmp(b)),
            (Self::LocalTime(a), Self::LocalTime(b)) => Some(a.cmp(b)),
            (Self::LocalDateTime(a), Self::LocalDateTime(b)) => Some(a.cmp(b)),
            (Self::DateTime(a, az), Self::DateTime(b, bz)) => Some(
                a.cmp(b)
                    .then_with(|| {
                        a.offset()
                            .local_minus_utc()
                            .cmp(&b.offset().local_minus_utc())
                    })
                    .then_with(|| az.cmp(bz)),
            ),
            (Self::Time(a, ao), Self::Time(b, bo)) => {
                let key = |t: &NaiveTime, offset: i32| {
                    (
                        t.num_seconds_from_midnight() as i64 - offset as i64,
                        t.nanosecond(),
                    )
                };
                Some(key(a, *ao).cmp(&key(b, *bo)).then_with(|| ao.cmp(bo)))
            }
            _ => None,
        }
    }
    pub fn component(&self, key: &str) -> Value {
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
            "dayofweek" => date.map(|d| d.weekday().number_from_monday() as i64),
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
    /// Stable snapshot payload, independent of display formatting.
    pub fn encode(&self) -> String {
        match self {
            Self::Duration {
                months,
                days,
                seconds,
                nanos,
            } => format!("duration|{months}|{days}|{seconds}|{nanos}"),
            _ => format!("{}|{}", self.kind(), self),
        }
    }
    pub fn decode(text: &str) -> Result<Self> {
        let (kind, value) = text.split_once('|').ok_or("Invalid temporal payload")?;
        if kind == "duration" {
            let fields = value
                .split('|')
                .map(str::parse::<i64>)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let [months, days, seconds, nanos] = fields.as_slice() else {
                return Err("Invalid duration payload".into());
            };
            return Ok(Self::Duration {
                months: *months,
                days: *days,
                seconds: *seconds,
                nanos: i32::try_from(*nanos).map_err(|e| e.to_string())?,
            });
        }
        parse(kind, value)
    }
}
fn time_text(t: NaiveTime) -> String {
    let mut text = format!("{:02}:{:02}", t.hour(), t.minute());
    if t.second() != 0 || t.nanosecond() != 0 {
        text.push_str(&format!(":{:02}", t.second()));
    }
    if t.nanosecond() != 0 {
        text.push_str(format!(".{:09}", t.nanosecond()).trim_end_matches('0'));
    }
    text
}
fn offset_text(offset: i32) -> String {
    if offset == 0 {
        return "Z".into();
    }
    let sign = if offset < 0 { '-' } else { '+' };
    let n = offset.unsigned_abs();
    if n % 60 == 0 {
        format!("{sign}{:02}:{:02}", n / 3600, n % 3600 / 60)
    } else {
        format!("{sign}{:02}:{:02}:{:02}", n / 3600, n % 3600 / 60, n % 60)
    }
}
impl fmt::Display for TemporalValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Date(d) => write!(f, "{d}"),
            Self::LocalTime(t) => write!(f, "{}", time_text(*t)),
            Self::Time(t, o) => write!(f, "{}{}", time_text(*t), offset_text(*o)),
            Self::LocalDateTime(d) => write!(f, "{}T{}", d.date(), time_text(d.time())),
            Self::DateTime(d, z) => {
                write!(
                    f,
                    "{}T{}{}",
                    d.date_naive(),
                    time_text(d.time()),
                    offset_text(d.offset().local_minus_utc())
                )?;
                if let Some(z) = z {
                    write!(f, "[{z}]")?;
                }
                Ok(())
            }
            Self::Duration {
                months,
                days,
                seconds,
                nanos,
            } => {
                let total = *seconds as i128 * 1_000_000_000 + *nanos as i128;
                let seconds = (total / 1_000_000_000) as i64;
                let nanos = (total % 1_000_000_000) as i32;
                let seconds = &seconds;
                let nanos = &nanos;
                write!(f, "P")?;
                if *months != 0 {
                    if months / 12 != 0 {
                        write!(f, "{}Y", months / 12)?;
                    }
                    if months % 12 != 0 {
                        write!(f, "{}M", months % 12)?;
                    }
                }
                if *days != 0 {
                    write!(f, "{days}D")?;
                }
                if *seconds != 0 || *nanos != 0 || (*months == 0 && *days == 0) {
                    write!(f, "T")?;
                    let hours = seconds / 3600;
                    let minutes = seconds % 3600 / 60;
                    let sec = seconds % 60;
                    if hours != 0 {
                        write!(f, "{hours}H")?;
                    }
                    if minutes != 0 {
                        write!(f, "{minutes}M")?;
                    }
                    if sec != 0 || *nanos != 0 || (*seconds == 0) {
                        if *nanos == 0 {
                            write!(f, "{sec}S")?;
                        } else {
                            let negative = sec < 0 || *nanos < 0;
                            write!(
                                f,
                                "{}{}{}S",
                                if negative { "-" } else { "" },
                                sec.unsigned_abs(),
                                format!(".{:09}", nanos.unsigned_abs()).trim_end_matches('0')
                            )?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
}
fn parse_date(text: &str) -> Result<NaiveDate> {
    let compact = text.bytes().all(|b| b.is_ascii_digit());
    if compact {
        let (pattern, expanded) = match text.len() {
            4 => ("%Y-%m-%d", format!("{text}-01-01")),
            6 => ("%Y-%m-%d", format!("{}-{}-01", &text[..4], &text[4..])),
            7 => ("%Y%j", text.into()),
            8 => ("%Y%m%d", text.into()),
            _ => return Err(format!("Invalid date {text}")),
        };
        return NaiveDate::parse_from_str(&expanded, pattern)
            .map_err(|_| format!("Invalid date {text}"));
    }
    if let Some((year, week)) = text.split_once('W') {
        let year = year.trim_end_matches('-');
        let week = week.replace('-', "");
        if !week.is_ascii() || !matches!(week.len(), 2 | 3) {
            return Err(format!("Invalid week date {text}"));
        }
        let day = if week.len() == 3 { &week[2..] } else { "1" };
        return NaiveDate::parse_from_str(&format!("{year}-W{}-{day}", &week[..2]), "%G-W%V-%u")
            .map_err(|_| format!("Invalid week date {text}"));
    }
    // Pick the grammar before parsing: chrono accepts variable digit widths
    // that otherwise make YYYYMM ambiguous with an ordinal date.
    let parts = text
        .trim_start_matches(['+', '-'])
        .split('-')
        .collect::<Vec<_>>();
    let (pattern, expanded) = match parts.as_slice() {
        [_, month] if month.len() == 2 => ("%Y-%m-%d", format!("{text}-01")),
        [_, ordinal] if ordinal.len() == 3 => ("%Y-%j", text.to_string()),
        [_, _, _] => ("%Y-%m-%d", text.to_string()),
        _ => return Err(format!("Invalid date {text}")),
    };
    NaiveDate::parse_from_str(&expanded, pattern).map_err(|_| format!("Invalid date {text}"))
}
fn parse_time(text: &str) -> Result<NaiveTime> {
    let text = text.trim_start_matches('T');
    for pattern in ["%H:%M:%S%.f", "%H:%M", "%H%M%S%.f", "%H%M"] {
        if let Ok(t) = NaiveTime::parse_from_str(text, pattern) {
            return Ok(t);
        }
    }
    if let Ok(hour) = text.parse() {
        if let Some(t) = NaiveTime::from_hms_opt(hour, 0, 0) {
            return Ok(t);
        }
    }
    Err(format!("Invalid time {text}"))
}
fn fixed_offset(text: &str) -> Result<i32> {
    if matches!(text, "Z" | "UTC" | "GMT") {
        return Ok(0);
    }
    let sign = if text.starts_with('-') {
        -1
    } else if text.starts_with('+') {
        1
    } else {
        return Err("Expected timezone offset".into());
    };
    let digits = text[1..].replace(':', "");
    if !matches!(digits.len(), 2 | 4 | 6) || !digits.is_ascii() {
        return Err("Invalid timezone offset".into());
    }
    let h: i32 = digits[..2].parse().map_err(|_| "Invalid timezone hour")?;
    let m: i32 = if digits.len() >= 4 {
        digits[2..4]
            .parse()
            .map_err(|_| "Invalid timezone minute")?
    } else {
        0
    };
    let s: i32 = if digits.len() == 6 {
        digits[4..].parse().map_err(|_| "Invalid timezone second")?
    } else {
        0
    };
    if h > 18 || m > 59 || s > 59 || (h == 18 && (m != 0 || s != 0)) {
        return Err("Timezone offset out of range".into());
    }
    Ok(sign * (h * 3600 + m * 60 + s))
}
fn resolve_zone(
    local: NaiveDateTime,
    zone: &str,
) -> Result<(DateTime<FixedOffset>, Option<String>)> {
    if let Ok(offset) = fixed_offset(zone) {
        return FixedOffset::east_opt(offset)
            .and_then(|o| o.from_local_datetime(&local).single())
            .map(|d| (d, None))
            .ok_or("Invalid zoned datetime".into());
    }
    let tz: chrono_tz::Tz = zone
        .parse()
        .map_err(|_| format!("Unknown timezone {zone}"))?;
    let dt = tz
        .from_local_datetime(&local)
        .earliest()
        .ok_or("Local datetime falls in a timezone gap")?;
    Ok((dt.fixed_offset(), Some(zone.to_string())))
}
pub fn parse(kind: &str, text: &str) -> Result<TemporalValue> {
    if kind == "duration" {
        return parse_duration(text);
    }
    if kind == "date" {
        return parse_date(text).map(TemporalValue::Date);
    }
    if kind == "localtime" {
        return parse_time(text).map(TemporalValue::LocalTime);
    }
    if kind == "localdatetime" {
        let (d, t) = text
            .split_once('T')
            .ok_or("Datetime requires T separator")?;
        return Ok(TemporalValue::LocalDateTime(
            parse_date(d)?.and_time(parse_time(t)?),
        ));
    }
    if matches!(kind, "time" | "datetime") {
        let (text, zone) = if let Some((a, b)) = text.split_once('[') {
            (
                a,
                Some(b.strip_suffix(']').ok_or("Invalid timezone suffix")?),
            )
        } else {
            (text, None)
        };
        let (date, clock) = if kind == "datetime" {
            let (d, t) = text
                .split_once('T')
                .ok_or("Datetime requires T separator")?;
            (parse_date(d)?, t)
        } else {
            (NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(), text)
        };
        let offset_at = clock
            .char_indices()
            .find(|(_, c)| matches!(c, 'Z' | '+' | '-'))
            .map(|(i, _)| i);
        let (clock, offset) = offset_at
            .map(|i| (&clock[..i], Some(&clock[i..])))
            .unwrap_or((clock, None));
        let time = parse_time(clock)?;
        let (dt, name) = if let (Some(zone),Some(offset)) = (zone,offset) {
            let offset=fixed_offset(offset)?;
            let tz: chrono_tz::Tz=zone.parse().map_err(|_|format!("Unknown timezone {zone}"))?;
            let candidates=tz.from_local_datetime(&date.and_time(time));
            let chosen=candidates.earliest().filter(|d|d.fixed_offset().offset().local_minus_utc()==offset)
                .or_else(||candidates.latest().filter(|d|d.fixed_offset().offset().local_minus_utc()==offset))
                .ok_or("Timezone and offset disagree")?;
            (chosen.fixed_offset(),Some(zone.to_string()))
        } else { resolve_zone(date.and_time(time), zone.or(offset).unwrap_or("Z"))? };
        if let Some(offset) = offset {
            if fixed_offset(offset)? != dt.offset().local_minus_utc() {
                return Err("Timezone and offset disagree".into());
            }
        }
        return Ok(if kind == "time" {
            TemporalValue::Time(time, dt.offset().local_minus_utc())
        } else {
            TemporalValue::DateTime(dt, name)
        });
    }
    Err(format!("Unknown temporal type {kind}"))
}
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
            key.as_str() != super::value::STRUCT_ORDER_KEY
                && key.as_str() != super::value::STRUCT_TYPES_KEY
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
fn duration(fields: &BTreeMap<String, Value>) -> Result<TemporalValue> {
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

fn parse_duration(text: &str) -> Result<TemporalValue> {
    let pattern=regex::Regex::new(r"^([+-])?P(?:(-?\d+(?:[.,]\d+)?)Y)?(?:(-?\d+(?:[.,]\d+)?)M)?(?:(-?\d+(?:[.,]\d+)?)W)?(?:(-?\d+(?:[.,]\d+)?)D)?(?:T(?:(-?\d+(?:[.,]\d+)?)H)?(?:(-?\d+(?:[.,]\d+)?)M)?(?:(-?\d+(?:[.,]\d+)?)S)?)?$").unwrap();
    let found = pattern
        .captures(text)
        .ok_or_else(|| format!("Invalid duration {text}"))?;
    let sign = if found.get(1).is_some_and(|m| m.as_str() == "-") {
        -1
    } else {
        1
    };
    let mut fields = BTreeMap::new();
    for (index, key) in [
        "years", "months", "weeks", "days", "hours", "minutes", "seconds",
    ]
    .iter()
    .enumerate()
    {
        if let Some(value) = found.get(index + 2) {
            fields.insert(
                (*key).into(),
                Value::BigDecimal(
                    bigdecimal::BigDecimal::from(sign)
                        * value
                            .as_str()
                            .replace(',', ".")
                            .parse::<bigdecimal::BigDecimal>()
                            .map_err(|_| "Invalid duration component")?,
                ),
            );
        }
    }
    if fields.is_empty() {
        return Err("Duration requires a component".into());
    }
    duration(&fields)
}

/// Calendar months are applied before days, and elapsed seconds last. In a
/// named timezone this preserves wall-clock days across daylight-saving changes.
fn shift(
    value: &TemporalValue,
    months: i64,
    days: i64,
    seconds: i64,
    nanos: i32,
) -> Result<TemporalValue> {
    let calendar = |date: NaiveDate| -> Result<NaiveDate> {
        let count = u32::try_from(months.unsigned_abs()).map_err(|_| "Month shift out of range")?;
        let shifted = if months >= 0 {
            date.checked_add_months(chrono::Months::new(count))
        } else {
            date.checked_sub_months(chrono::Months::new(count))
        };
        shifted
            .and_then(|d| d.checked_add_signed(Duration::try_days(days)?))
            .ok_or("Calendar shift out of range".into())
    };
    let elapsed = Duration::try_seconds(seconds)
        .and_then(|s| s.checked_add(&Duration::nanoseconds(nanos as i64)))
        .ok_or("Elapsed duration out of range")?;
    Ok(match value {
        TemporalValue::Date(d) => TemporalValue::Date(calendar(*d)?),
        TemporalValue::LocalTime(t) => {
            TemporalValue::LocalTime(t.overflowing_add_signed(elapsed).0)
        }
        TemporalValue::Time(t, offset) => {
            TemporalValue::Time(t.overflowing_add_signed(elapsed).0, *offset)
        }
        TemporalValue::LocalDateTime(d) => TemporalValue::LocalDateTime(
            calendar(d.date())?
                .and_time(d.time())
                .checked_add_signed(elapsed)
                .ok_or("Datetime shift out of range")?,
        ),
        TemporalValue::DateTime(d, zone) => {
            let local = calendar(d.date_naive())?.and_time(d.time());
            let (base, _) = resolve_zone(
                local,
                &zone
                    .clone()
                    .unwrap_or_else(|| offset_text(d.offset().local_minus_utc())),
            )?;
            let result = base
                .checked_add_signed(elapsed)
                .ok_or("Datetime shift out of range")?;
            let result = if let Some(zone) = zone {
                let tz: chrono_tz::Tz = zone.parse().map_err(|_| "Invalid timezone")?;
                result.with_timezone(&tz).fixed_offset()
            } else {
                result
            };
            TemporalValue::DateTime(result, zone.clone())
        }
        TemporalValue::Duration { .. } => return Err("Expected a calendar value".into()),
    })
}

fn normalized_duration(months: i64, days: i64, total_nanos: i128) -> Result<TemporalValue> {
    Ok(TemporalValue::Duration {
        months,
        days,
        seconds: i64::try_from(total_nanos.div_euclid(1_000_000_000))
            .map_err(|_| "Duration overflow")?,
        nanos: total_nanos.rem_euclid(1_000_000_000) as i32,
    })
}

pub fn arithmetic(op: super::expr::BinaryOp, left: &Value, right: &Value) -> Result<Value> {
    use super::expr::BinaryOp;
    use TemporalValue::Duration as D;
    if matches!(left, Value::Null) || matches!(right, Value::Null) {
        return Ok(Value::Null);
    }
    let sign = if matches!(op, BinaryOp::Sub) {
        -1i64
    } else {
        1
    };
    match (left, right) {
        (
            Value::Temporal(D {
                months: a,
                days: b,
                seconds: c,
                nanos: d,
            }),
            Value::Temporal(D {
                months: e,
                days: f,
                seconds: g,
                nanos: h,
            }),
        ) if matches!(op, BinaryOp::Add | BinaryOp::Sub) => {
            let add = |x: i64, y: i64| {
                y.checked_mul(sign)
                    .and_then(|y| x.checked_add(y))
                    .ok_or("Duration overflow")
            };
            normalized_duration(
                add(*a, *e)?,
                add(*b, *f)?,
                (*c as i128 + sign as i128 * *g as i128) * 1_000_000_000
                    + *d as i128
                    + sign as i128 * *h as i128,
            )
            .map(Value::Temporal)
        }
        (
            Value::Temporal(value),
            Value::Temporal(D {
                months,
                days,
                seconds,
                nanos,
            }),
        ) if matches!(op, BinaryOp::Add | BinaryOp::Sub) => {
            let signed = |v: i64| v.checked_mul(sign).ok_or("Duration overflow");
            shift(
                value,
                signed(*months)?,
                signed(*days)?,
                signed(*seconds)?,
                *nanos * sign as i32,
            )
            .map(Value::Temporal)
        }
        (Value::Temporal(D { .. }), Value::Temporal(_)) if matches!(op, BinaryOp::Add) => {
            arithmetic(op, right, left)
        }
        (
            Value::Temporal(D {
                months,
                days,
                seconds,
                nanos,
            }),
            scalar,
        ) if matches!(op, BinaryOp::Mul | BinaryOp::Div) => {
            use bigdecimal::BigDecimal;
            use num_traits::ToPrimitive;
            let factor = match scalar {
                Value::Int(v) | Value::Long(v) => BigDecimal::from(*v),
                Value::Float(v) if v.is_finite() => v
                    .to_string()
                    .parse()
                    .map_err(|_| "Invalid duration factor")?,
                Value::BigDecimal(v) => v.clone(),
                _ => return Err("Duration scale requires a number".into()),
            };
            if matches!(op, BinaryOp::Div) && factor == BigDecimal::from(0) {
                return Err("Cannot divide duration by zero".into());
            }
            let scale = |v: BigDecimal| {
                if matches!(op, BinaryOp::Div) {
                    v / &factor
                } else {
                    v * &factor
                }
            };
            let m = scale(BigDecimal::from(*months));
            let whole_m = m.to_i64().ok_or("Duration months overflow")?;
            let calendar_seconds = scale(BigDecimal::from(*days)) * BigDecimal::from(86400)
                + (&m - BigDecimal::from(whole_m)) * BigDecimal::from(2_629_746);
            let whole_d = (&calendar_seconds / BigDecimal::from(86400))
                .to_i64()
                .ok_or("Duration days overflow")?;
            let ns = scale(BigDecimal::from(
                *seconds as i128 * 1_000_000_000 + *nanos as i128,
            )) + (calendar_seconds - BigDecimal::from(whole_d) * BigDecimal::from(86400))
                * BigDecimal::from(1_000_000_000);
            normalized_duration(whole_m, whole_d, ns.to_i128().ok_or("Duration overflow")?)
                .map(Value::Temporal)
        }
        (_, Value::Temporal(D { .. })) if matches!(op, BinaryOp::Mul) => {
            arithmetic(op, right, left)
        }
        _ => Err("Invalid temporal arithmetic operands".into()),
    }
}

pub fn between(unit: &str, left: &Value, right: &Value) -> Result<Value> {
    if matches!(left, Value::Null) || matches!(right, Value::Null) {
        return Ok(Value::Null);
    }
    let (Value::Temporal(left), Value::Temporal(right)) = (left, right) else {
        return Err("Duration difference requires temporal operands".into());
    };
    if matches!(left, TemporalValue::Duration { .. })
        || matches!(right, TemporalValue::Duration { .. })
    {
        return Err("Duration difference requires calendar values".into());
    }
    let fallback = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    // A missing date inherits the other operand's date; a missing time is midnight.
    let ld = left.date().or_else(|| right.date()).unwrap_or(fallback);
    let rd = right.date().or_else(|| left.date()).unwrap_or(fallback);
    let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let lt = left.time().unwrap_or(midnight);
    let rt = right.time().unwrap_or(midnight);
    let both_dates = left.date().is_some() && right.date().is_some();
    let offset = |value: &TemporalValue| match value {
        TemporalValue::Time(_, o) => Some(*o),
        TemporalValue::DateTime(d, _) => Some(d.offset().local_minus_utc()),
        _ => None,
    };
    let offset_delta = match (offset(left), offset(right)) {
        (Some(a), Some(b)) => (a as i64 - b as i64) * 1_000_000_000,
        _ => 0,
    };
    let delta_nanos = |a: NaiveDateTime, b: NaiveDateTime| {
        let d = b - a;
        d.num_seconds() as i128 * 1_000_000_000 + d.subsec_nanos() as i128
    };
    if unit == "inseconds" {
        return normalized_duration(
            0,
            0,
            delta_nanos(ld.and_time(lt), rd.and_time(rt)) + offset_delta as i128,
        )
        .map(Value::Temporal);
    }
    if unit == "indays" {
        return normalized_duration(0, if both_dates { (rd - ld).num_days() } else { 0 }, 0)
            .map(Value::Temporal);
    }
    let mut months = if both_dates {
        (rd.year() as i64 - ld.year() as i64) * 12 + rd.month() as i64 - ld.month() as i64
    } else {
        0
    };
    let shifted_date = |months| -> Result<NaiveDate> {
        shift(&TemporalValue::Date(ld), months, 0, 0, 0)?
            .date()
            .ok_or("Date required".into())
    };
    let mut cursor = shifted_date(months)?;
    if (months > 0 && cursor > rd) || (months < 0 && cursor < rd) {
        months -= months.signum();
        cursor = shifted_date(months)?;
    }
    if unit == "inmonths" {
        return normalized_duration(months, 0, 0).map(Value::Temporal);
    }
    if unit != "between" {
        return Err("Unknown duration difference unit".into());
    }
    // Whole calendar days/months must not overshoot the end's clock time.
    if (months > 0 && cursor.and_time(lt) > rd.and_time(rt))
        || (months < 0 && cursor.and_time(lt) < rd.and_time(rt))
    {
        months -= months.signum();
        cursor = shifted_date(months)?;
    }
    let mut days = if both_dates {
        (rd - cursor).num_days()
    } else {
        0
    };
    let time_nanos = delta_nanos(cursor.and_time(lt), cursor.and_time(rt));
    if (days > 0 && time_nanos < 0) || (days < 0 && time_nanos > 0) {
        days -= days.signum();
    }
    let after_days = cursor
        .checked_add_signed(Duration::try_days(days).ok_or("Duration overflow")?)
        .ok_or("Date overflow")?;
    let mut remaining =
        delta_nanos(after_days.and_time(lt), rd.and_time(rt)) + offset_delta as i128;
    // Recompute the source offset after calendar shifts in a named zone.
    if let (TemporalValue::DateTime(original, Some(zone)), Some(_)) = (left, offset(right)) {
        let (shifted, _) = resolve_zone(after_days.and_time(lt), zone)?;
        remaining += (shifted.offset().local_minus_utc() as i128
            - original.offset().local_minus_utc() as i128)
            * 1_000_000_000;
    }
    normalized_duration(months, days, remaining).map(Value::Temporal)
}

/// SQL string materialization would erase calendar type identity, including in
/// containers. Keep these values in typed residual batches until native SQL
/// temporal transport is available.
pub(crate) fn contains_temporal(value: &Value) -> bool {
    match value {
        Value::Temporal(_) => true,
        Value::List(items) | Value::Set(items) | Value::BulkSet(items) | Value::Path(items) => {
            items.iter().any(contains_temporal)
        }
        Value::Map(items) => items.values().any(contains_temporal),
        Value::TypedMap(items) => items
            .iter()
            .any(|(k, v)| contains_temporal(k) || contains_temporal(v)),
        Value::MapEntry(pair) => contains_temporal(&pair.0) || contains_temporal(&pair.1),
        Value::Property { value, .. }
        | Value::VertexProperty { value, .. }
        | Value::CardinalityValue { value, .. } => contains_temporal(value),
        _ => false,
    }
}
