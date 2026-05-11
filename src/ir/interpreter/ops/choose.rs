//! GraphChoose.
//!
//! Extracted from `interpreter.rs` lines 1759..1804.

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{ChooseArm, ChooseSelector, Node};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::run::ExecutionContext;
use super::super::{IrResult, Row};
use super::repeat::run_with_outer;

pub(crate) fn choose_op(
    selector: &ChooseSelector,
    correlation: &[String],
    rows: Vec<Row>,
    arms: &[ChooseArm],
    default: Option<&Node>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let _ = correlation;
    let mut out = Vec::new();
    for row in rows {
        let pick: Option<&Node> = match selector {
            ChooseSelector::Boolean(condition) => {
                let cond = eval(condition, &row, graph)?;
                let idx = if matches!(cond, Value::Bool(true)) {
                    0
                } else {
                    1
                };
                arms.get(idx).map(|arm| &arm.body).or(default)
            }
            ChooseSelector::Value(expr) => {
                let value = eval(expr, &row, graph)?;
                arms.iter()
                    .find(|arm| arm.key.as_ref() == Some(&value))
                    .map(|arm| &arm.body)
                    .or(default)
            }
        };
        let Some(arm) = pick else {
            // No matching arm and no default — pass-through (the default
            // `unmatched` policy emitted by lowering).
            out.push(row);
            continue;
        };
        let arm_rows = run_with_outer(arm, &row, graph, ctx)?;
        for arm_row in arm_rows {
            let mut new_row = row.clone();
            for (k, v) in arm_row.bindings {
                new_row.bindings.insert(k, v);
            }
            out.push(new_row);
        }
    }
    Ok(out)
}
