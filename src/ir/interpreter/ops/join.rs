//! GraphJoin.
//!
//! Extracted from `interpreter.rs` lines 1547..1573.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};

pub(crate) fn join_op(
    left: Vec<Row>,
    right: Vec<Row>,
    condition: Option<&IrExpr>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for l in &left {
        for r in &right {
            let mut combined = l.clone();
            for (k, v) in &r.bindings {
                combined
                    .bindings
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
            let pass = match condition {
                None => true,
                Some(expr) => matches!(eval(expr, &combined, graph)?, Value::Bool(true)),
            };
            if pass {
                out.push(combined);
            }
        }
    }
    Ok(out)
}
