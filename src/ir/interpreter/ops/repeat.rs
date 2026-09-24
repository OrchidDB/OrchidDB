//! GraphRepeat — frontier semantics, emit, until, run-with-outer.
//!
//! Extracted from `interpreter.rs` lines 713..1035.

use std::collections::BTreeSet;

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
    until_first: bool,
    until: Option<&IrExpr>,
    until_traversal: Option<&Node>,
    _path: Option<&str>,
    seed_rows: Vec<Row>,
    body: &Node,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let nested = !ctx.step_state.is_empty();
    let seed_rows = if nested {
        seed_rows.into_iter().map(|mut row| {
            let mut stack = match row.bindings.remove("__repeat_loop_stack") {
                Some(Value::List(stack)) => stack, _ => Vec::new(),
            };
            let mut saved = std::collections::BTreeMap::new();
            for key in std::iter::once("__loops".to_string()).chain(loop_name.map(|name| format!("__loops:{name}"))) {
                saved.insert(key.clone(), row.bindings.get(&key).cloned().unwrap_or(Value::Null));
            }
            stack.push(Value::Map(saved));
            row.bindings.insert("__repeat_loop_stack".into(), Value::List(stack));
            row
        }).collect()
    } else { seed_rows };
    ctx.push_step_state_frame();
    let mut result = repeat_op_inner(
        loop_name,
        times,
        emit_each_iteration,
        emit_mode,
        emit_seed_predicate,
        emit_seed_traversal,
        until_first,
        until,
        until_traversal,
        _path,
        seed_rows,
        body,
        graph,
        ctx,
    );
    ctx.pop_step_state_frame();
    if nested {
        if let Ok(rows) = &mut result {
            for row in rows {
                if let Some(Value::List(mut stack)) = row.bindings.remove("__repeat_loop_stack") {
                    if let Some(Value::Map(saved)) = stack.pop() {
                        for (key, value) in saved {
                            if matches!(value, Value::Null) { row.bindings.remove(&key); }
                            else { row.bindings.insert(key, value); }
                        }
                    }
                    if !stack.is_empty() { row.bindings.insert("__repeat_loop_stack".into(), Value::List(stack)); }
                }
            }
        }
    }
    result
}

fn has_observable_state(root: &Node) -> bool {
    use crate::ir::analysis::{Effect, children, node_effect};
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node_effect(node) != Effect::Pure { return true; }
        pending.extend(children(node));
    }
    false
}

fn repeat_op_inner(
    loop_name: Option<&str>,
    times: Option<u32>,
    emit_each_iteration: bool,
    emit_mode: &EmitMode,
    emit_seed_predicate: Option<&IrExpr>,
    emit_seed_traversal: Option<&Node>,
    until_first: bool,
    until: Option<&IrExpr>,
    until_traversal: Option<&Node>,
    _path: Option<&str>,
    seed_rows: Vec<Row>,
    body: &Node,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    // Standard RepeatStep supplies one upstream seed only after its body
    // has drained. When an unbounded body reads/writes shared state, later
    // seeds must observe the earlier seed's completed work. The source has
    // already run once; retain each seed's bulk and the shared step frame.
    if seed_rows.len() > 1 && times.is_none() && !emit_each_iteration
        && emit_seed_predicate.is_none() && emit_seed_traversal.is_none()
        && until.is_none() && until_traversal.is_none() && has_observable_state(body)
    {
        let mut rows = Vec::new();
        for seed in seed_rows {
            rows.extend(repeat_op_inner(loop_name, times, emit_each_iteration,
                emit_mode, emit_seed_predicate, emit_seed_traversal, until_first,
                until, until_traversal, _path, vec![seed], body, graph, ctx)?);
        }
        return Ok(rows);
    }
    // Termination, in priority order:
    //   1. `times = Some(N)` — at most N iterations.
    //   2. `until = Some(p)` — stop when p matches a row (that row is
    //      emitted, others continue).
    //   3. Otherwise — stop when the frontier becomes empty.
    // A resource ceiling is an error, never a successful truncated result.
    const MAX_REPEAT_ITERATIONS: u32 = 10_000;
    let trace_metrics = ctx.step_state.len() == 1 && std::env::var_os("CRABGRAPH_REPEAT_METRICS").is_some();
    let mut peak_expanded = seed_rows.len();
    let mut frontier = super::barrier::compact_repeat_frontier(seed_rows)?;
    let mut peak_compacted = frontier.len();
    let mut peak_bulk = 0_u64;
    let mut out = Vec::new();
    for row in &mut frontier {
        row.bindings.insert("__loops".into(), Value::Int(0));
        if let Some(name) = loop_name { row.bindings.insert(format!("__loops:{name}"), Value::Int(0)); }
    }
    if until_first {
        let mut continuing = Vec::new();
        for row in frontier {
            let done = if let Some(predicate) = until {
                matches!(eval(predicate, &row, graph)?, Value::Bool(true))
            } else if let Some(probe) = until_traversal {
                !run_body_with_frontier(probe, vec![row.clone()], graph, ctx)?.is_empty()
            } else { false };
            if done { out.push(row); } else { continuing.push(row); }
        }
        frontier = continuing;
    }
    if emit_each_iteration {
        if let Some(seed_predicate) = emit_seed_predicate {
            emit_matching(&frontier, Some(seed_predicate), &mut out, graph)?;
        } else if let Some(seed_probe) = emit_seed_traversal {
            emit_matching_traversal(&frontier, seed_probe, &mut out, graph, ctx)?;
        }
    }
    let mut iteration: u32 = 0;
    loop {
        if frontier.is_empty() { break; }
        ctx.charge(1)?;
        if let Some(n) = times {
            if iteration >= n {
                break;
            }
        }
        if iteration >= MAX_REPEAT_ITERATIONS {
            return Err(super::super::InterpretError::ExecutionLimit(format!(
                "repeat exceeded {MAX_REPEAT_ITERATIONS} iterations"
            )));
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
        peak_expanded = peak_expanded.max(stepped.len());
        let stepped = super::barrier::compact_repeat_frontier(stepped)?;
        peak_compacted = peak_compacted.max(stepped.len());
        if trace_metrics {
            peak_bulk = peak_bulk.max(stepped.iter().fold(0_u64, |total, row| total.saturating_add(row.bulk)));
        }
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
        // Until/times exits take precedence over emit splitting: a terminal
        // traverser is returned once even when the emit predicate rejects it.
        let at_bound = times.is_some_and(|n| iteration + 1 >= n);
        let mut continuing = Vec::new();
        for row in stepped {
            let done = if at_bound {
                true
            } else if let Some(predicate) = until {
                matches!(eval(predicate, &row, graph)?, Value::Bool(true))
            } else if let Some(probe) = until_traversal {
                !run_body_with_frontier(probe, vec![row.clone()], graph, ctx)?.is_empty()
            } else {
                false
            };
            if done { out.push(row); } else { continuing.push(row); }
        }
        if emit_each_iteration {
            match emit_mode {
                EmitMode::AfterEachIteration => emit_matching(&continuing, None, &mut out, graph)?,
                EmitMode::AfterEachIfPredicate(p) => emit_matching(&continuing, Some(p), &mut out, graph)?,
                EmitMode::AfterEachIfTraversal(probe) => emit_matching_traversal(&continuing, probe, &mut out, graph, ctx)?,
                EmitMode::AfterLoop => {}
            }
        }
        frontier = continuing;
        if frontier.is_empty() { break; }
        iteration += 1;
    }
    if !emit_each_iteration && until.is_none() && until_traversal.is_none() {
        out.extend(frontier);
    }
    if trace_metrics {
        eprintln!("repeat metrics: iterations={} peak_expanded={} peak_compacted={} peak_logical_bulk={}", iteration + 1, peak_expanded, peak_compacted, peak_bulk);
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
            if bindings == &["__gremlin_group_members"] {
                return Ok(frontier.to_vec());
            }
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
                for implicit in ["__path", "__path_labels", "__edge_other", "__loops", "__sack", "__sack_merge", "__bulk_enabled", "__gremlin_bulk_safe", "__repeat_loop_stack"].iter() {
                    if !row.bindings.contains_key(*implicit) {
                        if let Some(value) = outer.bindings.get(*implicit) {
                            row.bindings.insert((*implicit).to_string(), value.clone());
                        }
                    }
                }
                for (binding, value) in &outer.bindings {
                    if (binding.starts_with("__loops:") || binding.starts_with("__gremlin_select_history_")) && !row.bindings.contains_key(binding) {
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
            group_map_op(key, value, output, rows, graph, ctx)
        }
        Node::GraphShortestPath { input, .. } => run_with_frontier(input, frontier, graph, ctx),
        Node::GraphSort { keys, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            sort_op(keys, rows, graph)
        }
        Node::GraphSample { kind, seed, step_id, weight, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            let rng = ctx.random_steps.entry(step_id.clone())
                .or_insert_with(|| super::sample::JavaRandom::new(*seed));
            super::sample::sample_op(*kind, weight.as_ref(), rows, graph, rng)
        }
        Node::GraphSlice { slice, input } => {
            if let Node::GraphSideEffect { label, value_input, value, seed, reducer, eager: false, input: source } = input.as_ref() {
                if let Some(fetch) = slice.fetch.filter(|_| slice.tail.is_none()) {
                    let rows = run_with_frontier(source, frontier, graph, ctx)?;
                    let consumed = slice_op(&crate::ir::plan::Slice { offset: 0, fetch: Some(slice.offset.saturating_add(fetch)), tail: None }, rows)?;
                    let rows = ctx.write_side_effect(label, value_input, value, seed, reducer, false, consumed, graph)?;
                    return slice_op(slice, rows);
                }
            }
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
            until_first,
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
                *until_first,
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
        Node::GraphGroupSideEffect { label, key, value, key_input, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            super::aggregate::group_side_effect_write(ctx, label, key, value, key_input, &rows, graph)?;
            Ok(rows)
        }
        Node::GraphGroupCountSideEffect { label, key, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            let counts = ctx.group_counts.entry(label.clone()).or_default();
            for row in &rows {
                let key_value = eval(key, row, graph)?;
                if let Some((_, count)) = counts.iter_mut().find(|(key, _)| key == &key_value) {
                    *count += row.bulk;
                } else { counts.push((key_value, row.bulk)); }
            }
            Ok(rows)
        }
        Node::GraphSideEffect { label, value_input, value, seed, reducer, eager, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            ctx.write_side_effect(label, value_input, value, seed, reducer, *eager, rows, graph)
        }
        Node::GraphReadSideEffect { label, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            ctx.read_side_effect(label, rows, graph)
        }
        Node::GraphCap { labels, input } => {
            let _ = run_with_frontier(input, frontier, graph, ctx)?;
            ctx.cap_side_effects(labels, graph)
        }
        Node::GraphBarrier { partition, order, slice, materialize, bulk_policy, input } => {
            let rows = run_with_frontier(input, frontier, graph, ctx)?;
            super::barrier::barrier_op(partition, order, slice, *materialize, *bulk_policy, rows, graph)
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
        Node::GraphProcedureCall {name,args,yields,input,..} => {
            let rows=match input {Some(input)=>run_with_frontier(input,frontier,graph,ctx)?,None=>vec![Row::new()]};
            super::super::run::procedure_call_op(name,args,yields,rows,graph)
        }
        // Sources without correlation behave normally.
        other => run_with_context(other, graph, ctx),
    }
}
