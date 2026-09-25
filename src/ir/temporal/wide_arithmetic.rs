//! Calendar differences that do not narrow dates to chrono's year range.
use super::{Result, TemporalValue, arithmetic::normalized_duration};
use chrono::{NaiveTime, Timelike};

pub(super) fn between(
    unit: &str,
    left: &TemporalValue,
    right: &TemporalValue,
) -> Result<TemporalValue> {
    let ld = left
        .calendar_date()
        .or_else(|| right.calendar_date())
        .ok_or("Expected calendar date")?;
    let rd = right
        .calendar_date()
        .or_else(|| left.calendar_date())
        .ok_or("Expected calendar date")?;
    let both_dates = left.calendar_date().is_some() && right.calendar_date().is_some();
    let clock = |value: &TemporalValue| {
        let time = value
            .time()
            .unwrap_or(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        i128::from(time.num_seconds_from_midnight()) * 1_000_000_000 + i128::from(time.nanosecond())
    };
    let offset = |value: &TemporalValue| match value {
        TemporalValue::Time(_, offset) => Some(*offset),
        TemporalValue::DateTime(value, _) => Some(value.offset().local_minus_utc()),
        _ => None,
    };
    let lo = offset(left).or_else(|| offset(right)).unwrap_or(0);
    let ro = offset(right).or_else(|| offset(left)).unwrap_or(0);
    let clock_delta = clock(right) - clock(left) + i128::from(lo - ro) * 1_000_000_000;
    let day_nanos = 86400_i128 * 1_000_000_000;
    let remainder = |date: super::CalendarDate| {
        i128::from(rd.epoch_days() - date.epoch_days()) * day_nanos + clock_delta
    };
    if unit == "inseconds" {
        return normalized_duration(0, 0, remainder(ld));
    }
    if unit == "indays" {
        return normalized_duration(
            0,
            if both_dates {
                (remainder(ld) / day_nanos) as i64
            } else {
                0
            },
            0,
        );
    }
    let mut months = if both_dates {
        i64::from(rd.year - ld.year) * 12 + i64::from(rd.month) - i64::from(ld.month)
    } else {
        0
    };
    let mut cursor = ld.add_months(months)?;
    if i128::from(months.signum()) * remainder(cursor) < 0 {
        months -= months.signum();
        cursor = ld.add_months(months)?;
    }
    if unit == "inmonths" {
        return normalized_duration(months, 0, 0);
    }
    if unit != "between" {
        return Err("Unknown duration difference unit".into());
    }
    let days = if both_dates {
        (remainder(cursor) / day_nanos) as i64
    } else {
        0
    };
    normalized_duration(months, days, remainder(cursor.add_days(days)?))
}
