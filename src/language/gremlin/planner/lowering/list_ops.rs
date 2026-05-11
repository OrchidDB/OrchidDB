//! List / set operators applied to a folded list traverser:
//! `merge`/`combine`/`intersect`/`difference`/`disjunct`/`product`.
//!
//! Each lowers to an `IrExpr::Call` against a runtime helper. The
//! traversal form (`combine(__.t)`) needs the sub-traversal evaluated
//! per row as a scalar; we cap it at one result before using the probe
//! binding as the RHS argument.
//!
//! `fold(seed, op)` (the reducer-folded variant of `fold()`) is also
//! handled here since its runtime helper is in the same family.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::literals::{gvalue_to_expr, gvalue_to_lit};
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{ApplyKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice};
use crate::ir::policy::OptionalMissing;
use crate::language::gremlin::ast::{ListOpKind, SackOp, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_list_op(input: Node, op: ListOpKind, rhs: &GValue) -> GremlinPlanResult<Node> {
    let helper = list_op_name(op);
    let rhs_expr = gvalue_to_expr(rhs)?;
    Ok(Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.to_string(),
            expr: IrExpr::Call {
                name: helper.into(),
                args: vec![IrExpr::Binding(CURRENT.into()), rhs_expr],
            },
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    })
}

pub(super) fn lower_list_op_traversal(
    input: Node,
    op: ListOpKind,
    sub: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let probe = lo.fresh("list_rhs");
    let child_kind = if starts_from_source(sub) {
        ChildTraversalKind::SourceUnionArm
    } else {
        ChildTraversalKind::ListRhs
    };
    let sub_node = Node::GraphSlice {
        slice: Slice {
            offset: 0,
            fetch: Some(1),
            tail: None,
        },
        input: lower_child_traversal(sub, lo, ctx, child_kind)?.boxed(),
    };
    let projected_probe = Node::GraphProject {
        mode: ProjectMode::ReplaceScope,
        items: vec![ProjectionItem {
            alias: probe.clone(),
            expr: IrExpr::Binding(CURRENT.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: sub_node.boxed(),
    };
    let applied = Node::GraphApply {
        kind: ApplyKind::Scalar,
        correlation: Vec::new(),
        outputs: vec![probe.clone()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: projected_probe.boxed(),
    };
    Ok(Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.to_string(),
            expr: IrExpr::Call {
                name: list_op_name(op).into(),
                args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::Binding(probe)],
            },
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: applied.boxed(),
    })
}

fn starts_from_source(sub: &[Step]) -> bool {
    matches!(
        sub.first(),
        Some(Step::V { .. } | Step::E { .. } | Step::Inject(_) | Step::Union(_))
    )
}

pub(super) fn lower_fold_reduce(input: Node, seed: &GValue, op: SackOp) -> GremlinPlanResult<Node> {
    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "fold_reduce".into(),
            args: vec![
                IrExpr::Binding(CURRENT.into()),
                IrExpr::Lit(gvalue_to_lit(seed)?),
                IrExpr::Lit(Lit::String(sack_op_name(op).into())),
            ],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}

fn list_op_name(op: ListOpKind) -> &'static str {
    match op {
        ListOpKind::Merge => "list_merge",
        ListOpKind::Combine => "list_combine",
        ListOpKind::Intersect => "list_intersect",
        ListOpKind::Difference => "list_difference",
        ListOpKind::Disjunct => "list_disjunct",
        ListOpKind::Product => "list_product",
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
