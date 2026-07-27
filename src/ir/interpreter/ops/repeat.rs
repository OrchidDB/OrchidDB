//! GraphRepeat — frontier semantics, emit, until, run-with-outer.
//!
//! Extracted from `interpreter.rs` lines 713..1035.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{EmitMode, Node};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::run::{ExecutionContext, run_with_context};
use super::super::{IrResult, Row};
use super::aggregate::{aggregate_op, group_map_op};
use super::apply::apply_op;
use super::choose::choose_op;
use super::coalesce::coalesce_op;
use super::distinct::{distinct_op, distinct_op_with_seen, row_signature};
use super::expand::{bind_op, expand_op};
use super::project::{current_project_op, project_op};
use super::quantifier::quantifier_op;
use super::select::select_op;
use super::slice::{slice_expr_op, slice_op};
use super::sort::sort_op;
use super::source::values_op;
use super::unwind::unwind_op;

pub(crate) fn repeat_op(
    loop_name: Option<&str>,
    times: Option<u32>,
    emit_each_iteration: bool,
    emit_mode: &EmitMode,
    emit_seed_predicate: Option<&IrExpr>,
    emit_seed_traversal: Option<&Node>,
    until: Option<&IrExpr>,
    until_traversal: Option<&Node>,
    _path: Option<&str>,
    seed_rows: Vec<Row>,
    body: &Node,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    ctx.push_step_state_frame();
    let result = repeat_op_inner(
        loop_name,
        times,
        emit_each_iteration,
        emit_mode,
        emit_seed_predicate,
        emit_seed_traversal,
        until,
        until_traversal,
        _path,
        seed_rows,
        body,
        graph,
        ctx,
    );
    ctx.pop_step_state_frame();
    result
}

fn repeat_op_inner(
    loop_name: Option<&str>,
    times: Option<u32>,
    emit_each_iteration: bool,
    emit_mode: &EmitMode,
    emit_seed_predicate: Option<&IrExpr>,
    emit_seed_traversal: Option<&Node>,
    until: Option<&IrExpr>,
    until_traversal: Option<&Node>,
    _path: Option<&str>,
    seed_rows: Vec<Row>,
    body: &Node,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    // Termination, in priority order:
    //   1. `times = Some(N)` — at most N iterations.
    //   2. `until = Some(p)` — stop when p matches a row (that row is
    //      emitted, others continue).
    //   3. Otherwise — stop when the frontier becomes empty.
    // Hard runtime cap: regardless of (1)/(2)/(3), no traversal runs
    // more than `MAX_REPEAT_ITERATIONS` iterations.
    const MAX_REPEAT_ITERATIONS: u32 = 16;
    let mut frontier = seed_rows;
    let mut out = Vec::new();
    if emit_each_iteration {
        if let Some(seed_predicate) = emit_seed_predicate {
            emit_matching(&frontier, Some(seed_predicate), &mut out, graph)?;
        } else if let Some(seed_probe) = emit_seed_traversal {
            emit_matching_traversal(&frontier, seed_probe, &mut out, graph, ctx)?;
        }
    }
    let mut iteration: u32 = 0;
    loop {
        ctx.charge(1)?;
        if iteration >= MAX_REPEAT_ITERATIONS {
            break;
        }
        if let Some(n) = times {
            if iteration >= n {
                break;
            }
        }
        let body_frontier = frontier
            .into_iter()
            .map(|mut row| {
                row.bindings
                    .insert("__loops".to_string(), Value::Int(iteration as i64));
                if let Some(name) = loop_name {
                    row.bindings
                        .insert(format!("__loops:{name}"), Value::Int(iteration as i64));
                }
                row
            })
            .collect::<Vec<_>>();
        ctx.activate_step_state_frame();
        let stepped = run_body_with_frontier(body, body_frontier, graph, ctx);
        ctx.deactivate_step_state_frame();
        let stepped = stepped?;
        let stepped = stepped
            .into_iter()
            .map(|mut row| {
                row.bindings
                    .insert("__loops".to_string(), Value::Int((iteration + 1) as i64));
                if let Some(name) = loop_name {
                    row.bindings.insert(
                        format!("__loops:{name}"),
                        Value::Int((iteration + 1) as i64),
                    );
                }
                row
            })
            .collect::<Vec<_>>();
        if emit_each_iteration {
            match emit_mode {
                EmitMode::AfterEachIteration => emit_matching(&stepped, None, &mut out, graph)?,
                EmitMode::AfterEachIfPredicate(p) => {
                    emit_matching(&stepped, Some(p), &mut out, graph)?
                }
                EmitMode::AfterEachIfTraversal(probe) => {
                    emit_matching_traversal(&stepped, probe, &mut out, graph, ctx)?
                }
                EmitMode::AfterLoop => {} // not active when emit_each_iteration
            }
        }
        if until.is_some() || until_traversal.is_some() {
            // When `until` matches, that row is emitted and not advanced.
            let mut matched = Vec::new();
            let mut continuing = Vec::new();
            for row in stepped {
                let done = if let Some(predicate) = until {
                    matches!(eval(predicate, &row, graph)?, Value::Bool(true))
                } else if let Some(probe) = until_traversal {
                    !run_body_with_frontier(probe, vec![row.clone()], graph, ctx)?.is_empty()
                } else {
                    false
                };
                if done {
                    matched.push(row);
                } else {
                    continuing.push(row);
                }
            }
            if !emit_each_iteration {
                out.extend(matched.clone());
            }
            frontier = continuing;
            if frontier.is_empty() {
                break;
            }
        } else {
            frontier = stepped;
            if frontier.is_empty() {
                break;
            }
        }
        iteration += 1;
    }
    if !emit_each_iteration && until.is_none() && until_traversal.is_none() {
        out.extend(frontier);
    }
    Ok(out)
}

pub(crate) fn emit_matching(
    rows: &[Row],
    predicate: Option<&IrExpr>,
    out: &mut Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<()> {
    for row in rows {
        if let Some(predicate) = predicate {
            if !matches!(eval(predicate, row, graph)?, Value::Bool(true)) {
                continue;
            }
        }
        out.push(row.clone());
    }
    Ok(())
}

/// Traversal-form emit: emit each row whose probe yields ≥1 result
/// when run with `[row]` as the upstream frontier. Implements
/// `repeat(...).emit(__.traversal)` per TinkerPop semantics.
pub(crate) fn emit_matching_traversal(
    rows: &[Row],
    probe: &Node,
    out: &mut Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<()> {
    for row in rows {
        let produced = run_body_with_frontier(probe, vec![row.clone()], graph, ctx)?;
        if !produced.is_empty() {
            out.push(row.clone());
        }
    }
    Ok(())
}

/// Run `body` with `frontier` as the upstream rows. We achieve this by
/// rewriting any leaf `Correlate { .. }` in `body` into a `Values` source
/// containing the frontier — but to keep things simple, we perform the
/// substitution at execution time while preserving whole-frontier barrier
/// semantics for each iteration.
pub(crate) fn run_body_with_frontier(
    body: &Node,
    frontier: Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let snippet = SubstitutedFrontier {
        inner: body,
        frontier: &frontier,
    };
    snippet.run(graph, ctx)
}

pub(crate) struct SubstitutedFrontier<'a> {
    inner: &'a Node,
    frontier: &'a [Row],
}

impl<'a> SubstitutedFrontier<'a> {
    fn run(&self, graph: &PropertyGraph, ctx: &mut ExecutionContext) -> IrResult<Vec<Row>> {
        // Replace the bottom-most `Correlate` in `inner` with `Values`
        // containing the frontier rows. We achieve this by execution, not
        // substitution: we evaluate `inner` while threading the row.
        run_with_frontier(self.inner, self.frontier, graph, ctx)
    }
}

/// Execute `node` with one outer row available at the bottom (Correlate).
pub(crate) fn run_with_outer(
    node: &Node,
    outer: &Row,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    run_with_frontier(node, std::slice::from_ref(outer), graph, ctx)
}

/// Execute `node` with the repeat frontier available at the bottom
/// (`Correlate`). Barrier steps in a repeat body, such as `limit`, `tail`, and
/// `order`, must operate over the whole current frontier for an iteration
/// rather than independently for each traverser.
pub(crate) fn run_with_frontier(
    node: &Node,
    frontier: &[Row],
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    ctx.charge(1)?;
    match node {
        Node::GraphCorrelate { bindings } => {
            let mut rows = Vec::with_capacity(frontier.len());
            for outer in frontier {
                let mut row = Row::new();
                for binding in bindings {
                    if let Some(value) = outer.bindings.get(binding) {
                        row.bindings.insert(binding.clone(), value.clone());
                    }
                }
                // Implicit history bindings (synthesized by `expand_op` and
                // friends) are not enumerated by the explicit correlation
                // list but must thread through the body so `path()` reflects
                // the full traverser history across loop iterations.
                for implicit in ["__path", "__edge_other", "__loops"].iter() {
                    if !row.bindings.contains_key(*implicit) {
                        if let Some(value) = outer.bindings.get(*implicit) {
                            row.bindings.insert((*implicit).to_string(), value.clone());
                        }
                    }
                }
                for (binding, value) in &outer.bindings {
                    if binding.starts_with("__loops:") && !row.bindings.contains_key(binding) {
                        row.bindings.insert(binding.clone(), value.clone());
                    }
                }
                row.bulk = outer.bulk;
                rows.push(row);
            }
            Ok(rows)
        }
        Node::GraphReturn { input, .. } => run_with_frontier(input, frontier, graph, ctx),
        Node::GraphFilter { condition, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            let mut out = Vec::new();
            for row in rows {
                match eval(condition, &row, graph)? {
                    Value::Bool(true) => out.push(row),
                    _ => {}
                }
            }
            Ok(out)
        }
        Node::GraphProject {
            mode, items, input, ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            project_op(*mode, items, rows, graph)
        }
        Node::GraphCurrentProject { expr, input, .. } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            current_project_op(expr, rows, graph)
        }
        Node::GraphBind {
            bind,
            kind,
            expr,
            input,
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            bind_op(bind, *kind, expr.as_ref(), rows)
        }
        Node::GraphExpand {
            source,
            target,
            target_mode,
            target_labels,
            rel_binding,
            rel_types,
            dir,
            length,
            history,
            path,
            path_mode,
            match_mode,
            input,
            ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            expand_op(
                source,
                target,
                *target_mode,
                target_labels,
                rel_binding.as_deref(),
                rel_types,
                *dir,
                length,
                history.as_deref(),
                path.as_deref(),
                *path_mode,
                *match_mode,
                rows,
                graph,
                ctx,
            )
        }
        Node::GraphAggregate {
            group, aggs, input, ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            aggregate_op(group, aggs, rows, graph)
        }
        Node::GraphGroupMap {
            key,
            value,
            output,
            input,
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            group_map_op(key, value, output, rows, graph)
        }
        Node::GraphShortestPath { input, .. } => run_with_frontier(input, frontier, graph, ctx),
        Node::GraphSort { keys, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            sort_op(keys, rows, graph)
        }
        Node::GraphSlice { slice, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            slice_op(slice, rows)
        }
        Node::GraphSliceExpr {
            offset,
            fetch,
            input,
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            slice_expr_op(offset.as_ref(), fetch.as_ref(), rows, graph)
        }
        Node::GraphDistinct {
            keys, mode, input, ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            if let Some(seen) = ctx.next_distinct_seen() {
                return distinct_op_with_seen(keys, *mode, rows, seen);
            }
            distinct_op(keys, *mode, rows)
        }
        Node::GraphPathFilter {
            condition, input, ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            Ok(rows
                .into_iter()
                .filter(|row| matches!(eval(condition, row, graph), Ok(Value::Bool(true))))
                .collect())
        }
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            select_op(labels, outputs, rows)
        }
        Node::GraphUnwind {
            input_expr,
            bind,
            outer: outer_flag,
            input,
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            unwind_op(input_expr, bind, *outer_flag, rows, graph)
        }
        Node::GraphApply {
            kind,
            correlation,
            outputs,
            optional_missing,
            left,
            right,
        } => {
            let outer_rows = run_with_frontier(left, frontier, graph, ctx)?;
            apply_op(
                *kind,
                correlation,
                outputs,
                *optional_missing,
                outer_rows,
                right,
                graph,
                ctx,
            )
        }
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => values_op(bindings, rows, bulk.as_deref()),
        Node::GraphOneRow => Ok(vec![Row::new()]),
        Node::GraphEmpty => Ok(vec![]),
        Node::GraphChoose {
            selector,
            correlation,
            arms,
            default,
            input,
            ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            choose_op(
                selector,
                correlation,
                rows,
                arms,
                default.as_deref(),
                graph,
                ctx,
            )
        }
        Node::GraphCoalesce {
            success,
            output,
            correlation,
            input,
            arms,
            ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            coalesce_op(*success, output, correlation, rows, arms, graph, ctx)
        }
        Node::GraphUnion {
            all, left, right, ..
        } => {
            let mut l = run_with_frontier(left, frontier, graph, ctx)?;
            let r = run_with_frontier(right, frontier, graph, ctx)?;
            l.extend(r);
            if !all {
                let mut seen = BTreeSet::new();
                l.retain(|row| {
                    let key = row_signature(row);
                    seen.insert(key)
                });
            }
            Ok(l)
        }
        Node::GraphRepeat {
            loop_name,
            times,
            emit,
            until,
            until_traversal,
            path,
            prefix_predicate,
            prefix_traversal,
            seed,
            body,
            ..
        } => {
            let seed_rows = run_with_frontier(seed, frontier, graph, ctx)?;
            let emit_each_iteration = !matches!(emit, EmitMode::AfterLoop);
            repeat_op(
                loop_name.as_deref(),
                *times,
                emit_each_iteration,
                emit,
                prefix_predicate.as_ref(),
                prefix_traversal.as_deref(),
                until.as_ref(),
                until_traversal.as_deref(),
                path.as_deref(),
                seed_rows,
                body,
                graph,
                ctx,
            )
        }
        Node::GraphQuantifier {
            kind,
            item_binding,
            input_expr,
            predicate,
            output,
            input,
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            quantifier_op(
                *kind,
                item_binding,
                input_expr,
                predicate,
                output,
                rows,
                graph,
            )
        }
        Node::GraphGroupCountSideEffect { label, key, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            let counts = ctx.group_counts.entry(label.clone()).or_default();
            for row in &rows {
                let key_value = eval(key, row, graph)?;
                let key = super::aggregate::map_key(&key_value);
                *counts.entry(key).or_insert(0) += row.bulk;
            }
            Ok(rows)
        }
        Node::GraphCap { labels, input } => {
            let _ = run_with_frontier(input, frontier, graph, ctx)?;
            if labels.len() == 1 {
                Ok(vec![
                    Row::new().with("current", group_count_map_value(ctx, &labels[0])),
                ])
            } else {
                let mut map = BTreeMap::new();
                for label in labels {
                    map.insert(label.clone(), group_count_map_value(ctx, label));
                }
                Ok(vec![Row::new().with("current", Value::Map(map))])
            }
        }
        // Mutations inside a correlated arm (MERGE's create arm, a
        // CREATE under an Apply) must see the outer bindings their
        // patterns reference.
        Node::GraphCreate {
            nodes,
            edges,
            input,
            ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            super::mutation::create_op(nodes, edges, rows, graph)
        }
        Node::GraphSetProperty { items, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            super::mutation::set_property_op(items, rows, graph)
        }
        Node::GraphDelete {
            targets,
            detach,
            input,
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            super::mutation::delete_op(targets, *detach, rows, graph)
        }
        Node::GraphMerge {
            outputs,
            input,
            match_arm,
            create_arm,
            ..
        } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            super::mutation::merge_op(outputs, rows, match_arm, create_arm, graph, ctx)
        }
        // Sources without correlation behave normally.
        other => run_with_context(other, graph, ctx),
    }
}

fn group_count_map_value(ctx: &ExecutionContext, label: &str) -> Value {
    let map = ctx
        .group_counts
        .get(label)
        .map(|counts| {
            counts
                .iter()
                .map(|(key, count)| (key.clone(), Value::String(format!("d[{count}].l"))))
                .collect()
        })
        .unwrap_or_default();
    Value::Map(map)
}
