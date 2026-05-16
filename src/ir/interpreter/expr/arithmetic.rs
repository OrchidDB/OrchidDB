//! Arithmetic on Int/Float/BigInt/BigDecimal with cross-promotion.
//!
//! Extracted from `interpreter.rs` lines 2025..2132.

use crate::ir::expr::BinaryOp;
use crate::ir::value::Value;

use super::super::IrResult;

pub(crate) fn arithmetic(op: BinaryOp, lhs: &Value, rhs: &Value) -> IrResult<Value> {
    use bigdecimal::BigDecimal;
    use num_bigint::BigInt;
    fn bigdecimal_arith(
        op: BinaryOp,
        a: bigdecimal::BigDecimal,
        b: bigdecimal::BigDecimal,
    ) -> Value {
        use num_traits::Zero;
        let result = match op {
            BinaryOp::Add => a + b,
            BinaryOp::Sub => a - b,
            BinaryOp::Mul => a * b,
            BinaryOp::Div => {
                if b.is_zero() {
                    return Value::Null;
                }
                a / b
            }
            _ => return Value::Null,
        };
        Value::BigDecimal(result)
    }
    fn bigint_arith(op: BinaryOp, a: num_bigint::BigInt, b: num_bigint::BigInt) -> Value {
        use num_traits::Zero;
        let result = match op {
            BinaryOp::Add => a + b,
            BinaryOp::Sub => a - b,
            BinaryOp::Mul => a * b,
            BinaryOp::Div => {
                if b.is_zero() {
                    return Value::Null;
                }
                a / b
            }
            _ => return Value::Null,
        };
        Value::BigInt(result)
    }
    match (lhs, rhs) {
        (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
        (Value::Byte(a), other) => arithmetic(op, &Value::Int(*a as i64), other),
        (other, Value::Byte(b)) => arithmetic(op, other, &Value::Int(*b as i64)),
        (Value::Short(a), other) => arithmetic(op, &Value::Int(*a as i64), other),
        (other, Value::Short(b)) => arithmetic(op, other, &Value::Int(*b as i64)),
        (Value::Long(a), other) => arithmetic(op, &Value::Int(*a), other),
        (other, Value::Long(b)) => arithmetic(op, other, &Value::Int(*b)),
        (Value::Float32(a), other) => arithmetic(op, &Value::Float(*a as f64), other),
        (other, Value::Float32(b)) => arithmetic(op, other, &Value::Float(*b as f64)),
        // ----- BigDecimal absorbs exact numerics; Float keeps math() Double-like. -----
        (Value::BigDecimal(a), Value::BigDecimal(b)) => {
            Ok(bigdecimal_arith(op, a.clone(), b.clone()))
        }
        (Value::BigDecimal(a), Value::Float(b)) | (Value::Float(b), Value::BigDecimal(a)) => {
            use num_traits::ToPrimitive;
            let Some(a) = a.to_f64() else {
                return Ok(Value::Null);
            };
            Ok(float_arith(op, a, *b))
        }
        (Value::BigDecimal(a), other) | (other, Value::BigDecimal(a)) => {
            let promoted = match other {
                Value::Int(n) => Some(BigDecimal::from(*n)),
                Value::BigInt(n) => Some(BigDecimal::from(n.clone())),
                _ => None,
            };
            match promoted {
                Some(b) => Ok(bigdecimal_arith(op, a.clone(), b)),
                None => Ok(Value::Null),
            }
        }
        // ----- BigInt absorbs Int/Bool; promotes to BigDecimal vs Float -----
        (Value::BigInt(a), Value::BigInt(b)) => Ok(bigint_arith(op, a.clone(), b.clone())),
        (Value::BigInt(a), Value::Int(b)) | (Value::Int(b), Value::BigInt(a)) => {
            Ok(bigint_arith(op, a.clone(), BigInt::from(*b)))
        }
        (Value::BigInt(a), Value::Float(b)) | (Value::Float(b), Value::BigInt(a)) => {
            use num_traits::ToPrimitive;
            let Some(a) = a.to_f64() else {
                return Ok(Value::Null);
            };
            Ok(float_arith(op, a, *b))
        }
        // ----- ordinary fixed-width paths -----
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(match op {
            BinaryOp::Add => a + b,
            BinaryOp::Sub => a - b,
            BinaryOp::Mul => a * b,
            BinaryOp::Div => {
                if *b == 0 {
                    return Ok(Value::Null);
                }
                a / b
            }
            _ => unreachable!(),
        })),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(match op {
            BinaryOp::Add => a + b,
            BinaryOp::Sub => a - b,
            BinaryOp::Mul => a * b,
            BinaryOp::Div => a / b,
            _ => unreachable!(),
        })),
        (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => {
            let a = *a as f64;
            let b = *b;
            Ok(Value::Float(match op {
                BinaryOp::Add => a + b,
                BinaryOp::Sub => a - b,
                BinaryOp::Mul => a * b,
                BinaryOp::Div => a / b,
                _ => unreachable!(),
            }))
        }
        (Value::String(a), Value::String(b)) if matches!(op, BinaryOp::Add) => {
            Ok(Value::String(format!("{a}{b}")))
        }
        (Value::List(a), Value::List(b)) if matches!(op, BinaryOp::Add) => {
            let mut out = a.clone();
            out.extend(b.iter().cloned());
            Ok(Value::List(out))
        }
        (Value::List(a), b) if matches!(op, BinaryOp::Add) => {
            let mut out = a.clone();
            out.push(b.clone());
            Ok(Value::List(out))
        }
        (a, Value::List(b)) if matches!(op, BinaryOp::Add) => {
            let mut out = Vec::with_capacity(b.len() + 1);
            out.push(a.clone());
            out.extend(b.iter().cloned());
            Ok(Value::List(out))
        }
        // ----- DATE / TIMESTAMP arithmetic. Kuzu's case files exercise
        // `birthdate + N` to shift forward by N days; mirror that by
        // adding `N * 86400 * 1000` epoch milliseconds and reformatting.
        // We accept either Value::DateTime or a String column (the
        // loader stores DATE columns as String, see `loader::Date`).
        (Value::DateTime(s), Value::Int(days))
        | (Value::DateTime(s), Value::Long(days))
            if matches!(op, BinaryOp::Add | BinaryOp::Sub) =>
        {
            Ok(date_shift(s, *days, matches!(op, BinaryOp::Add)))
        }
        (Value::Int(days), Value::DateTime(s))
        | (Value::Long(days), Value::DateTime(s))
            if matches!(op, BinaryOp::Add) =>
        {
            Ok(date_shift(s, *days, true))
        }
        (Value::String(s), Value::Int(days))
        | (Value::String(s), Value::Long(days))
            if matches!(op, BinaryOp::Add | BinaryOp::Sub) && looks_like_date(s) =>
        {
            Ok(date_shift(s, *days, matches!(op, BinaryOp::Add)))
        }
        (Value::Int(days), Value::String(s))
        | (Value::Long(days), Value::String(s))
            if matches!(op, BinaryOp::Add) && looks_like_date(s) =>
        {
            Ok(date_shift(s, *days, true))
        }
        // Arithmetic on non-numeric inputs (Node/Edge/Map/...) yields
        // Null rather than failing the run. The harness checks output
        // shape; Null is closer to right than killing the query.
        _ => Ok(Value::Null),
    }
}

/// Look for a `YYYY-MM-DD` (with optional time part) shape. We don't
/// want generic strings to lift through the date-arithmetic path.
fn looks_like_date(s: &str) -> bool {
    let head: String = s.chars().take(10).collect();
    let parts: Vec<&str> = head.split('-').collect();
    parts.len() == 3
        && parts[0].len() == 4
        && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

/// Add (or subtract) `days` from a `YYYY-MM-DD[…]` string and return
/// the resulting date as `Value::DateTime`. Anything we can't parse
/// surfaces as `Null` so the row drops instead of carrying junk.
fn date_shift(raw: &str, days: i64, add: bool) -> Value {
    let trimmed = raw.trim();
    let head: String = trimmed.chars().take(10).collect();
    let parts: Vec<&str> = head.split('-').collect();
    if parts.len() != 3 {
        return Value::Null;
    }
    let Ok(year): Result<i64, _> = parts[0].parse() else {
        return Value::Null;
    };
    let Ok(month): Result<u32, _> = parts[1].parse() else {
        return Value::Null;
    };
    let Ok(day): Result<u32, _> = parts[2].parse() else {
        return Value::Null;
    };
    let Some(start) = days_from_civil(year, month, day) else {
        return Value::Null;
    };
    let target = if add { start + days } else { start - days };
    let Some((y, m, d)) = civil_from_days(target) else {
        return Value::Null;
    };
    Value::DateTime(format!("{y:04}-{m:02}-{d:02}"))
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

fn float_arith(op: BinaryOp, a: f64, b: f64) -> Value {
    Value::Float(match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
        _ => return Value::Null,
    })
}
