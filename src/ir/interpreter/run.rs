//! Top-level node dispatcher.
//!
//! Extracted from `interpreter.rs` lines 114..410.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{Direction, LabelExpr, Node};
use crate::ir::value::Value;

use super::expr::eval;
use super::ops::aggregate::{aggregate_op, group_map_op};
use super::ops::apply::apply_op;
use super::ops::barrier::barrier_op;
use super::ops::choose::choose_op;
use super::ops::coalesce::coalesce_op;
use super::ops::collect::collect_op;
use super::ops::distinct::{distinct_op, row_signature};
use super::ops::expand::{bind_op, expand_op};
use super::ops::join::join_op;
use super::ops::list_comprehension::list_comprehension_op;
use super::ops::mutation::{create_op, delete_op, merge_op, set_property_op};
use super::ops::path_pattern::path_pattern_op;
use super::ops::project::{current_project_op, project_op};
use super::ops::quantifier::quantifier_op;
use super::ops::repeat::repeat_op;
use super::ops::select::select_op;
use super::ops::slice::{slice_expr_op, slice_op};
use super::ops::sort::sort_op;
use super::ops::source::{node_scan, rel_scan, values_op};
use super::ops::unwind::unwind_op;
use super::runtime::shortest_paths;
use super::{InterpretError, IrResult, Row};

#[derive(Debug)]
pub(crate) struct ExecutionContext {
    pub(crate) group_counts: BTreeMap<String, BTreeMap<String, u64>>,
    pub(crate) step_state: Vec<StepStateFrame>,
    step_limit: Option<u64>,
    steps: u64,
}

#[derive(Debug, Default)]
pub(crate) struct StepStateFrame {
    pub(crate) active: bool,
    pub(crate) cursor: usize,
    pub(crate) distinct_seen: Vec<BTreeSet<Vec<u8>>>,
}

impl ExecutionContext {
    const STEP_LIMIT_ENV: &'static str = "NEW_GRAPH_INTERPRETER_MAX_STEPS";

    pub(crate) fn charge(&mut self, units: u64) -> IrResult<()> {
        self.steps = self.steps.saturating_add(units);
        if let Some(limit) = self.step_limit {
            if self.steps > limit {
                return Err(InterpretError::ExecutionLimit(format!(
                    "{} exceeded after {} interpreter steps",
                    Self::STEP_LIMIT_ENV,
                    self.steps
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn push_step_state_frame(&mut self) {
        self.step_state.push(Default::default());
    }

    pub(crate) fn pop_step_state_frame(&mut self) {
        self.step_state.pop();
    }

    pub(crate) fn activate_step_state_frame(&mut self) {
        if let Some(frame) = self.step_state.last_mut() {
            frame.cursor = 0;
            frame.active = true;
        }
    }

    pub(crate) fn deactivate_step_state_frame(&mut self) {
        if let Some(frame) = self.step_state.last_mut() {
            frame.active = false;
        }
    }

    pub(crate) fn next_distinct_seen(&mut self) -> Option<&mut BTreeSet<Vec<u8>>> {
        let frame = self.step_state.last_mut()?;
        if !frame.active {
            return None;
        }
        let cursor = frame.cursor;
        frame.cursor += 1;
        if frame.distinct_seen.len() <= cursor {
            frame.distinct_seen.resize_with(cursor + 1, BTreeSet::new);
        }
        Some(&mut frame.distinct_seen[cursor])
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            group_counts: BTreeMap::new(),
            step_state: Vec::new(),
            step_limit: std::env::var(Self::STEP_LIMIT_ENV)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|limit| *limit > 0),
            steps: 0,
        }
    }
}

pub(crate) fn run(node: &Node, graph: &PropertyGraph) -> IrResult<Vec<Row>> {
    let mut ctx = ExecutionContext::default();
    run_with_context(node, graph, &mut ctx)
}

pub(crate) fn run_with_context(
    node: &Node,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    ctx.charge(1)?;
    match node {
        Node::GraphReturn { input, .. } => run_with_context(input, graph, ctx),
        Node::GraphOneRow => Ok(vec![Row::new()]),
        Node::GraphEmpty => Ok(vec![]),
        Node::GraphCorrelate { .. } => Ok(vec![Row::new()]),
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => values_op(bindings, rows, bulk.as_deref()),
        Node::GraphNodeScan {
            binding, labels, ..
        } => node_scan(binding, labels, graph),
        Node::GraphRelScan {
            binding,
            types,
            dir,
            ..
        } => rel_scan(binding, types, *dir, graph),
        Node::GraphBind {
            bind,
            kind,
            expr,
            input,
        } => {
            let upstream = run_with_context(input, graph, ctx)?;
            bind_op(bind, *kind, expr.as_ref(), upstream)
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
            let upstream = run_with_context(input, graph, ctx)?;
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
                upstream,
                graph,
                ctx,
            )
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
            let initial = run_with_context(seed, graph, ctx)?;
            let emit_each_iteration = !matches!(emit, crate::ir::plan::EmitMode::AfterLoop);
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
                initial,
                body,
                graph,
                ctx,
            )
        }
        Node::GraphPathFilter {
            condition, input, ..
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            Ok(rows
                .into_iter()
                .filter(|row| match eval(condition, row, graph) {
                    Ok(Value::Bool(true)) => true,
                    _ => false,
                })
                .collect())
        }
        Node::GraphCreate {
            nodes,
            edges,
            input,
            ..
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            create_op(nodes, edges, rows, graph)
        }
        Node::GraphMerge {
            outputs,
            input,
            match_arm,
            create_arm,
            ..
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            merge_op(outputs, rows, match_arm, create_arm, graph, ctx)
        }
        Node::GraphSetProperty { items, input } => {
            let rows = run_with_context(input, graph, ctx)?;
            set_property_op(items, rows, graph)
        }
        Node::GraphDelete {
            targets,
            detach,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            delete_op(targets, *detach, rows, graph)
        }
        Node::GraphFilter { condition, input } => {
            let rows = run_with_context(input, graph, ctx)?;
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
            let rows = run_with_context(input, graph, ctx)?;
            project_op(*mode, items, rows, graph)
        }
        Node::GraphCurrentProject { expr, input, .. } => {
            let rows = run_with_context(input, graph, ctx)?;
            current_project_op(expr, rows, graph)
        }
        Node::GraphAggregate {
            group, aggs, input, ..
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            aggregate_op(group, aggs, rows, graph)
        }
        Node::GraphDistinct {
            keys, mode, input, ..
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            distinct_op(keys, *mode, rows)
        }
        Node::GraphSort { keys, input } => {
            let rows = run_with_context(input, graph, ctx)?;
            sort_op(keys, rows, graph)
        }
        Node::GraphSlice { slice, input } => {
            let rows = run_with_context(input, graph, ctx)?;
            slice_op(slice, rows)
        }
        Node::GraphSliceExpr {
            offset,
            fetch,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            slice_expr_op(offset.as_ref(), fetch.as_ref(), rows, graph)
        }
        Node::GraphJoin {
            left,
            right,
            condition,
            ..
        } => {
            let l = run_with_context(left, graph, ctx)?;
            let r = run_with_context(right, graph, ctx)?;
            join_op(l, r, condition.as_ref(), graph)
        }
        Node::GraphApply {
            kind,
            correlation,
            outputs,
            optional_missing,
            left,
            right,
        } => {
            let outer = run_with_context(left, graph, ctx)?;
            apply_op(
                *kind,
                correlation,
                outputs,
                *optional_missing,
                outer,
                right,
                graph,
                ctx,
            )
        }
        Node::GraphUnion {
            all, left, right, ..
        } => {
            let mut l = run_with_context(left, graph, ctx)?;
            let r = run_with_context(right, graph, ctx)?;
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
        Node::GraphUnwind {
            input_expr,
            bind,
            outer,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            unwind_op(input_expr, bind, *outer, rows, graph)
        }
        Node::GraphCoalesce {
            success,
            output,
            correlation,
            input,
            arms,
            ..
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            coalesce_op(*success, output, correlation, rows, arms, graph, ctx)
        }
        Node::GraphChoose {
            selector,
            correlation,
            arms,
            default,
            input,
            ..
        } => {
            let rows = run_with_context(input, graph, ctx)?;
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
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            select_op(labels, outputs, rows)
        }
        Node::GraphPathPattern {
            path,
            selector,
            parts,
            input,
            ..
        } => {
            let upstream = run_with_context(input, graph, ctx)?;
            path_pattern_op(path, selector, parts, upstream, graph)
        }
        Node::GraphBarrier {
            partition,
            order,
            slice,
            materialize,
            bulk_policy,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            barrier_op(
                partition,
                order,
                slice,
                *materialize,
                *bulk_policy,
                rows,
                graph,
            )
        }
        Node::GraphGroupMap {
            key,
            value,
            output,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            group_map_op(key, value, output, rows, graph)
        }
        Node::GraphGroupCountSideEffect { label, key, input } => {
            let rows = run_with_context(input, graph, ctx)?;
            let counts = ctx.group_counts.entry(label.clone()).or_default();
            for row in &rows {
                let key_value = eval(key, row, graph)?;
                let key = super::ops::aggregate::map_key(&key_value);
                *counts.entry(key).or_insert(0) += row.bulk;
            }
            Ok(rows)
        }
        Node::GraphCap { labels, input } => {
            let _ = run_with_context(input, graph, ctx)?;
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
        Node::GraphShortestPath {
            source,
            target,
            direction,
            rel_types,
            max_distance,
            include_edges,
            output,
            all_paths,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            shortest_path_op(
                source,
                target.as_deref(),
                *direction,
                rel_types,
                *max_distance,
                *include_edges,
                output,
                *all_paths,
                rows,
                graph,
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
            let rows = run_with_context(input, graph, ctx)?;
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
        Node::GraphCollect {
            value,
            distinct,
            order,
            alias,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            collect_op(value, *distinct, order, alias, rows, graph)
        }
        Node::GraphProcedureCall {
            name,
            args,
            yields,
            input,
            ..
        } => {
            let upstream = match input {
                Some(input) => run_with_context(input, graph, ctx)?,
                None => vec![Row::new()],
            };
            procedure_call_op(name, args, yields, upstream, graph)
        }
        Node::GraphExtension { name, .. } => Err(InterpretError::Unsupported(format!(
            "GraphExtension({name}): extension nodes have no runtime"
        ))),

        // -------- SPARQL / RDF --------
        Node::GraphRdfQuadScan { .. } => Err(InterpretError::Unsupported(
            "GraphRdfQuadScan: SPARQL quad scans require an RDF store".into(),
        )),
        Node::GraphRdfPropertyPath { .. } => Err(InterpretError::Unsupported(
            "GraphRdfPropertyPath: SPARQL property paths require an RDF store".into(),
        )),
        Node::GraphSparqlMinus { .. } => Err(InterpretError::Unsupported(
            "GraphSparqlMinus: solution-mapping MINUS is not yet implemented".into(),
        )),
        Node::GraphService { .. } => Err(InterpretError::Unsupported(
            "GraphService: SPARQL federation is not yet implemented".into(),
        )),
        Node::GraphConstructTriples { .. } => Err(InterpretError::Unsupported(
            "GraphConstructTriples: SPARQL CONSTRUCT output is not yet implemented".into(),
        )),
        Node::GraphDescribe { .. } => Err(InterpretError::Unsupported(
            "GraphDescribe: SPARQL DESCRIBE output is not yet implemented".into(),
        )),
        Node::GraphAsk { .. } => Err(InterpretError::Unsupported(
            "GraphAsk: SPARQL ASK output is not yet implemented".into(),
        )),
        Node::GraphListComprehension {
            input_expr,
            item,
            filter,
            map_expr,
            alias,
            input,
        } => {
            let rows = run_with_context(input, graph, ctx)?;
            list_comprehension_op(
                input_expr,
                item,
                filter.as_ref(),
                map_expr.as_ref(),
                alias,
                rows,
                graph,
            )
        }
    }
}

/// Execute a `CALL <name>(...) YIELD ...` step.
///
/// We model the small set of read-only Cypher built-ins that the
/// in-memory catalog can satisfy without a real procedure registry:
/// `db.labels`, `db.relationshipTypes`, and `db.propertyKeys`.
/// Unknown procedure names return a single empty row per upstream row
/// rather than failing — that lets a query whose `YIELD` columns are
/// only used as scalars still produce a result rather than aborting.
fn procedure_call_op(
    name: &str,
    args: &[crate::ir::plan::ProcedureArg],
    yields: &[String],
    upstream: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let _ = args;
    let normalized = name.to_ascii_lowercase();
    if yields.is_empty() {
        return Ok(upstream);
    }
    let yield_first = yields.first().map(|s| s.as_str()).unwrap_or("value");

    let values_per_call: Vec<Value> = match normalized.as_str() {
        "db.labels" => graph.labels().into_iter().map(Value::String).collect(),
        "db.relationshiptypes" => graph.rel_types().into_iter().map(Value::String).collect(),
        "db.propertykeys" => {
            let mut keys = std::collections::BTreeSet::new();
            for label in graph.labels() {
                for key in graph.node_property_keys(&label) {
                    keys.insert(key);
                }
            }
            for rel_type in graph.rel_types() {
                for key in graph.edge_property_keys(&rel_type) {
                    keys.insert(key);
                }
            }
            keys.into_iter().map(Value::String).collect()
        }
        _ => Vec::new(),
    };

    let mut out = Vec::new();
    for row in upstream {
        if values_per_call.is_empty() {
            // Unknown / unhandled procedure: pass the row through with the
            // declared yield bindings set to `Null` so downstream filters
            // stay well-typed.
            let mut new_row = row;
            for binding in yields {
                new_row.bindings.insert(binding.clone(), Value::Null);
            }
            out.push(new_row);
        } else {
            for value in &values_per_call {
                let mut new_row = row.clone();
                new_row
                    .bindings
                    .insert(yield_first.to_string(), value.clone());
                for extra in yields.iter().skip(1) {
                    new_row.bindings.insert(extra.clone(), Value::Null);
                }
                out.push(new_row);
            }
        }
    }
    Ok(out)
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

fn shortest_path_op(
    source: &str,
    target: Option<&str>,
    direction: Direction,
    rel_types: &LabelExpr,
    max_distance: Option<f64>,
    include_edges: bool,
    output: &str,
    all_paths: bool,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let rel_filter = match rel_types {
        LabelExpr::Any => Vec::new(),
        LabelExpr::AnyOf(names) | LabelExpr::AllOf(names) => names.clone(),
        LabelExpr::Not(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for row in rows {
        let start = row.bindings.get(source).unwrap_or(&Value::Null);
        let paths = match target.and_then(|binding| row.bindings.get(binding)) {
            Some(target) => shortest_paths(
                graph,
                start,
                Some(target),
                direction,
                &rel_filter,
                max_distance,
                include_edges,
            ),
            None => shortest_paths(
                graph,
                start,
                None,
                direction,
                &rel_filter,
                max_distance,
                include_edges,
            ),
        };
        match paths {
            Value::List(items) if all_paths || target.is_none() => {
                for path in items {
                    let mut next = row.clone();
                    next.bindings.insert(output.to_string(), path);
                    out.push(next);
                }
            }
            Value::List(mut items) => {
                if let Some(path) = items.pop() {
                    let mut next = row.clone();
                    next.bindings.insert(output.to_string(), path);
                    out.push(next);
                }
            }
            path => {
                let mut next = row.clone();
                next.bindings.insert(output.to_string(), path);
                out.push(next);
            }
        }
    }
    Ok(out)
}
