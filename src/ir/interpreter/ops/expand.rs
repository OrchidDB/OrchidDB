//! Pattern expansion: bind, expand, endpoint label matches.
//!
//! Extracted from `interpreter.rs` lines 541..711.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{BindKind, Direction, LabelExpr, Length, TargetMode};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::run::ExecutionContext;
use super::super::{InterpretError, IrResult, Row};

pub(crate) fn bind_op(
    bind: &str,
    _kind: BindKind,
    expr: Option<&IrExpr>,
    upstream: Vec<Row>,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(upstream.len());
    for mut row in upstream {
        if let Some(expr) = expr {
            let value = eval(expr, &row, &PropertyGraph::default())?;
            row.bindings.insert(bind.to_string(), value);
        }
        // No-op when expr is None: upstream produced a binding under some
        // name; `Bind` is a metadata acknowledgement.
        out.push(row);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn expand_op(
    source: &str,
    target: &str,
    target_mode: TargetMode,
    target_labels: &LabelExpr,
    rel_binding: Option<&str>,
    rel_types: &LabelExpr,
    dir: Direction,
    length: &Length,
    history_binding: Option<&str>,
    path_binding: Option<&str>,
    upstream: Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    // The IR allows `[..]` (unbounded upper) but a graph in the
    // conformance corpus terminates BFS at the natural frontier when
    // `DifferentRelationships` keeps each edge from being reused. Cap
    // the loop at a conservative depth so the interpreter still has a
    // hard exit if the data picks up unexpected cycles.
    const UNBOUNDED_MAX: u32 = 64;
    let max_hops = length.max.unwrap_or(UNBOUNDED_MAX);
    if length.min > max_hops {
        return Err(InterpretError::Unsupported(format!(
            "expand length {}..{} invalid",
            length.min, max_hops
        )));
    }
    let rel_filter = match rel_types {
        LabelExpr::Any => Vec::new(),
        LabelExpr::AnyOf(names) | LabelExpr::AllOf(names) => names.clone(),
        LabelExpr::Not(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for row in upstream {
        ctx.charge(1)?;
        let source_node = match row.bindings.get(source) {
            Some(Value::Node { label, id }) => (label.clone(), *id),
            _ => continue,
        };
        // BFS up to the bounded max, emitting rows whose hop count is in
        // [length.min, max_hops]. `history` carries Cypher relationship
        // uniqueness across a whole graph pattern; `path` carries the
        // user-visible materialized path for this expansion chain.
        let initial_history = history_binding
            .and_then(|binding| row.bindings.get(binding))
            .and_then(|value| match value {
                Value::Path(items) => Some(items.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let initial_path = path_binding
            .and_then(|binding| row.bindings.get(binding))
            .and_then(|value| match value {
                Value::Path(items) => Some(items.clone()),
                _ => None,
            })
            .unwrap_or_else(|| {
                vec![Value::Node {
                    label: source_node.0.clone(),
                    id: source_node.1,
                }]
            });
        let mut frontier: Vec<(String, i64, Vec<Value>, Vec<Value>)> = vec![(
            source_node.0.clone(),
            source_node.1,
            initial_history,
            initial_path,
        )];
        if length.min == 0 && label_matches(&source_node.0, target_labels) {
            let emit = match target_mode {
                TargetMode::Existing => match row.bindings.get(target) {
                    Some(Value::Node { label, id }) => {
                        label == &source_node.0 && *id == source_node.1
                    }
                    _ => false,
                },
                _ => true,
            };
            if emit {
                let mut out_row = row.clone();
                if !matches!(target_mode, TargetMode::Existing) {
                    out_row.bindings.insert(
                        target.to_string(),
                        Value::Node {
                            label: source_node.0.clone(),
                            id: source_node.1,
                        },
                    );
                }
                if let Some(p) = path_binding {
                    out_row
                        .bindings
                        .insert(p.to_string(), Value::Path(frontier[0].3.clone()));
                }
                if let Some(h) = history_binding {
                    out_row
                        .bindings
                        .insert(h.to_string(), Value::Path(frontier[0].2.clone()));
                }
                ctx.charge(1)?;
                out.push(out_row);
            }
        }
        for hop in 1..=max_hops {
            let mut next_frontier = Vec::new();
            for (cur_label, cur_id, history_so_far, path_so_far) in frontier {
                ctx.charge(1)?;
                let edges = match dir {
                    Direction::Out => graph.out_edges(&cur_label, cur_id, &rel_filter),
                    Direction::In => graph.in_edges(&cur_label, cur_id, &rel_filter),
                    Direction::Both => {
                        let mut both = graph.out_edges(&cur_label, cur_id, &rel_filter);
                        both.extend(graph.in_edges(&cur_label, cur_id, &rel_filter));
                        both.sort();
                        both.dedup();
                        both
                    }
                };
                for (rel_type, edge_row, other_label, other_id) in edges {
                    ctx.charge(1)?;
                    let mut history = history_so_far.clone();
                    let mut path = path_so_far.clone();
                    let (sl, sid, dl, did) = graph
                        .edge_endpoints(&rel_type, edge_row)
                        .expect("endpoints");
                    let edge_value = Value::Edge {
                        rel_type: rel_type.clone(),
                        id: edge_row,
                        src_label: sl,
                        src_id: sid,
                        dst_label: dl,
                        dst_id: did,
                    };
                    let target_node = Value::Node {
                        label: other_label.clone(),
                        id: other_id,
                    };
                    let enforces_trail = history_binding.is_some()
                        || path_binding.is_some_and(|binding| binding != "__path");
                    let history_contains = history_binding
                        .map(|_| path_contains_edge(&history, &rel_type, edge_row))
                        .unwrap_or_else(|| path_contains_edge(&path, &rel_type, edge_row));
                    if enforces_trail && history_contains {
                        continue;
                    }
                    if history_binding.is_some() {
                        history.push(edge_value.clone());
                    }
                    let include_edge_in_path = rel_binding.is_some() && target == "__edge_other";
                    if path_binding.is_some_and(|binding| binding != "__path") {
                        path.push(edge_value.clone());
                        path.push(target_node.clone());
                    } else if include_edge_in_path {
                        path.push(edge_value.clone());
                    } else {
                        path.push(target_node.clone());
                    }
                    if hop >= length.min {
                        // Target label filter.
                        if !label_matches(&other_label, target_labels) {
                            // Still extend frontier; just don't emit.
                        } else {
                            // For `Existing` mode, the row must already
                            // bind `target` and match it.
                            let emit = match target_mode {
                                TargetMode::Existing => match row.bindings.get(target) {
                                    Some(Value::Node { label: tl, id: tid }) => {
                                        tl == &other_label && *tid == other_id
                                    }
                                    _ => false,
                                },
                                _ => true,
                            };
                            if emit {
                                let mut out_row = row.clone();
                                if !matches!(target_mode, TargetMode::Existing) {
                                    out_row
                                        .bindings
                                        .insert(target.to_string(), target_node.clone());
                                }
                                if let Some(binding) = rel_binding {
                                    out_row
                                        .bindings
                                        .insert(binding.to_string(), edge_value.clone());
                                }
                                if let Some(p) = path_binding {
                                    out_row
                                        .bindings
                                        .insert(p.to_string(), Value::Path(path.clone()));
                                }
                                if let Some(h) = history_binding {
                                    out_row
                                        .bindings
                                        .insert(h.to_string(), Value::Path(history.clone()));
                                }
                                ctx.charge(1)?;
                                out.push(out_row);
                            }
                        }
                    }
                    ctx.charge(1)?;
                    next_frontier.push((other_label, other_id, history, path));
                }
            }
            frontier = next_frontier;
            if frontier.is_empty() {
                break;
            }
        }
    }
    Ok(out)
}

pub(crate) fn label_matches(label: &str, expr: &LabelExpr) -> bool {
    match expr {
        LabelExpr::Any => true,
        LabelExpr::AnyOf(names) => names.iter().any(|n| n == label),
        LabelExpr::AllOf(names) => names.len() == 1 && names[0] == label,
        LabelExpr::Not(inner) => !label_matches(label, inner),
    }
}

fn path_contains_edge(path: &[Value], rel_type: &str, id: i64) -> bool {
    path.iter().any(|value| {
        matches!(
            value,
            Value::Edge {
                rel_type: existing_type,
                id: existing_id,
                ..
            } if existing_type == rel_type && *existing_id == id
        )
    })
}
