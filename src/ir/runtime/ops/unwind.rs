//! GraphUnwind — list fan-out.

use std::collections::BTreeMap;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};

pub(crate) fn unwind_op(
    expr: &IrExpr,
    bind: &str,
    outer_flag: bool,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for mut row in rows {
        let value = eval(expr, &row, graph)?;
        // The destination is overwritten in every output. Do not copy its old
        // payload when duplicating the bindings that actually survive.
        row.bindings.remove(bind);
        match value {
            // Native sets unfold to their members, preserving encounter order.
            Value::List(items) | Value::BulkSet(items) | Value::Set(items) => {
                if items.is_empty() && outer_flag {
                    append_rows(row, bind, std::iter::once(Value::Null), &mut out);
                } else {
                    append_rows(row, bind, items.into_iter(), &mut out);
                }
            }
            Value::TypedMap(items) => {
                append_rows(
                    row,
                    bind,
                    items.into_iter().map(|(key, value)| {
                        Value::Map(BTreeMap::from([
                            ("key".into(), key),
                            ("value".into(), value),
                        ]))
                    }),
                    &mut out,
                );
            }
            Value::Map(items) => {
                append_rows(
                    row,
                    bind,
                    items.into_iter().map(|(key, value)| {
                        Value::Map(BTreeMap::from([
                            ("key".into(), Value::String(key)),
                            ("value".into(), value),
                        ]))
                    }),
                    &mut out,
                );
            }
            Value::Null if !outer_flag => {}
            other => append_rows(row, bind, std::iter::once(other), &mut out),
        }
    }
    Ok(out)
}

fn append_rows(
    mut row: Row,
    bind: &str,
    mut values: impl ExactSizeIterator<Item = Value>,
    out: &mut Vec<Row>,
) {
    while let Some(value) = values.next() {
        // Move the original row into the final result. Single-result inputs
        // therefore never clone the surviving bindings.
        if values.len() == 0 {
            row.bindings.insert(bind.to_string(), value);
            out.push(row);
            break;
        }
        let mut next = row.clone();
        next.bindings.insert(bind.to_string(), value);
        out.push(next);
    }
}
