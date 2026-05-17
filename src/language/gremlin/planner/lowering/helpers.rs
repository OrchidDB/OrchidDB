//! Small utilities shared across the lowering modules.
//!
//! These are pure, stateless transforms: AST → IR plumbing, label
//! plumbing, boolean composition, by(...) consumption, and the
//! id-filter helper. Anything that needs `Lowerer` state lives in its
//! own category module.

use std::iter::Peekable;

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use crate::ir::expr::{BinaryOp, IrExpr};
use crate::ir::plan::{Direction, LabelExpr, Node};
use crate::language::gremlin::ast::{BySpec, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::{Direction as SemDirection, GValue};

pub(super) fn rel_types_label(types: &[String]) -> LabelExpr {
    if types.is_empty() {
        LabelExpr::Any
    } else {
        LabelExpr::AnyOf(types.iter().cloned().collect())
    }
}

pub(super) fn map_direction(d: SemDirection) -> Direction {
    match d {
        SemDirection::Out => Direction::Out,
        SemDirection::In => Direction::In,
        SemDirection::Both => Direction::Both,
    }
}

pub(super) fn any_label(binding: &str, labels: &[String]) -> IrExpr {
    let exprs = labels
        .iter()
        .map(|label| IrExpr::HasLabel {
            binding: binding.to_string(),
            label: label.clone(),
        })
        .collect::<Vec<_>>();
    or_chain(exprs)
}

pub(super) fn or_chain(parts: Vec<IrExpr>) -> IrExpr {
    let mut iter = parts.into_iter();
    let mut acc = match iter.next() {
        Some(first) => first,
        None => return IrExpr::lit_bool(false),
    };
    for next in iter {
        acc = IrExpr::Binary {
            op: BinaryOp::Or,
            lhs: Box::new(acc),
            rhs: Box::new(next),
        };
    }
    acc
}

#[allow(dead_code)]
pub(super) fn strip_return(node: Node) -> Node {
    match node {
        Node::GraphReturn { input, .. } => *input,
        other => other,
    }
}

/// Peek-and-consume a single trailing `by(spec)` modulator from the
/// step stream, if any.
pub(super) fn consume_by<'a, I>(steps: &mut Peekable<I>) -> Option<BySpec>
where
    I: Iterator<Item = &'a Step>,
{
    if matches!(steps.peek(), Some(Step::By(_))) {
        if let Some(Step::By(spec)) = steps.next() {
            return Some(spec.clone());
        }
    }
    None
}

/// Apply a single `by(...)` modulator. Returns the (possibly augmented)
/// input plus an `IrExpr` that resolves to the by-key when evaluated
/// against a row of the augmented input.
///
/// `by()` is a pass-through. For `by("name")` and `by(__.sub)` we lower
/// a scalar child traversal, bind the first produced result to a fresh
/// probe, and return that probe binding as the key expression. Default
/// Gremlin `by(...)` is productive: if the child produces no result the
/// traverser is filtered. `ProductiveByStrategy` flips that boundary to
/// optional/null instead.
pub(super) fn apply_by_spec(
    input: Node,
    spec: &BySpec,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<(Node, IrExpr)> {
    apply_by_spec_impl(input, spec, lo, ctx, lo.productive_by)
}

pub(super) fn apply_project_by_spec(
    input: Node,
    spec: &BySpec,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<(Node, IrExpr)> {
    apply_by_spec_impl(input, spec, lo, ctx, true)
}

fn apply_by_spec_impl(
    input: Node,
    spec: &BySpec,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
    keep_unproductive: bool,
) -> GremlinPlanResult<(Node, IrExpr)> {
    use super::sub_traversal::lower_child_traversal;
    use crate::ir::plan::{ApplyKind, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice};
    use crate::ir::policy::OptionalMissing;

    let sub_steps;
    let sub = if let Some(sub) = &spec.traversal {
        Some(sub.as_slice())
    } else if let Some(key) = &spec.key {
        sub_steps = vec![Step::Values(vec![key.clone()])];
        Some(sub_steps.as_slice())
    } else {
        None
    };

    if let Some(sub) = sub {
        let probe = lo.fresh("by_key");
        let sub_node = Node::GraphSlice {
            slice: Slice {
                offset: 0,
                fetch: Some(1),
                tail: None,
            },
            input: lower_child_traversal(sub, lo, ctx, ChildTraversalKind::ByModulator)?.boxed(),
        };
        let projected = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: probe.clone(),
                expr: IrExpr::Binding(CURRENT.into()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: sub_node.boxed(),
        };
        let apply = Node::GraphApply {
            kind: if keep_unproductive {
                ApplyKind::Optional
            } else {
                ApplyKind::Inner
            },
            correlation: Vec::new(),
            outputs: vec![probe.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: projected.boxed(),
        };
        return Ok((apply, IrExpr::Binding(probe)));
    }
    Ok((input, IrExpr::Binding(CURRENT.into())))
}

/// Filter the current binding by id (or `label#row` synthetic id token).
/// The harness binds `vid1` → `"person#0"` etc.; the parser emits those
/// strings as `GValue::String`, so we accept both numeric and label-row
/// shapes here.
pub(super) fn filter_by_ids(input: Node, ids: &[GValue]) -> Node {
    if ids.is_empty() {
        return input;
    }
    let id_target = IrExpr::Id(CURRENT.into());
    let parts = ids
        .iter()
        .flat_map(|v| id_filter_parts(v, &id_target))
        .collect::<Vec<_>>();
    Node::GraphFilter {
        condition: if parts.is_empty() {
            IrExpr::lit_bool(false)
        } else {
            or_chain(parts)
        },
        input: input.boxed(),
    }
}

fn id_filter_parts(value: &GValue, id_target: &IrExpr) -> Vec<IrExpr> {
    match value {
        GValue::Int(n) => vec![IrExpr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(id_target.clone()),
            rhs: Box::new(IrExpr::lit_int(*n)),
        }],
        GValue::String(s) => {
            vec![
                element_token_filter(CURRENT, s).unwrap_or_else(|| IrExpr::Binary {
                    op: BinaryOp::Eq,
                    lhs: Box::new(id_target.clone()),
                    rhs: Box::new(IrExpr::lit_str(s.clone())),
                }),
            ]
        }
        GValue::List(items) => items
            .iter()
            .flat_map(|item| id_filter_parts(item, id_target))
            .collect(),
        _ => Vec::new(),
    }
}

/// Decompose a `"label#row"` synthetic id token into a label-and-id
/// filter expression. Returns `None` if the token isn't of that shape.
pub(super) fn element_token_filter(binding: &str, token: &str) -> Option<IrExpr> {
    let (label, row_id) = token.split_once('#')?;
    let row_id = row_id.parse::<i64>().ok()?;
    Some(IrExpr::and(vec![
        IrExpr::HasLabel {
            binding: binding.to_string(),
            label: label.to_string(),
        },
        IrExpr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(IrExpr::Id(binding.to_string())),
            rhs: Box::new(IrExpr::lit_int(row_id)),
        },
    ]))
}
