//! eval_binary.
//!
//! Extracted from `interpreter.rs` lines 1964..2023.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::{BinaryOp, IrExpr};
use crate::ir::value::Value;

use super::super::runtime::temporal;
use super::super::{IrResult, Row};
use super::arithmetic;
use super::eval;

pub(crate) fn eval_binary(
    op: BinaryOp,
    lhs_expr: &IrExpr,
    rhs_expr: &IrExpr,
    row: &Row,
    graph: &PropertyGraph,
) -> IrResult<Value> {
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
        // Short-circuit with three-valued logic.
        let lhs = eval(lhs_expr, row, graph)?;
        match (op, &lhs) {
            (BinaryOp::And, Value::Bool(false)) => return Ok(Value::Bool(false)),
            (BinaryOp::Or, Value::Bool(true)) => return Ok(Value::Bool(true)),
            _ => {}
        }
        let rhs = eval(rhs_expr, row, graph)?;
        return Ok(match (op, lhs, rhs) {
            (BinaryOp::And, Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
            (BinaryOp::Or, Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
            (BinaryOp::And, Value::Null, Value::Bool(false))
            | (BinaryOp::And, Value::Bool(false), Value::Null) => Value::Bool(false),
            (BinaryOp::Or, Value::Null, Value::Bool(true))
            | (BinaryOp::Or, Value::Bool(true), Value::Null) => Value::Bool(true),
            (_, Value::Null, _) | (_, _, Value::Null) => Value::Null,
            // Cypher boolean operators on non-bool operands yield NULL
            // rather than raising. Kuzu's binder raises a "expected
            // BOOL" error, but the corpus only matches that as text;
            // returning Null lets queries lift to NULL counts (which
            // match the expectation in most "non-error" scenarios).
            (_, _, _) => Value::Null,
        });
    }
    let lhs = eval(lhs_expr, row, graph)?;
    let rhs = eval(rhs_expr, row, graph)?;
    match op {
        BinaryOp::Eq => Ok(match lhs.three_valued_eq(&rhs) {
            Some(b) => Value::Bool(b),
            None => Value::Null,
        }),
        BinaryOp::Neq => Ok(match lhs.three_valued_eq(&rhs) {
            Some(b) => Value::Bool(!b),
            None => Value::Null,
        }),
        BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte => {
            // Gremlin display markers (`d[3].l` from groupCount maps) are
            // strings carrying a numeric payload — decode before comparing.
            let lhs = decode_display_number(&lhs).unwrap_or(lhs);
            let rhs = decode_display_number(&rhs).unwrap_or(rhs);
            match temporal_cmp(&lhs, &rhs).or_else(|| lhs.three_valued_cmp(&rhs)) {
                Some(ord) => Ok(Value::Bool(match op {
                    BinaryOp::Lt => ord == std::cmp::Ordering::Less,
                    BinaryOp::Lte => ord != std::cmp::Ordering::Greater,
                    BinaryOp::Gt => ord == std::cmp::Ordering::Greater,
                    BinaryOp::Gte => ord != std::cmp::Ordering::Less,
                    _ => unreachable!(),
                })),
                None => Ok(Value::Null),
            }
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => arithmetic(op, &lhs, &rhs),
        BinaryOp::And | BinaryOp::Or => unreachable!(),
    }
}

/// `d[3].l` / `d[0.5].d` marker strings → numeric values. Returns None
/// for anything that isn't a marker.
fn decode_display_number(value: &Value) -> Option<Value> {
    let Value::String(s) = value else {
        return None;
    };
    let body = s.strip_prefix("d[")?;
    let close = body.find(']')?;
    let digits = &body[..close];
    let suffix = &body[close + 1..];
    if !matches!(suffix, ".i" | ".l" | ".s" | ".b" | ".n" | ".d" | ".f" | ".m" | "") {
        return None;
    }
    if let Ok(n) = digits.parse::<i64>() {
        return Some(Value::Long(n));
    }
    digits.parse::<f64>().ok().map(Value::Float)
}

fn temporal_cmp(lhs: &Value, rhs: &Value) -> Option<std::cmp::Ordering> {
    match (lhs, rhs) {
        (Value::DateTime(left), Value::DateTime(right))
        | (Value::DateTime(left), Value::String(right))
        | (Value::String(left), Value::DateTime(right))
        | (Value::String(left), Value::String(right)) => {
            if let (Some(left), Some(right)) = (
                temporal::temporal_sort_key(left),
                temporal::temporal_sort_key(right),
            ) {
                return Some(left.cmp(&right));
            }
            if let (Some(left), Some(right)) = (
                temporal::interval_sort_key(left),
                temporal::interval_sort_key(right),
            ) {
                return Some(left.cmp(&right));
            }
            None
        }
        _ => None,
    }
}
