//! Calendar arithmetic, duration scaling, and temporal differences.
use super::{Result, TemporalValue, parsing::resolve_zone, value::offset_text};
use crate::ir::Value;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
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
        TemporalValue::Date(d) => TemporalValue::Date(
            calendar(*d)?
                .checked_add_signed(Duration::days(seconds / 86400))
                .ok_or("Date shift out of range")?,
        ),
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

pub(super) fn normalized_duration(
    months: i64,
    days: i64,
    total_nanos: i128,
) -> Result<TemporalValue> {
    Ok(TemporalValue::Duration {
        months,
        days,
        seconds: i64::try_from(total_nanos.div_euclid(1_000_000_000))
            .map_err(|_| "Duration overflow")?,
        nanos: total_nanos.rem_euclid(1_000_000_000) as i32,
    })
}

pub fn arithmetic(op: crate::ir::expr::BinaryOp, left: &Value, right: &Value) -> Result<Value> {
    use crate::ir::expr::BinaryOp;
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
    // Missing zones inherit the other operand's zone at their own local date,
    // including the offset on the far side of a DST transition.
    let inherited_offset = |value: &TemporalValue, other: &TemporalValue, local| -> Result<i32> {
        if let Some(offset) = offset(value) {
            return Ok(offset);
        }
        if let TemporalValue::DateTime(_, Some(zone)) = other {
            return resolve_zone(local, zone).map(|(d, _)| d.offset().local_minus_utc());
        }
        Ok(offset(other).unwrap_or(0))
    };
    let left_offset = inherited_offset(left, right, ld.and_time(lt))?;
    let right_offset = inherited_offset(right, left, rd.and_time(rt))?;
    let offset_delta = (left_offset as i128 - right_offset as i128) * 1_000_000_000;
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
    let remainder_at = |date: NaiveDate| -> Result<i128> {
        let shifted_offset = match left {
            TemporalValue::DateTime(_, Some(zone)) => resolve_zone(date.and_time(lt), zone)?
                .0
                .offset()
                .local_minus_utc(),
            _ => left_offset,
        };
        Ok(delta_nanos(date.and_time(lt), rd.and_time(rt))
            + (shifted_offset as i128 - right_offset as i128) * 1_000_000_000)
    };
    if unit == "indays" {
        let mut days = if both_dates { (rd - ld).num_days() } else { 0 };
        let remaining = remainder_at(rd)?;
        if (days > 0 && remaining < 0) || (days < 0 && remaining > 0) {
            days -= days.signum();
        }
        return normalized_duration(0, days, 0).map(Value::Temporal);
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
    let remaining = remainder_at(cursor)?;
    if (months > 0 && remaining < 0) || (months < 0 && remaining > 0) {
        months -= months.signum();
        cursor = shifted_date(months)?;
    }
    if unit == "inmonths" {
        return normalized_duration(months, 0, 0).map(Value::Temporal);
    }
    if unit != "between" {
        return Err("Unknown duration difference unit".into());
    }
    let mut days = if both_dates {
        (rd - cursor).num_days()
    } else {
        0
    };
    let time_nanos = remainder_at(rd)?;
    if (days > 0 && time_nanos < 0) || (days < 0 && time_nanos > 0) {
        days -= days.signum();
    }
    let after_days = cursor
        .checked_add_signed(Duration::try_days(days).ok_or("Duration overflow")?)
        .ok_or("Date overflow")?;
    let remaining = remainder_at(after_days)?;
    normalized_duration(months, days, remaining).map(Value::Temporal)
}
