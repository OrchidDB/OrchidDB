//! ISO temporal parsing and timezone resolution.
use super::{CalendarDate, Result, TemporalValue, constructors::duration};
use crate::ir::Value;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use std::collections::BTreeMap;
fn parse_calendar_date(text: &str) -> Result<CalendarDate> {
    if let Ok(date) = parse_date(text) { return Ok(CalendarDate::from_chrono(date)); }
    let sign = usize::from(text.starts_with(['+', '-']));
    let end = sign + text[sign..].bytes().take_while(u8::is_ascii_digit).count();
    let year: i32 = text[..end].parse().map_err(|_| format!("Invalid date {text}"))?;
    if !(-999_999_999..=999_999_999).contains(&year) { return Err("Year outside calendar range".into()); }
    let representative = 2000 + year.rem_euclid(400);
    let date = parse_date(&format!("{representative:04}{}", &text[end..]))?;
    use chrono::Datelike;
    CalendarDate::new(year + date.year() - representative, date.month(), date.day())
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
pub(super) fn resolve_zone(
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
        return parse_calendar_date(text).map(CalendarDate::date_value);
    }
    if kind == "localtime" {
        return parse_time(text).map(TemporalValue::LocalTime);
    }
    if kind == "localdatetime" {
        let (d, t) = text.split_once('T').unwrap_or((text, "00:00"));
        return Ok(parse_calendar_date(d)?.datetime_value(parse_time(t)?));
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
            let (d, t) = text.split_once('T').unwrap_or((text, "00:00"));
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
        let (dt, name) = if let (Some(zone), Some(offset)) = (zone, offset) {
            let offset = fixed_offset(offset)?;
            let tz: chrono_tz::Tz = zone
                .parse()
                .map_err(|_| format!("Unknown timezone {zone}"))?;
            let candidates = tz.from_local_datetime(&date.and_time(time));
            let chosen = candidates
                .earliest()
                .filter(|d| d.fixed_offset().offset().local_minus_utc() == offset)
                .or_else(|| {
                    candidates
                        .latest()
                        .filter(|d| d.fixed_offset().offset().local_minus_utc() == offset)
                })
                .ok_or("Timezone and offset disagree")?;
            (chosen.fixed_offset(), Some(zone.to_string()))
        } else {
            resolve_zone(date.and_time(time), zone.or(offset).unwrap_or("Z"))?
        };
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
fn parse_duration(text: &str) -> Result<TemporalValue> {
    // ISO alternative representation specifies year/month/day components,
    // rather than a calendar date; do not pass it through date validation.
    if let Some((date, time)) = text.strip_prefix('P').and_then(|s| s.split_once('T')) {
        if date.contains('-') && time.contains(':') {
            let date = date.split('-').collect::<Vec<_>>();
            let time = time.split(':').collect::<Vec<_>>();
            if date.len() == 3 && time.len() == 3 {
                let fields = ["years", "months", "days", "hours", "minutes", "seconds"]
                    .into_iter()
                    .zip(date.into_iter().chain(time))
                    .map(|(key, value)| {
                        value
                            .parse::<bigdecimal::BigDecimal>()
                            .map(|v| (key.to_string(), Value::BigDecimal(v)))
                            .map_err(|_| "Invalid duration component".to_string())
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                return duration(&fields);
            }
        }
    }

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
