//! GraphChoose.
//!
//! Extracted from `interpreter.rs` lines 1759..1804.

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{ChooseArm, ChooseSelector, Node};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::run::ExecutionContext;
use super::super::{IrResult, Row};
use super::repeat::{run_body_with_frontier, run_with_outer};

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
    if let ChooseSelector::Predicates(conditions) = selector {
        // BranchStep processes one incoming traverser at a time unless an
        // option contains a barrier. Partitioning nonbarrier options first
        // reorders outputs (and their observable side effects) by option.
        if !arms.iter().any(|arm| contains_barrier(&arm.body))
            && default.is_none_or(|body| !contains_barrier(body))
        {
            let mut out = Vec::new();
            for row in rows {
                let mut matched = false;
                for (condition, arm) in conditions.iter().zip(arms) {
                    if matches!(eval(condition, &row, graph)?, Value::Bool(true)) {
                        out.extend(run_with_outer(&arm.body, &row, graph, ctx)?);
                        matched = true;
                    }
                }
                if !matched {
                    if let Some(default) = default {
                        out.extend(run_with_outer(default, &row, graph, ctx)?);
                    }
                }
            }
            return Ok(out);
        }
        let mut streams = vec![Vec::new(); arms.len()];
        let mut unmatched = Vec::new();
        for row in rows {
            let mut matched = false;
            for (index, condition) in conditions.iter().take(arms.len()).enumerate() {
                if matches!(eval(condition, &row, graph)?, Value::Bool(true)) {
                    streams[index].push(row.clone());
                    matched = true;
                }
            }
            if !matched {
                unmatched.push(row);
            }
        }
        let mut out = Vec::new();
        for (arm, stream) in arms.iter().zip(streams) {
            if !stream.is_empty() {
                out.extend(run_body_with_frontier(&arm.body, stream, graph, ctx)?);
            }
        }
        if let Some(default) = default {
            if !unmatched.is_empty() {
                out.extend(run_body_with_frontier(default, unmatched, graph, ctx)?);
            }
        }
        return Ok(out);
    }
    let mut out = Vec::new();
    for row in rows {
        let pick: Option<&Node> = match selector {
            ChooseSelector::Predicates(_) => unreachable!("stream selector handled above"),
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
            new_row.bulk = arm_row.bulk;
            for (k, v) in arm_row.bindings {
                new_row.bindings.insert(k, v);
            }
            out.push(new_row);
        }
    }
    Ok(out)
}

pub(crate) fn contains_barrier(node: &Node) -> bool {
    matches!(node,
        Node::GraphAggregate { .. } | Node::GraphGroupMap { .. }
        | Node::GraphDistinct { .. } | Node::GraphSort { .. }
        | Node::GraphSlice { .. } | Node::GraphSliceExpr { .. }
        | Node::GraphBarrier { .. } | Node::GraphCap { .. }
        | Node::GraphSideEffect { eager: true, .. }
        | Node::GraphSample { kind: crate::ir::plan::SampleKind::Global(_), .. }
    ) || crate::ir::analysis::children(node).into_iter().any(contains_barrier)
}
