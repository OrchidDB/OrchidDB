//! Traversal-scoped side effects and per-traverser sacks.
//! Writers execute their input exactly once and update ExecutionContext;
//! reads never reconstruct a writer subtree. Sacks remain row-local.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::group::lower_group;
use super::helpers::{apply_by_spec, consume_by};
use super::literals::{gvalue_to_expr, gvalue_to_value};
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::language::gremlin::ast::{SackOp, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) const SACK: &str = "__sack";

pub(super) fn lower_aggregate_as<'a, I>(
    input: Node,
    label: &str,
    eager: bool,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let value_input = Node::GraphCorrelate { bindings: vec!["__gremlin_group_members".into()] };
    let (value_input, value_expr) = match consume_by(steps) {
        Some(spec) => apply_by_spec(value_input, &spec, lo, ctx)?,
        None => (value_input, IrExpr::Binding(CURRENT.into())),
    };
    lo.side_effect_bags.entry(label.to_string()).or_insert_with(|| label.to_string());
    let seed = lo.side_effect_seeds.get(label).and_then(gvalue_to_value).unwrap_or_else(|| crate::ir::value::Value::BulkSet(Vec::new()));
    let reducer = lo.side_effect_reducers.get(label).map(|(_, op)| sack_op_name(*op)).unwrap_or("collect");
    Ok(Node::GraphSideEffect {
        label: label.to_string(), value_input: value_input.boxed(), value: value_expr, seed, reducer: reducer.to_string(),
        eager, input: input.boxed(),
    })
}

pub(super) fn lower_cap(input: Node, label: &str, lo: &Lowerer) -> Node {
    lower_cap_multi(input, &[label.to_string()], lo)
}

pub(super) fn lower_side_effect_bag_as_list(input: Node, label: &str, lo: &Lowerer) -> Option<Node> {
    if !lo.side_effect_bags.contains_key(label) { return None; }
    Some(Node::GraphReadSideEffect { label: label.to_string(), input: input.boxed() })
}

pub(super) fn lower_side_effect_value(input: Node, label: &str, lo: &Lowerer) -> Option<Node> {
    if let Some(input) = lower_side_effect_bag_as_list(input.clone(), label, lo) {
        return Some(input);
    }
    let seed = lo.side_effect_seeds.get(label)?;
    let expr = gvalue_to_expr(seed).ok()?;
    Some(Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.into(),
            expr,
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    })
}

pub(super) fn lower_cap_multi(mut input: Node, labels: &[String], lo: &Lowerer) -> Node {
    for label in labels {
        if let Some(seed) = lo.side_effect_seeds.get(label).and_then(gvalue_to_value) {
            input = Node::GraphSideEffect { value_input: Node::GraphCorrelate { bindings: vec!["__gremlin_group_members".into()] }.boxed(), label: label.clone(), value: IrExpr::Binding(CURRENT.into()), seed,
                reducer: "register".into(), eager: true, input: input.boxed() };
        }
    }
    Node::GraphCap { labels: labels.to_vec(), input: input.boxed() }
}

pub(super) fn lower_subgraph(input: Node, _label: &str) -> Node {
    input
}

pub(super) fn lower_tree(input: Node, _label: Option<&str>) -> Node {
    // Fold every traverser's path history and build the nested tree map
    // (root = traversal origin) from those paths.
    let folded = fold_expr(input, IrExpr::Binding(super::context::PATH.into()));
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "tree_value".into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        },
        fields: vec![CURRENT.to_string()],
        input: folded.boxed(),
    }
}

pub(super) fn lower_group_as<'a, I>(
    input: Node,
    label: &str,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let key_by = consume_by(steps);
    let value_by = consume_by(steps);
    let members = Node::GraphCorrelate { bindings: vec!["__gremlin_group_members".into()] };
    let grouped = lower_group(members, key_by, value_by, false, lo, ctx)?;
    let Node::GraphGroupMap { key, value, input: key_input, .. } = grouped else { unreachable!() };
    lo.group_count_side_effects.insert(label.to_string());
    Ok(Node::GraphGroupSideEffect { label: label.to_string(), key, value, key_input, input: input.boxed() })
}

pub(super) fn lower_group_count_as<'a, I>(
    input: Node,
    label: &str,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let key_by = consume_by(steps);
    let members = Node::GraphCorrelate { bindings: vec!["__gremlin_group_members".into()] };
    let grouped = lower_group(members, key_by, None, true, lo, ctx)?;
    let Node::GraphGroupMap { key, value, input: key_input, .. } = grouped else { unreachable!() };
    lo.group_count_side_effects.insert(label.to_string());
    Ok(Node::GraphGroupSideEffect { label: label.to_string(), key, value, key_input, input: input.boxed() })
}

pub(super) fn lower_sack_read(input: Node) -> Node {
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.into(),
            expr: IrExpr::Binding(SACK.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

pub(super) fn lower_sack_op<'a, I>(
    input: Node,
    op: SackOp,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let (input, rhs) = match consume_by(steps) {
        Some(spec) => apply_by_spec(input, &spec, lo, ctx)?,
        None => (input, IrExpr::Binding(CURRENT.into())),
    };
    Ok(Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: SACK.into(),
            expr: IrExpr::Call {
                name: "sack_apply".into(),
                args: vec![
                    IrExpr::Binding(SACK.into()),
                    rhs,
                    IrExpr::lit_str(sack_op_name(op)),
                ],
            },
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    })
}

/// Reducer-folded `cap()` for `withSideEffect(label, init, op)` and
/// `groupCount(...).cap()`. Currently the same as plain cap (collect
/// current stream).
#[allow(dead_code)]
pub(super) fn lower_cap_aggregated(input: Node, _label: &str) -> Node {
    fold_expr(input, IrExpr::Binding(CURRENT.into()))
}

fn fold_expr(input: Node, arg: IrExpr) -> Node {
    Node::GraphAggregate {
        group: Vec::new(),
        aggs: vec![AggCall {
            kind: AggKind::CollectTraversers,
            alias: CURRENT.into(),
            arg: Some(arg),
            distinct: false,
        }],
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

pub(super) fn sack_op_name(op: SackOp) -> &'static str {
    match op {
        SackOp::Sum | SackOp::SumLong => "sum",
        SackOp::Minus => "minus",
        SackOp::Mult => "mult",
        SackOp::Div => "div",
        SackOp::Min => "min",
        SackOp::Max => "max",
        SackOp::Assign => "assign",
        SackOp::And => "and",
        SackOp::Or => "or",
        SackOp::AddAll => "addAll",
    }
}
