use num_traits::ToPrimitive;

use crate::ir::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TemporalParts {
    pub(crate) year: i64,
    pub(crate) month: u32,
    pub(crate) day: u32,
    pub(crate) hour: i64,
    pub(crate) minute: i64,
    pub(crate) second: i64,
    pub(crate) micros: i64,
    pub(crate) has_time: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct IntervalParts {
    pub(crate) months: i64,
    pub(crate) days: i64,
    pub(crate) micros: i128,
}

pub(crate) fn parse_temporal(raw: &str) -> Option<TemporalParts> {
    let inner = raw
        .trim()
        .strip_prefix("dt[")
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(raw.trim());
    let (date, time, has_time) = if let Some((date, time)) = inner.split_once('T') {
        (date, time, true)
    } else if let Some((date, time)) = inner.split_once(' ') {
        if date.eq_ignore_ascii_case("(BC)") {
            return None;
        }
        (date, time, true)
    } else {
        (inner, "00:00:00", false)
    };

    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse().ok()?;
    let month = date_parts.next()?.parse().ok()?;
    let day = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !valid_date(year, month, day) {
        return None;
    }

    let time = strip_timezone(time);
    let mut time_parts = time.split(':');
    let hour = time_parts.next().unwrap_or("0").parse().ok()?;
    let minute = time_parts.next().unwrap_or("0").parse().ok()?;
    let second_part = time_parts.next().unwrap_or("0");
    if time_parts.next().is_some() {
        return None;
    }
    let (second_text, fraction) = second_part.split_once('.').unwrap_or((second_part, ""));
    let second = second_text.parse().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=59).contains(&second) {
        return None;
    }
    let micros = parse_fraction_micros(fraction)?;

    Some(TemporalParts {
        year,
        month,
        day,
        hour,
        minute,
        second,
        micros,
        has_time,
    })
}

pub(crate) fn parse_interval(raw: &str) -> Option<IntervalParts> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut interval = IntervalParts::default();
    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    let mut i = 0;
    while i < parts.len() {
        if let Some(time) = parse_interval_time(parts[i]) {
            interval.micros = interval.micros.checked_add(time)?;
            i += 1;
            continue;
        }
        let (value, unit, consumed) = if let Ok(value) = parts[i].parse::<i64>() {
            (value, parts.get(i + 1)?.to_ascii_lowercase(), 2)
        } else {
            let (value, unit) = split_compact_interval_part(parts[i])?;
            (value, unit, 1)
        };
        match unit.as_str() {
            "year" | "years" | "y" | "yr" | "yrs" => {
                interval.months = interval.months.checked_add(value.checked_mul(12)?)?;
            }
            "month" | "months" | "mon" | "mons" => {
                interval.months = interval.months.checked_add(value)?;
            }
            "week" | "weeks" => {
                interval.days = interval.days.checked_add(value.checked_mul(7)?)?;
            }
            "day" | "days" | "d" => {
                interval.days = interval.days.checked_add(value)?;
            }
            "hour" | "hours" | "h" | "hr" | "hrs" => {
                interval.micros = interval
                    .micros
                    .checked_add((value as i128).checked_mul(3_600_000_000)?)?;
            }
            "minute" | "minutes" | "m" | "min" | "mins" => {
                interval.micros = interval
                    .micros
                    .checked_add((value as i128).checked_mul(60_000_000)?)?;
            }
            "second" | "seconds" | "s" | "sec" | "secs" => {
                interval.micros = interval
                    .micros
                    .checked_add((value as i128).checked_mul(1_000_000)?)?;
            }
            "millisecond" | "milliseconds" | "ms" => {
                interval.micros = interval
                    .micros
                    .checked_add((value as i128).checked_mul(1_000)?)?;
            }
            "microsecond" | "microseconds" | "us" | "µs" => {
                interval.micros = interval.micros.checked_add(value as i128)?;
            }
            _ => return None,
        }
        i += consumed;
    }

    Some(interval)
}

pub(crate) fn format_interval(interval: IntervalParts) -> String {
    let mut parts = Vec::new();
    let years = interval.months / 12;
    let months = interval.months % 12;
    if years != 0 {
        parts.push(format!(
            "{years} {}",
            if years.abs() == 1 { "year" } else { "years" }
        ));
    }
    if months != 0 {
        parts.push(format!(
            "{months} {}",
            if months.abs() == 1 { "month" } else { "months" }
        ));
    }
    if interval.days != 0 {
        parts.push(format!(
            "{} {}",
            interval.days,
            if interval.days.abs() == 1 {
                "day"
            } else {
                "days"
            }
        ));
    }
    if interval.micros != 0 || parts.is_empty() {
        parts.push(format_interval_time(interval.micros));
    }
    parts.join(" ")
}

pub(crate) fn temporal_part(unit: &str, value: &str) -> Option<Value> {
    if let Some(interval) = parse_interval(value) {
        return interval_part(unit, interval).map(Value::Long);
    }
    let temporal = parse_temporal(value)?;
    let value = match normalized_unit(unit).as_str() {
        "year" => temporal.year,
        "month" => temporal.month as i64,
        "day" => temporal.day as i64,
        "hour" => temporal.hour,
        "minute" => temporal.minute,
        "second" => temporal.second,
        "millisecond" => temporal.second * 1_000 + temporal.micros / 1_000,
        "microsecond" => temporal.second * 1_000_000 + temporal.micros,
        "decade" => temporal.year.div_euclid(10),
        "century" => (temporal.year - 1).div_euclid(100) + 1,
        "millennium" => (temporal.year - 1).div_euclid(1000) + 1,
        "quarter" => ((temporal.month - 1) / 3 + 1) as i64,
        _ => return None,
    };
    Some(Value::Long(value))
}

pub(crate) fn trunc_temporal(unit: &str, value: &str) -> Option<String> {
    let mut temporal = parse_temporal(value)?;
    match normalized_unit(unit).as_str() {
        "millennium" => {
            temporal.year = temporal.year.div_euclid(1000) * 1000;
            temporal.month = 1;
            temporal.day = 1;
            zero_time(&mut temporal);
        }
        "century" => {
            temporal.year = temporal.year.div_euclid(100) * 100;
            temporal.month = 1;
            temporal.day = 1;
            zero_time(&mut temporal);
        }
        "decade" => {
            temporal.year = temporal.year.div_euclid(10) * 10;
            temporal.month = 1;
            temporal.day = 1;
            zero_time(&mut temporal);
        }
        "year" => {
            temporal.month = 1;
            temporal.day = 1;
            zero_time(&mut temporal);
        }
        "quarter" => {
            temporal.month = ((temporal.month - 1) / 3) * 3 + 1;
            temporal.day = 1;
            zero_time(&mut temporal);
        }
        "month" => {
            temporal.day = 1;
            zero_time(&mut temporal);
        }
        "day" => zero_time(&mut temporal),
        "hour" => {
            temporal.minute = 0;
            temporal.second = 0;
            temporal.micros = 0;
        }
        "minute" => {
            temporal.second = 0;
            temporal.micros = 0;
        }
        "second" => temporal.micros = 0,
        "millisecond" => temporal.micros = temporal.micros / 1_000 * 1_000,
        "microsecond" => {}
        _ => return None,
    }
    Some(format_temporal(temporal, temporal.has_time))
}

pub(crate) fn last_day(value: &str) -> Option<String> {
    let mut temporal = parse_temporal(value)?;
    temporal.day = days_in_month(temporal.year, temporal.month);
    temporal.has_time = false;
    zero_time(&mut temporal);
    Some(format_temporal(temporal, temporal.has_time))
}

pub(crate) fn day_name(value: &str) -> Option<&'static str> {
    const NAMES: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    let temporal = parse_temporal(value)?;
    let days = days_from_civil(temporal.year, temporal.month, temporal.day)?;
    let idx = (days + 4).rem_euclid(7) as usize;
    Some(NAMES[idx])
}

pub(crate) fn month_name(value: &str) -> Option<&'static str> {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let temporal = parse_temporal(value)?;
    Some(NAMES[(temporal.month - 1) as usize])
}

pub(crate) fn make_date(year: i64, month: i64, day: i64) -> Option<String> {
    let month = u32::try_from(month).ok()?;
    let day = u32::try_from(day).ok()?;
    if !valid_date(year, month, day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

pub(crate) fn numeric_to_interval(value: &Value, unit: &str) -> Option<String> {
    let n = value.as_i64()?;
    let interval = match unit {
        "years" => IntervalParts {
            months: n.checked_mul(12)?,
            days: 0,
            micros: 0,
        },
        "months" => IntervalParts {
            months: n,
            days: 0,
            micros: 0,
        },
        "days" => IntervalParts {
            months: 0,
            days: n,
            micros: 0,
        },
        "hours" => IntervalParts {
            months: 0,
            days: 0,
            micros: (n as i128).checked_mul(3_600_000_000)?,
        },
        "minutes" => IntervalParts {
            months: 0,
            days: 0,
            micros: (n as i128).checked_mul(60_000_000)?,
        },
        "seconds" => IntervalParts {
            months: 0,
            days: 0,
            micros: (n as i128).checked_mul(1_000_000)?,
        },
        "milliseconds" => IntervalParts {
            months: 0,
            days: 0,
            micros: (n as i128).checked_mul(1_000)?,
        },
        "microseconds" => IntervalParts {
            months: 0,
            days: 0,
            micros: n as i128,
        },
        _ => return None,
    };
    Some(format_interval(interval))
}

pub(crate) fn add_intervals(left: IntervalParts, right: IntervalParts) -> Option<IntervalParts> {
    Some(IntervalParts {
        months: left.months.checked_add(right.months)?,
        days: left.days.checked_add(right.days)?,
        micros: left.micros.checked_add(right.micros)?,
    })
}

pub(crate) fn subtract_intervals(
    left: IntervalParts,
    right: IntervalParts,
) -> Option<IntervalParts> {
    Some(IntervalParts {
        months: left.months.checked_sub(right.months)?,
        days: left.days.checked_sub(right.days)?,
        micros: left.micros.checked_sub(right.micros)?,
    })
}

pub(crate) fn divide_interval(interval: IntervalParts, divisor: i64) -> Option<IntervalParts> {
    if divisor == 0 {
        return None;
    }
    let months = interval.months / divisor;
    let month_remainder = interval.months % divisor;
    let days_with_month_remainder = interval
        .days
        .checked_add(month_remainder.checked_mul(30)?)?;
    let days = days_with_month_remainder / divisor;
    let day_remainder = days_with_month_remainder % divisor;
    let micros_with_day_remainder = interval
        .micros
        .checked_add((day_remainder as i128).checked_mul(86_400_000_000)?)?;
    Some(IntervalParts {
        months,
        days,
        micros: micros_with_day_remainder / divisor as i128,
    })
}

pub(crate) fn add_interval_to_temporal(
    value: &str,
    interval: IntervalParts,
    add: bool,
) -> Option<String> {
    let original = parse_temporal(value)?;
    let sign = if add { 1 } else { -1 };
    let shifted = add_months(original, interval.months.checked_mul(sign)?)?;
    let day_delta = interval.days.checked_mul(sign)?;
    let days = days_from_civil(shifted.year, shifted.month, shifted.day)?.checked_add(day_delta)?;
    if !original.has_time {
        let whole_time_days = (interval.micros / 86_400_000_000) as i64;
        let final_days = days.checked_add(whole_time_days.checked_mul(sign)?)?;
        let (year, month, day) = civil_from_days(final_days)?;
        return Some(format_temporal(
            TemporalParts {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
                second: 0,
                micros: 0,
                has_time: false,
            },
            false,
        ));
    }
    let (year, month, day) = civil_from_days(days)?;
    let base_micros = ((shifted.hour as i128)
        .checked_mul(3_600_000_000)?
        .checked_add((shifted.minute as i128).checked_mul(60_000_000)?)?
        .checked_add((shifted.second as i128).checked_mul(1_000_000)?)?)
    .checked_add(shifted.micros as i128)?;
    let micros = base_micros.checked_add(interval.micros.checked_mul(sign as i128)?)?;
    let day_adjust = micros.div_euclid(86_400_000_000);
    let micros_of_day = micros.rem_euclid(86_400_000_000);
    let final_days = days.checked_add(day_adjust as i64)?;
    let (year, month, day) = civil_from_days(final_days).unwrap_or((year, month, day));
    let hour = micros_of_day / 3_600_000_000;
    let minute = (micros_of_day % 3_600_000_000) / 60_000_000;
    let second = (micros_of_day % 60_000_000) / 1_000_000;
    let micros = micros_of_day % 1_000_000;
    Some(format_temporal(
        TemporalParts {
            year,
            month,
            day,
            hour: hour as i64,
            minute: minute as i64,
            second: second as i64,
            micros: micros as i64,
            has_time: original.has_time || interval.micros != 0,
        },
        original.has_time,
    ))
}

pub(crate) fn epoch_seconds_to_timestamp(value: &Value) -> Result<Option<String>, ()> {
    let Some(seconds) = value_to_i128(value) else {
        return Ok(None);
    };
    let micros = ((seconds as f64) * 1_000_000.0) as i128;
    if micros > i64::MAX as i128 || micros < i64::MIN as i128 {
        return Err(());
    }
    let epoch_seconds = micros.div_euclid(1_000_000);
    let fraction = micros.rem_euclid(1_000_000) as i64;
    let days = epoch_seconds.div_euclid(86_400);
    let seconds_of_day = epoch_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days_i128(days).ok_or(())?;
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    Ok(Some(format_year_timestamp(
        year,
        month,
        day,
        hour as i64,
        minute as i64,
        second as i64,
        fraction,
    )))
}

pub(crate) fn temporal_sort_key(value: &str) -> Option<(i64, u32, u32, i64, i64, i64, i64)> {
    let temporal = parse_temporal(value)?;
    Some((
        temporal.year,
        temporal.month,
        temporal.day,
        temporal.hour,
        temporal.minute,
        temporal.second,
        temporal.micros,
    ))
}

pub(crate) fn interval_sort_key(value: &str) -> Option<(i64, i64, i128)> {
    let interval = parse_interval(value)?;
    Some((interval.months, interval.days, interval.micros))
}

fn normalized_unit(unit: &str) -> String {
    let lower = unit.to_ascii_lowercase();
    let lower = lower.trim_start_matches("dt.");
    match lower {
        "years" => "year".to_string(),
        "months" => "month".to_string(),
        "days" => "day".to_string(),
        "hours" => "hour".to_string(),
        "minutes" => "minute".to_string(),
        "seconds" => "second".to_string(),
        "milliseconds" | "millis" | "ms" => "millisecond".to_string(),
        "microseconds" | "micros" | "us" => "microsecond".to_string(),
        "decades" => "decade".to_string(),
        "centuries" => "century".to_string(),
        "millenniums" | "millennia" => "millennium".to_string(),
        "quarters" => "quarter".to_string(),
        _ => lower.to_string(),
    }
}

fn interval_part(unit: &str, interval: IntervalParts) -> Option<i64> {
    let value = match normalized_unit(unit).as_str() {
        "year" => interval.months / 12,
        "month" => interval.months % 12,
        "day" => interval.days,
        "hour" => (interval.micros / 3_600_000_000) as i64,
        "minute" => ((interval.micros / 60_000_000) % 60) as i64,
        "second" => ((interval.micros / 1_000_000) % 60) as i64,
        "millisecond" => ((interval.micros / 1_000) % 60_000) as i64,
        "microsecond" => (interval.micros % 60_000_000) as i64,
        "decade" => interval.months / 120,
        "century" => interval.months / 1200,
        "millennium" => interval.months / 12000,
        "quarter" => interval.months.rem_euclid(12) / 3 + 1,
        _ => return None,
    };
    Some(value)
}

fn split_compact_interval_part(part: &str) -> Option<(i64, String)> {
    let split = part
        .char_indices()
        .find_map(|(idx, ch)| (idx > 0 && !ch.is_ascii_digit() && ch != '-').then_some(idx))?;
    let (value, unit) = part.split_at(split);
    Some((value.parse().ok()?, unit.to_ascii_lowercase()))
}

fn strip_timezone(time: &str) -> &str {
    if let Some(time) = time.strip_suffix('Z') {
        return time;
    }
    time.char_indices()
        .rev()
        .find_map(|(idx, ch)| (idx > 0 && (ch == '+' || ch == '-')).then_some(&time[..idx]))
        .unwrap_or(time)
}

fn parse_fraction_micros(fraction: &str) -> Option<i64> {
    let digits = fraction
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .take(6)
        .collect::<String>();
    if digits.is_empty() {
        return Some(0);
    }
    let padded = format!("{digits:0<6}");
    padded.parse().ok()
}

fn parse_interval_time(raw: &str) -> Option<i128> {
    let mut sign = 1i128;
    let mut text = raw;
    if let Some(rest) = text.strip_prefix('-') {
        sign = -1;
        text = rest;
    }
    let parts = text.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    let hours = parts[0].parse::<i128>().ok()?;
    let minutes = parts[1].parse::<i128>().ok()?;
    let (seconds_text, fraction) = parts[2].split_once('.').unwrap_or((parts[2], ""));
    let seconds = seconds_text.parse::<i128>().ok()?;
    let micros = parse_fraction_micros(fraction)? as i128;
    sign.checked_mul(
        hours
            .checked_mul(3_600_000_000)?
            .checked_add(minutes.checked_mul(60_000_000)?)?
            .checked_add(seconds.checked_mul(1_000_000)?)?
            .checked_add(micros)?,
    )
}

fn format_interval_time(micros: i128) -> String {
    let sign = if micros < 0 { "-" } else { "" };
    let abs = micros.abs();
    let hours = abs / 3_600_000_000;
    let minutes = (abs % 3_600_000_000) / 60_000_000;
    let seconds = (abs % 60_000_000) / 1_000_000;
    let fraction = abs % 1_000_000;
    if fraction == 0 {
        format!("{sign}{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!(
            "{sign}{hours:02}:{minutes:02}:{seconds:02}.{}",
            trim_fraction(fraction as i64)
        )
    }
}

fn format_temporal(temporal: TemporalParts, force_time: bool) -> String {
    if force_time || temporal.has_time {
        format_year_timestamp(
            temporal.year,
            temporal.month,
            temporal.day,
            temporal.hour,
            temporal.minute,
            temporal.second,
            temporal.micros,
        )
    } else {
        format!(
            "{:04}-{:02}-{:02}",
            temporal.year, temporal.month, temporal.day
        )
    }
}

fn format_year_timestamp(
    year: i64,
    month: u32,
    day: u32,
    hour: i64,
    minute: i64,
    second: i64,
    micros: i64,
) -> String {
    let fraction = if micros == 0 {
        String::new()
    } else {
        format!(".{}", trim_fraction(micros))
    };
    if year <= 0 {
        format!(
            "{:04}-{:02}-{:02} (BC) {:02}:{:02}:{:02}{}",
            1 - year,
            month,
            day,
            hour,
            minute,
            second,
            fraction
        )
    } else {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}{}",
            year, month, day, hour, minute, second, fraction
        )
    }
}

fn trim_fraction(fraction: i64) -> String {
    let mut text = format!("{fraction:06}");
    while text.ends_with('0') {
        text.pop();
    }
    text
}

fn zero_time(temporal: &mut TemporalParts) {
    temporal.hour = 0;
    temporal.minute = 0;
    temporal.second = 0;
    temporal.micros = 0;
}

fn add_months(mut temporal: TemporalParts, months: i64) -> Option<TemporalParts> {
    let zero_based = (temporal.month as i64 - 1).checked_add(months)?;
    temporal.year = temporal.year.checked_add(zero_based.div_euclid(12))?;
    temporal.month = (zero_based.rem_euclid(12) + 1) as u32;
    temporal.day = temporal
        .day
        .min(days_in_month(temporal.year, temporal.month));
    Some(temporal)
}

fn value_to_i128(value: &Value) -> Option<i128> {
    match value {
        Value::Byte(n) => Some(*n as i128),
        Value::Short(n) => Some(*n as i128),
        Value::Int(n) | Value::Long(n) => Some(*n as i128),
        Value::BigInt(n) => n.to_i128(),
        Value::BigDecimal(n) => n.to_i128(),
        _ => None,
    }
}

fn valid_date(year: i64, month: u32, day: u32) -> bool {
    (1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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

fn civil_from_days_i128(days_since_epoch: i128) -> Option<(i64, u32, u32)> {
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
    Some((year.try_into().ok()?, m as u32, d as u32))
}
