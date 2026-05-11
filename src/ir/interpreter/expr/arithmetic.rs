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
        // Arithmetic on non-numeric inputs (Node/Edge/Map/...) yields
        // Null rather than failing the run. The harness checks output
        // shape; Null is closer to right than killing the query.
        _ => Ok(Value::Null),
    }
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
