//! Fixture date, timestamp, and interval normalization.

pub(super) fn format_timestamp_for_type(raw: &str, type_name: &str) -> Option<String> {
    let normalized = normalize_timestamp(raw);
    let (body, zone) = normalized
        .strip_suffix("+00")
        .map(|body| (body, "+00"))
        .unwrap_or((normalized.as_str(), ""));
    let (prefix, fraction) = body
        .split_once('.')
        .map(|(prefix, fraction)| (prefix, fraction))
        .unwrap_or((body, ""));
    let digits = match type_name {
        "TIMESTAMP_MS" => 3,
        "TIMESTAMP_SEC" => 0,
        _ => 6,
    };
    let fraction = if digits == 0 {
        String::new()
    } else {
        let mut value: String = fraction
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .take(digits)
            .collect();
        while value.len() < digits && !value.is_empty() {
            value.push('0');
        }
        if value.is_empty() {
            String::new()
        } else {
            format!(".{value}")
        }
    };
    let zone = if type_name == "TIMESTAMP_TZ" {
        "+00"
    } else {
        zone
    };
    Some(format!("{prefix}{fraction}{zone}"))
}

/// Normalise a verbose interval literal to Kuzu's printer form. Kuzu
/// emits years and months as words but rolls hours / minutes / seconds
/// (plus optional microseconds) into a compact `HH:MM:SS[.uuuuuu]`
/// tail. The CSV input may use any of `hours`, `minutes`, `seconds`,
/// `milliseconds`, `microseconds`, or the abbreviation `us`.
pub(super) fn normalize_interval(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return raw.to_string();
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let mut prefix: Vec<String> = Vec::new();
    let mut hours: i64 = 0;
    let mut minutes: i64 = 0;
    let mut seconds: i64 = 0;
    let mut micros: i64 = 0;
    let mut saw_time_component = false;
    let mut i = 0;
    while i < parts.len() {
        let value_part = parts[i];
        let (value, unit, consumed) = match value_part.parse::<i64>() {
            Ok(v) if i + 1 < parts.len() => (
                v,
                parts[i + 1].trim_end_matches(',').to_ascii_lowercase(),
                2,
            ),
            _ => match split_compact_interval_part(value_part) {
                Some((value, unit)) => (value, unit, 1),
                None => return raw.to_string(),
            },
        };
        match unit.as_str() {
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
            "hour" | "hours" | "h" => {
                hours = value;
                saw_time_component = true;
            }
            "minute" | "minutes" | "min" | "mins" | "m" => {
                minutes = value;
                saw_time_component = true;
            }
            "second" | "seconds" | "sec" | "secs" | "s" => {
                seconds = value;
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
            _ => return raw.to_string(),
        }
        i += consumed;
    }
    if !saw_time_component {
        return if prefix.is_empty() {
            raw.to_string()
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

fn split_compact_interval_part(part: &str) -> Option<(i64, String)> {
    let split = part
        .char_indices()
        .find_map(|(idx, ch)| (idx > 0 && !ch.is_ascii_digit() && ch != '-').then_some(idx))?;
    let (value, unit) = part.split_at(split);
    Some((value.parse().ok()?, unit.to_ascii_lowercase()))
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

/// Lowercase and hyphenate a UUID-shaped string to Kuzu's canonical
/// 8-4-4-4-12 form. Anything that doesn't look like a UUID after the
/// strip passes through.
pub(super) fn normalize_timestamp(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return raw.to_string();
    }
    // Split into date + time part on `T` or ` `.
    let Some((date, time)) = trimmed.split_once('T').or_else(|| trimmed.split_once(' ')) else {
        return normalize_date(trimmed);
    };
    let date_norm = normalize_date(date);

    // Detect timezone suffix.
    let (time_body, offset_minutes) = if let Some(body) = time.strip_suffix('Z') {
        (body.to_string(), 0i32)
    } else if let Some(idx) = time
        .char_indices()
        .rev()
        .find_map(|(i, c)| (i > 0 && (c == '+' || c == '-')).then_some(i))
    {
        let (body, offset) = time.split_at(idx);
        let sign: i32 = if offset.starts_with('-') { -1 } else { 1 };
        let mut parts = offset[1..].split(':');
        let hours: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minutes: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        // Kuzu's CSV occasionally produces `…Z+00:00` (a `Z` followed
        // by an explicit offset). Strip a residual `Z` so the body is
        // a clean `HH:MM:SS[.fff]`.
        let body = body.trim_end_matches('Z');
        (body.to_string(), sign * (hours * 60 + minutes))
    } else {
        (time.to_string(), 0)
    };

    // Apply the offset by treating the value as an epoch-millis number.
    if offset_minutes == 0 {
        return format!("{date_norm} {time_body}");
    }
    let mut date_parts = date_norm.split('-');
    let year: i64 = date_parts
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1970);
    let month: u32 = date_parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let day: u32 = date_parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut time_parts = time_body.split(':');
    let hour: i64 = time_parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minute: i64 = time_parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let second_part = time_parts.next().unwrap_or("0");
    let (second, fraction) = if let Some((s, f)) = second_part.split_once('.') {
        (s.parse::<i64>().unwrap_or(0), Some(f.to_string()))
    } else {
        (second_part.parse::<i64>().unwrap_or(0), None)
    };
    let Some(days) = days_from_civil(year, month, day) else {
        return format!("{date_norm} {time_body}");
    };
    let local_secs = days * 86_400 + hour * 3600 + minute * 60 + second;
    let utc_secs = local_secs - (offset_minutes as i64) * 60;
    let new_days = utc_secs.div_euclid(86_400);
    let secs_of_day = utc_secs.rem_euclid(86_400);
    let new_hour = secs_of_day / 3600;
    let new_minute = (secs_of_day % 3600) / 60;
    let new_second = secs_of_day % 60;
    let Some((y, m, d)) = civil_from_days(new_days) else {
        return format!("{date_norm} {time_body}");
    };
    let frac = fraction.map(|f| format!(".{f}")).unwrap_or_default();
    format!("{y:04}-{m:02}-{d:02} {new_hour:02}:{new_minute:02}:{new_second:02}{frac}")
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

/// Zero-pad a `YYYY-M-D` style date to `YYYY-MM-DD`. Leaves anything
/// that doesn't look like a date alone.
pub(super) fn normalize_date(raw: &str) -> String {
    let trimmed = raw.trim();
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 3 {
        return raw.to_string();
    }
    let (year, month, day) = (parts[0], parts[1], parts[2]);
    if year.is_empty() || month.is_empty() || day.is_empty() {
        return raw.to_string();
    }
    if !year.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
        || !day.chars().all(|c| c.is_ascii_digit())
    {
        return raw.to_string();
    }
    format!("{year:0>4}-{month:0>2}-{day:0>2}")
}

pub(super) fn parse_date_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let (year, month, day) = (parts[0], parts[1], parts[2]);
    if year.is_empty() || month.is_empty() || day.is_empty() {
        return None;
    }
    if !year.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
        || !day.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(normalize_date(raw))
}
