//! GraphSlice — offset/fetch/tail.
//!
//! Extracted from `interpreter.rs` lines 1531..1545.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{InterpretError, IrResult, Row};

pub(crate) fn slice_op(slice: &crate::ir::plan::Slice, rows: Vec<Row>) -> IrResult<Vec<Row>> {
    // Offsets and bounds count logical traversers. Splitting a bulk row
    // preserves its bindings and sack; it does not materialize each copy.
    if let Some(tail) = slice.tail {
        let mut out = slice_bulk(rows.into_iter().rev(), 0, Some(tail));
        out.reverse();
        return Ok(out);
    }
    Ok(slice_bulk(rows.into_iter(), slice.offset, slice.fetch))
}

fn slice_bulk(rows: impl Iterator<Item = Row>, mut skip: u64, fetch: Option<u64>) -> Vec<Row> {
    let mut remaining = fetch.unwrap_or(u64::MAX);
    let mut out = Vec::new();
    for mut row in rows {
        if remaining == 0 {
            break;
        }
        if skip >= row.bulk {
            skip -= row.bulk;
            continue;
        }
        row.bulk -= skip;
        skip = 0;
        row.bulk = row.bulk.min(remaining);
        remaining -= row.bulk;
        if row.bulk > 0 {
            out.push(row);
        }
    }
    out
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
        Value::Float(value) if value.is_finite() && value >= 0.0 => Ok(value as u64),
        Value::Float32(value) if value.is_finite() && value >= 0.0 => Ok(value as u64),
        Value::BigInt(value) => value.to_string().parse::<u64>().map_err(|_| {
            InterpretError::Type(format!(
                "{name} expression must evaluate to a non-negative integer"
            ))
        }),
        // Cypher's NULL LIMIT / NULL SKIP semantics: treat as "no
        // bound" rather than fail. Returning `u64::MAX` lets the
        // slice operator see an effectively unbounded fetch and
        // mirror `LIMIT NULL`/`SKIP NULL` no-op behaviour.
        Value::Null => Ok(u64::MAX),
        other => Err(InterpretError::Type(format!(
            "{name} expression must evaluate to a non-negative integer, got {}",
            other.type_name()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::plan::Slice;
    fn rows() -> Vec<Row> {
        [(1, 3), (2, 5), (3, 2)]
            .into_iter()
            .map(|(value, bulk)| {
                let mut row = Row::new().with("current", Value::Int(value));
                row.bulk = bulk;
                row
            })
            .collect()
    }
    #[test]
    fn range_splits_both_boundary_traversers() {
        let out = slice_op(
            &Slice {
                offset: 2,
                fetch: Some(4),
                tail: None,
            },
            rows(),
        )
        .unwrap();
        assert_eq!(out.iter().map(|row| row.bulk).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(out[0].bindings["current"], Value::Int(1));
        assert_eq!(out[1].bindings["current"], Value::Int(2));
    }
    #[test]
    fn tail_splits_bulk_preserving_order_and_empty_bounds() {
        let out = slice_op(
            &Slice {
                offset: 0,
                fetch: None,
                tail: Some(4),
            },
            rows(),
        )
        .unwrap();
        assert_eq!(out.iter().map(|row| row.bulk).collect::<Vec<_>>(), [2, 2]);
        assert_eq!(out[0].bindings["current"], Value::Int(2));
        assert!(
            slice_op(
                &Slice {
                    offset: 0,
                    fetch: Some(0),
                    tail: None
                },
                rows()
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            slice_op(
                &Slice {
                    offset: 11,
                    fetch: None,
                    tail: None
                },
                rows()
            )
            .unwrap()
            .is_empty()
        );
    }
}
