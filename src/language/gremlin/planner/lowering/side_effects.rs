//! Side-effect bags and per-traverser sack.
//!
//! Full Gremlin semantics maintain a per-traversal map of *named bags*
//! (`aggregate("a")`, `store("a")`, `groupAs("a")`, `groupCountAs("a")`,
//! pre-seeded by `withSideEffect("a", v)`) plus a per-traverser
//! *sack* register (`withSack(initial)`, `sack()`, `sack(op)`).
//! Modeling all of that in Graph IR requires a side-channel output —
//! distinct from the main row stream — that the planner doesn't yet
//! emit.
//!
//! As a pragmatic first cut, we lower each side-effect step to its
//! *closest* row-stream-only approximation. The simple shapes appear
//! in the conformance corpus far more often than the exotic ones:
//!
//! - `aggregate(label)` / `store(label)` / `groupAs(label)` /
//!   `groupCountAs(label)` — no-op on the main stream. The bag would
//!   be built on the side.
//! - `cap(label)` — `fold()`-equivalent collect of the current row
//!   stream into a single list. This is correct when the only writer
//!   to `label` is an `aggregate(label)` *upstream of the cap*, with
//!   no branch in between (the dominant case in TinkerPop tests).
//! - `tree(label)` / `subgraph(label)` — no-op, then `cap`-equivalent
//!   when the same label is consumed.
//! - `withSideEffect(label, value)` — preamble; the leading-config
//!   loop in `lowering.rs` already drains it. A later `cap(label)`
//!   without a writer in between yields the seed value, which we
//!   approximate as a fold (probably empty).
//! - `sack()` / `sack(op)` — tracked as a hidden per-traverser row
//!   binding initialized by root `withSack(initial)`.
//!
//! Anything more elaborate (multi-bag merges, repeat-with-aggregate,
//! reducer-folded bags) gets a clean Unsupported.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::group::lower_group;
use super::helpers::{apply_by_spec, consume_by};
use super::literals::{gvalue_to_expr, gvalue_to_value};
use super::reduce::lower_fold;
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, UnionAlign};
use crate::language::gremlin::ast::{SackOp, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) const SACK: &str = "__sack";

pub(super) fn lower_aggregate_as<'a, I>(
    input: Node,
    label: &str,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let (input, value_expr) = match consume_by(steps) {
        Some(spec) => apply_by_spec(input, &spec, lo, ctx)?,
        None => (input, IrExpr::Binding(CURRENT.into())),
    };
    let alias = if let Some(existing) = lo.side_effect_bags.get(label) {
        existing.clone()
    } else {
        let fresh = lo.fresh("side_effect_bag");
        lo.side_effect_bags.insert(label.to_string(), fresh.clone());
        fresh
    };
    Ok(Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias,
            expr: value_expr,
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    })
}

pub(super) fn lower_cap(input: Node, label: &str, lo: &Lowerer) -> Node {
    // Map-valued side effects registered by `tree(label)` / `group(label)`
    // not already consumed: cap collapses the stream to that map.
    if let Some(map_node) = lo.group_side_effect_maps.get(label) {
        return map_node.clone();
    }
    if lo.group_count_side_effects.contains(label) {
        return Node::GraphCap {
            labels: vec![label.to_string()],
            input: input.boxed(),
        };
    }
    if let Some(alias) = lo.side_effect_bags.get(label) {
        if alias == CURRENT {
            return input;
        }
        let writers = collect_project_writers(&input, alias);
        let writer_inputs: Vec<Node> = if writers.is_empty() {
            vec![input.clone()]
        } else {
            writers
        };
        // One row per element: project the bag-binding as `current` over
        // each writer subtree, then union. Multiple `aggregate("a")`
        // writers in the same chain accumulate into the bag.
        let mut projected: Vec<Node> = writer_inputs
            .into_iter()
            .map(|writer| Node::GraphProject {
                mode: ProjectMode::ReplaceCurrent,
                items: vec![ProjectionItem {
                    alias: CURRENT.into(),
                    expr: IrExpr::Binding(alias.clone()),
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: writer.boxed(),
            })
            .collect();
        // A Set-typed seed (`withSideEffect(label, {..})`) makes the bag a
        // LinkedHashSet: cap() yields ONE traverser holding the deduplicated
        // seed-then-stream union rather than the raw bulk stream.
        // `{}` parses as an empty Map literal, but as a side-effect seed it
        // means an empty Set (TinkerPop gherkin `withSideEffect("x", {})`).
        let set_seed = match lo.side_effect_seeds.get(label) {
            Some(seed @ crate::language::gremlin::semantics::GValue::Set(_)) => Some(seed),
            Some(seed @ crate::language::gremlin::semantics::GValue::Map(m)) if m.is_empty() => {
                Some(seed)
            }
            _ => None,
        };
        if let Some(seed) = set_seed {
            if let Ok(seed_expr) = gvalue_to_expr(seed) {
                let folded = fold_expr(union_all(projected), IrExpr::Binding(CURRENT.into()));
                return Node::GraphProject {
                    mode: ProjectMode::ReplaceCurrent,
                    items: vec![ProjectionItem {
                        alias: CURRENT.into(),
                        expr: IrExpr::Call {
                            name: "set_compact".into(),
                            args: vec![seed_expr, IrExpr::Binding(CURRENT.into())],
                        },
                    }],
                    error_policy: ProjectErrorPolicy::PropagateError,
                    input: folded.boxed(),
                };
            }
        }
        // `withSideEffect(label, seed, Operator.addAll)` seeds the bag but
        // still exposes bag entries as traversers at cap time. `assign`
        // replaces the seed with the aggregate stream, so it fans out the
        // projected writers without reducing to the final scalar.
        if let Some((seed, op)) = lo.side_effect_reducers.get(label) {
            match op {
                SackOp::AddAll => {
                    projected.insert(0, seed_values(seed));
                    return union_all(projected);
                }
                SackOp::Assign => {
                    // `assign` replaces the seed with the aggregated
                    // stream. A barrier aggregate assigns the whole
                    // BulkSet at once (cap yields every entry); a lazy
                    // `local(aggregate(..))` writes one traverser at a
                    // time, so only the last value survives.
                    if lo.side_effect_local_labels.contains(label) {
                        return Node::GraphSlice {
                            slice: crate::ir::plan::Slice {
                                offset: 0,
                                fetch: None,
                                tail: Some(1),
                            },
                            input: union_all(projected).boxed(),
                        };
                    }
                    return union_all(projected);
                }
                _ => {
                    if let Ok(seed) = gvalue_to_expr(seed) {
                        let unioned = union_all(projected);
                        let folded = fold_expr(unioned, IrExpr::Binding(CURRENT.into()));
                        return Node::GraphProject {
                            mode: ProjectMode::ReplaceCurrent,
                            items: vec![ProjectionItem {
                                alias: CURRENT.into(),
                                expr: IrExpr::Call {
                                    name: "fold_reduce".into(),
                                    args: vec![
                                        IrExpr::Binding(CURRENT.into()),
                                        seed,
                                        IrExpr::lit_str(sack_op_name(*op)),
                                    ],
                                },
                            }],
                            error_policy: ProjectErrorPolicy::PropagateError,
                            input: folded.boxed(),
                        };
                    }
                }
            }
        }
        return union_all(projected);
    }
    // Approximate as `fold()` — collect the current stream into one row.
    lower_fold(input)
}

fn seed_values(seed: &crate::language::gremlin::semantics::GValue) -> Node {
    let rows = match seed {
        crate::language::gremlin::semantics::GValue::List(items)
        | crate::language::gremlin::semantics::GValue::Set(items) => items
            .iter()
            .map(|value| vec![gvalue_to_value(value)])
            .collect(),
        other => vec![vec![gvalue_to_value(other)]],
    };
    Node::GraphValues {
        bindings: vec![CURRENT.into()],
        rows,
        bulk: None,
    }
}

pub(super) fn lower_side_effect_bag_as_list(
    input: Node,
    label: &str,
    lo: &Lowerer,
) -> Option<Node> {
    let alias = lo.side_effect_bags.get(label)?;
    let writers = collect_project_writers(&input, alias);
    let writer_inputs: Vec<Node> = if writers.is_empty() {
        vec![input]
    } else {
        writers
    };
    // The fold's collect skips nulls; under ProductiveByStrategy the bag
    // legitimately contains nulls (`aggregate("a").by("foo")`), so route
    // them through a sentinel that survives the collect and restore after.
    let keep_nulls = lo.productive_by;
    let projected: Vec<Node> = writer_inputs
        .into_iter()
        .map(|writer| {
            let expr = if keep_nulls {
                IrExpr::Call {
                    name: "null_to_sentinel".into(),
                    args: vec![IrExpr::Binding(alias.clone())],
                }
            } else {
                IrExpr::Binding(alias.clone())
            };
            Node::GraphProject {
                mode: ProjectMode::ReplaceCurrent,
                items: vec![ProjectionItem {
                    alias: CURRENT.into(),
                    expr,
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: writer.boxed(),
            }
        })
        .collect();
    let folded = lower_fold(union_all(projected));
    if keep_nulls {
        Some(Node::GraphCurrentProject {
            expr: IrExpr::Call {
                name: "list_restore_null_sentinels".into(),
                args: vec![IrExpr::Binding(CURRENT.into())],
            },
            fields: vec![CURRENT.to_string()],
            input: folded.boxed(),
        })
    } else {
        Some(folded)
    }
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

fn union_all(mut nodes: Vec<Node>) -> Node {
    let mut acc = nodes.remove(0);
    for n in nodes {
        acc = Node::GraphUnion {
            all: true,
            align: UnionAlign::ByPosition,
            left: acc.boxed(),
            right: n.boxed(),
        };
    }
    acc
}

pub(super) fn lower_cap_multi(input: Node, labels: &[String], lo: &Lowerer) -> Node {
    if labels
        .iter()
        .any(|label| lo.group_count_side_effects.contains(label))
    {
        return Node::GraphCap {
            labels: labels.to_vec(),
            input: input.boxed(),
        };
    }
    // Multi-cap returns a map keyed by label; we approximate as a
    // single-list fold (closest cheap shape).
    lower_fold(input)
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
    if matches!(steps.peek(), Some(Step::Cap(cap_label)) if cap_label == label) {
        steps.next();
        lo.side_effect_bags
            .insert(label.to_string(), CURRENT.to_string());
        return lower_group(input, key_by, value_by, false, lo, ctx);
    }
    // `group(label)` not immediately capped: keep the main stream untouched
    // and remember a plan that computes the full group map, so a later
    // `select(label)` can attach it per traverser.
    let map_node = lower_group(input.clone(), key_by, value_by, false, lo, ctx)?;
    lo.group_side_effect_maps.insert(label.to_string(), map_node);
    Ok(input)
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
    let restore_current = key_by.as_ref().map(|_| lo.fresh("group_count_current"));
    let input = match restore_current.as_ref() {
        Some(alias) => Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: alias.clone(),
                expr: IrExpr::Binding(CURRENT.into()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        },
        None => input,
    };
    let (input, key) = match key_by {
        Some(spec) => apply_by_spec(input, &spec, lo, ctx)?,
        None => (input, IrExpr::Binding(CURRENT.into())),
    };
    let input = match restore_current {
        Some(alias) => Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Binding(alias),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        },
        None => input,
    };
    lo.group_count_side_effects.insert(label.to_string());
    Ok(Node::GraphGroupCountSideEffect {
        label: label.to_string(),
        key,
        input: input.boxed(),
    })
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

fn collect_project_writers(node: &Node, alias: &str) -> Vec<Node> {
    let mut out = Vec::new();
    visit_writers(node, alias, &mut out);
    out
}

fn visit_writers(node: &Node, alias: &str, out: &mut Vec<Node>) {
    match node {
        Node::GraphFilter { input, .. }
        | Node::GraphCurrentProject { input, .. }
        | Node::GraphAggregate { input, .. }
        | Node::GraphDistinct { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. }
        | Node::GraphSliceExpr { input, .. }
        | Node::GraphBarrier { input, .. }
        | Node::GraphUnwind { input, .. }
        | Node::GraphGroupMap { input, .. }
        | Node::GraphGroupCountSideEffect { input, .. }
        | Node::GraphCap { input, .. }
        | Node::GraphCollect { input, .. }
        | Node::GraphExpand { input, .. }
        | Node::GraphBind { input, .. } => visit_writers(input, alias, out),
        Node::GraphProject { input, items, .. } => {
            visit_writers(input, alias, out);
            if items.iter().any(|item| item.alias == alias) {
                out.push(node.clone());
            }
        }
        Node::GraphApply { left, right, .. } | Node::GraphUnion { left, right, .. } => {
            visit_writers(left, alias, out);
            visit_writers(right, alias, out);
        }
        Node::GraphRepeat { seed, body, .. } => {
            // Writers upstream of the loop (in the seed) contribute their
            // rows once. Writers inside the body write once per
            // iteration: re-run the loop with emit-after-each-iteration
            // so the bag sees every write, not just the final frontier.
            visit_writers(seed, alias, out);
            let mut body_writers = Vec::new();
            visit_writers(body, alias, &mut body_writers);
            if !body_writers.is_empty() {
                let mut emit_each = node.clone();
                if let Node::GraphRepeat { emit, .. } = &mut emit_each {
                    *emit = crate::ir::plan::EmitMode::AfterEachIteration;
                }
                out.push(Node::GraphProject {
                    mode: ProjectMode::PreserveVisible,
                    items: vec![ProjectionItem {
                        alias: alias.to_string(),
                        expr: IrExpr::Binding(alias.to_string()),
                    }],
                    error_policy: ProjectErrorPolicy::PropagateError,
                    input: emit_each.boxed(),
                });
            }
        }
        _ => {}
    }
}

fn sack_op_name(op: SackOp) -> &'static str {
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
