//! GraphApply (correlated subquery).
//!
//! Extracted from `interpreter.rs` lines 1575..1676.

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{ApplyKind, Node};
use crate::ir::policy::OptionalMissing;
use crate::ir::value::Value;

use super::super::run::ExecutionContext;
use super::super::{InterpretError, IrResult, Row};
use super::repeat::run_with_outer;

pub(crate) fn apply_op(
    kind: ApplyKind,
    correlation: &[String],
    outputs: &[String],
    optional_missing: OptionalMissing,
    outer: Vec<Row>,
    right: &Node,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let _ = correlation;
    let mut out = Vec::new();
    for outer_row in outer {
        ctx.charge(1)?;
        // A correlated child runs on a split representing one traverser.
        // Parent bulk weights the returned results, not child reducers or
        // side effects (TraversalUtil.prepare resets the split bulk to one).
        let mut probe = outer_row.clone();
        probe.bulk = 1;
        let inner_rows = run_with_outer(right, &probe, graph, ctx)?;

        ctx.charge(inner_rows.len() as u64)?;
        match kind {
            ApplyKind::Inner => {
                for inner in inner_rows {
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    let mut compatible = true;
                    for (binding, value) in &inner.bindings {
                        if binding == "current" && !outputs.iter().any(|output| output == binding) {
                            continue;
                        }
                        if (binding == "__path" || binding == "__path_labels" || binding == "__sack")
                            || binding.starts_with("__gremlin_select_history_")
                        {
                            if outputs.iter().any(|output| output == "current") {
                                row.bindings.insert(binding.clone(), value.clone());
                            }
                            continue;
                        }
                        // A path processor clears both the label and its
                        // history. This removes child scope, rather than
                        // imposing a null equality join on the parent.
                        // Productive null labels retain a list history.
                        let retracted = matches!(value, Value::Null)
                            && inner.bindings.get(&format!("__gremlin_select_history_{binding}"))
                                == Some(&Value::Null);
                        if retracted {
                            if outputs.iter().any(|output| output == "current") {
                                row.bindings.insert(binding.clone(), Value::Null);
                            }
                            continue;
                        }
                        if binding != "current" {
                            if row.bindings.contains_key(binding) {
                                if is_cypher_history_binding(binding) {
                                    row.bindings.insert(binding.clone(), value.clone());
                                    continue;
                                }
                                if is_internal_apply_binding(binding) {
                                    continue;
                                }
                                let existing = row
                                    .bindings
                                    .get(binding)
                                    .expect("binding existed when compatibility was checked");
                                if existing.three_valued_eq(value) != Some(true) {
                                    compatible = false;
                                    break;
                                }
                            }
                        }
                        row.bindings.insert(binding.clone(), value.clone());
                    }
                    if compatible {
                        row.bulk = outer_row.bulk.checked_mul(inner.bulk).ok_or_else(|| {
                            InterpretError::Runtime("correlated traverser bulk overflow".into())
                        })?;
                        ctx.charge(1)?;
                        out.push(row);
                    }
                }
            }
            ApplyKind::Optional => {
                if inner_rows.is_empty() {
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    let placeholder = match optional_missing {
                        OptionalMissing::Null => Value::Null,
                        OptionalMissing::Unbound => Value::Null,
                    };
                    for binding in outputs {
                        row.bindings.insert(binding.clone(), placeholder.clone());
                    }
                    out.push(row);
                } else {
                    for inner in inner_rows {
                        ctx.charge(1)?;
                        let mut row = outer_row.clone();
                        row.bulk = outer_row.bulk.checked_mul(inner.bulk).ok_or_else(|| {
                            InterpretError::Runtime("correlated traverser bulk overflow".into())
                        })?;
                        for binding in outputs {
                            row.bindings.insert(
                                binding.clone(),
                                inner.bindings.get(binding).cloned().unwrap_or(Value::Null),
                            );
                        }
                        for (binding, value) in &inner.bindings {
                            if outputs.iter().any(|output| output == "current")
                                && ((binding == "__path" || binding == "__path_labels" || binding == "__sack")
                                    || binding.starts_with("__gremlin_select_history_")
                                    || inner.bindings.contains_key(&format!(
                                        "__gremlin_select_history_{binding}"
                                    )))
                            {
                                row.bindings.insert(binding.clone(), value.clone());
                            }
                        }
                        out.push(row);
                    }
                }
            }
            ApplyKind::Semi => {
                if !inner_rows.is_empty() {
                    ctx.charge(1)?;
                    out.push(outer_row);
                }
            }
            ApplyKind::Anti => {
                if inner_rows.is_empty() {
                    ctx.charge(1)?;
                    out.push(outer_row);
                }
            }
            ApplyKind::Scalar => {
                if inner_rows.len() != 1 {
                    if outputs.is_empty() {
                        return Err(InterpretError::Type(
                            "scalar apply produced no output bindings".into(),
                        ));
                    }
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    let value = inner_rows
                        .first()
                        .and_then(|r| r.bindings.get(&outputs[0]))
                        .cloned()
                        .unwrap_or(Value::Null);
                    for binding in outputs {
                        row.bindings.insert(binding.clone(), value.clone());
                    }
                    out.push(row);
                } else {
                    let inner = &inner_rows[0];
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    for binding in outputs {
                        row.bindings.insert(
                            binding.clone(),
                            inner.bindings.get(binding).cloned().unwrap_or(Value::Null),
                        );
                    }
                    out.push(row);
                }
            }
        }
    }
    Ok(out)
}

fn is_internal_apply_binding(binding: &str) -> bool {
    binding.starts_with("__")
}

fn is_cypher_history_binding(binding: &str) -> bool {
    binding.starts_with("__cypher_match_history_")
        || binding.starts_with("__cypher_exists_history_")
        || binding.starts_with("__cypher_pattern_history_")
}
