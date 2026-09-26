//! GraphListComprehension — Cypher list comprehension as a node.
//!
//! Per upstream row, evaluate `input_expr` to a list, bind each item to
//! `item`, optionally filter, optionally map, and emit the resulting
//! list under `alias`. The original row is preserved.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};

pub(crate) fn list_comprehension_op(
    input_expr: &IrExpr,
    item: &str,
    filter: Option<&IrExpr>,
    map_expr: Option<&IrExpr>,
    alias: &str,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let source = eval(input_expr, &row, graph)?;
        let items = match source {
            Value::List(items) => items,
            Value::Null => {
                let mut new_row = row;
                new_row.bindings.insert(alias.to_string(), Value::Null);
                out.push(new_row);
                continue;
            }
            other => vec![other],
        };
        let mut collected = Vec::with_capacity(items.len());
        for value in items {
            let mut scratch = row.clone();
            scratch.bindings.insert(item.to_string(), value.clone());
            if let Some(filter) = filter {
                if !matches!(eval(filter, &scratch, graph)?, Value::Bool(true)) {
                    continue;
                }
            }
            let mapped = match map_expr {
                Some(expr) => eval(expr, &scratch, graph)?,
                None => value,
            };
            collected.push(mapped);
        }
        let mut new_row = row;
        new_row
            .bindings
            .insert(alias.to_string(), Value::List(collected));
        out.push(new_row);
    }
    Ok(out)
}
