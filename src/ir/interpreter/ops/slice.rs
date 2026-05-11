//! GraphSlice — offset/fetch/tail.
//!
//! Extracted from `interpreter.rs` lines 1531..1545.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{InterpretError, IrResult, Row};

pub(crate) fn slice_op(slice: &crate::ir::plan::Slice, rows: Vec<Row>) -> IrResult<Vec<Row>> {
    let mut out: Vec<Row> = rows;
    if let Some(tail) = slice.tail {
        let len = out.len();
        let start = len.saturating_sub(tail as usize);
        out = out.split_off(start);
        return Ok(out);
    }
    let mut iter = out.into_iter().skip(slice.offset as usize);
    let collected: Vec<Row> = match slice.fetch {
        Some(fetch) => iter.by_ref().take(fetch as usize).collect(),
        None => iter.collect(),
    };
    Ok(collected)
}

pub(crate) fn slice_expr_op(
    offset: Option<&IrExpr>,
    fetch: Option<&IrExpr>,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let offset = match offset {
        Some(expr) => Some(evaluate_slice_bound("SKIP", expr, graph)?),
        None => None,
    };
    let fetch = match fetch {
        Some(expr) => Some(evaluate_slice_bound("LIMIT", expr, graph)?),
        None => None,
    };
    slice_op(
        &crate::ir::plan::Slice {
            offset: offset.unwrap_or(0),
            fetch,
            tail: None,
        },
        rows,
    )
}

fn evaluate_slice_bound(name: &str, expr: &IrExpr, graph: &PropertyGraph) -> IrResult<u64> {
    let row = Row::new();
    match eval(expr, &row, graph)? {
        Value::Byte(value) if value >= 0 => Ok(value as u64),
        Value::Short(value) if value >= 0 => Ok(value as u64),
        Value::Int(value) if value >= 0 => Ok(value as u64),
        Value::Long(value) if value >= 0 => Ok(value as u64),
        Value::BigInt(value) => value.to_string().parse::<u64>().map_err(|_| {
            InterpretError::Type(format!("{name} expression must evaluate to a non-negative integer"))
        }),
        other => Err(InterpretError::Type(format!(
            "{name} expression must evaluate to a non-negative integer, got {}",
            other.type_name()
        ))),
    }
}
