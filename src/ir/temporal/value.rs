//! Temporal type identity, ordering, display, and snapshot encoding.
use super::{CalendarDate, Result, parse};
use crate::ir::Value;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use std::{cmp::Ordering, fmt};
#[derive(Debug, Clone, Eq)]
pub enum TemporalValue {
    Date(NaiveDate),
    WideDate(CalendarDate),
    LocalTime(NaiveTime),
    Time(NaiveTime, i32),
    LocalDateTime(NaiveDateTime),
    WideLocalDateTime(CalendarDate, NaiveTime),
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
            (Self::WideDate(a), Self::WideDate(b)) => a == b,
            (Self::WideLocalDateTime(a, at), Self::WideLocalDateTime(b, bt)) => a == b && at == bt,
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

impl TemporalValue {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Date(_) | Self::WideDate(_) => "date",
            Self::LocalTime(_) => "localtime",
            Self::Time(..) => "time",
            Self::LocalDateTime(_) | Self::WideLocalDateTime(..) => "localdatetime",
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
    pub fn calendar_date(&self) -> Option<CalendarDate> {
        match self {
            Self::WideDate(date) | Self::WideLocalDateTime(date, _) => Some(*date),
            _ => self.date().map(CalendarDate::from_chrono),
        }
    }
    pub fn time(&self) -> Option<NaiveTime> {
        match self {
            Self::LocalTime(t) | Self::Time(t, _) => Some(*t),
            Self::LocalDateTime(d) => Some(d.time()),
            Self::WideLocalDateTime(_, t) => Some(*t),
            Self::DateTime(d, _) => Some(d.time()),
            _ => None,
        }
    }
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        if self.kind() == other.kind() && matches!(self.kind(), "date" | "localdatetime") {
            return Some(self.calendar_date()?.cmp(&other.calendar_date()?).then_with(|| self.time().cmp(&other.time())));
        }
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
}
impl TemporalValue {
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
pub(super) fn offset_text(offset: i32) -> String {
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
            Self::WideDate(d) => write!(f, "{d}"),
            Self::WideLocalDateTime(d, t) => write!(f, "{d}T{}", time_text(*t)),
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
