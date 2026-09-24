//! GraphCoalesce.
//!
//! Extracted from `interpreter.rs` lines 1730..1757.

use super::super::run::ExecutionContext;
use super::super::{IrResult, Row};
use super::repeat::run_with_outer;
use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{CoalesceSuccess, Node};

pub(crate) fn coalesce_op(
    success: CoalesceSuccess,
    output: &str,
    correlation: &[String],
    rows: Vec<Row>,
    arms: &[Node],
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let _ = success;
    let _ = correlation;
    let mut out = Vec::new();
    for row in rows {
        for arm in arms {
            let arm_rows = run_with_outer(arm, &row, graph, ctx)?;
            if !arm_rows.is_empty() {
                for arm_row in arm_rows {
                    let mut new_row = row.clone();
                    if let Some(value) = arm_row.bindings.get(output) {
                        new_row.bindings.insert(output.to_string(), value.clone());
                    }
                    for (binding, value) in &arm_row.bindings {
                        if binding == "__path"
                            || binding.starts_with("__gremlin_select_history_")
                            || arm_row
                                .bindings
                                .contains_key(&format!("__gremlin_select_history_{binding}"))
                        {
                            new_row.bindings.insert(binding.clone(), value.clone());
                        }
                    }
                    new_row.bulk = arm_row.bulk;
                    out.push(new_row);
                }
                break;
            }
        }
    }
    Ok(out)
}
