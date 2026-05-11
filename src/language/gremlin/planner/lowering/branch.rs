//! Branching: `union(t...)`, `coalesce(t...)`, `choose(P, then, else?)`,
//! `choose(t, then, else?)`, and `branch(t).option(...).option(...)`.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::literals::gvalue_to_expr;
use super::predicates::predicate_to_expr;
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::{BinaryOp, IrExpr};
use crate::ir::plan::{
    ApplyKind, ChooseArm, ChooseSelector, ChooseUnmatched, CoalesceSuccess, Node,
    ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice, UnionAlign,
};
use crate::ir::policy::OptionalMissing;
use crate::language::gremlin::ast::{OptionKey, Step, TraversalOption};
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};
use crate::language::gremlin::semantics::{GValue, Predicate};

pub(super) fn lower_coalesce(
    input: Node,
    arms: &[Vec<Step>],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let mut compiled = Vec::with_capacity(arms.len());
    for arm in arms {
        compiled.push(lower_child_traversal(
            arm,
            lo,
            ctx,
            ChildTraversalKind::CoalesceArm,
        )?);
    }
    Ok(Node::GraphCoalesce {
        success: CoalesceSuccess::FirstNonEmpty,
        output: CURRENT.into(),
        correlation: vec![CURRENT.into()],
        arm_outputs: Vec::new(),
        input: input.boxed(),
        arms: compiled,
    })
}

pub(super) fn lower_mid_traversal_union(
    input: Node,
    branches: &[Vec<Step>],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    // `union()` with no arms — pass the input through unchanged. The
    // parser produces this shape for a few odd-but-valid TinkerPop
    // queries (e.g. `or()` desugars to `union()` of the alternatives,
    // and an empty `or()` is a tautology).
    let mut iter = branches.iter();
    let Some(first) = iter.next() else {
        return Ok(input);
    };
    let mut acc = Node::GraphApply {
        kind: ApplyKind::Inner,
        correlation: vec![CURRENT.into()],
        outputs: vec![CURRENT.into()],
        optional_missing: OptionalMissing::Null,
        left: input.clone().boxed(),
        right: lower_child_traversal(first, lo, ctx, ChildTraversalKind::UnionArm)?.boxed(),
    };
    for next in iter {
        let arm = Node::GraphApply {
            kind: ApplyKind::Inner,
            correlation: vec![CURRENT.into()],
            outputs: vec![CURRENT.into()],
            optional_missing: OptionalMissing::Null,
            left: input.clone().boxed(),
            right: lower_child_traversal(next, lo, ctx, ChildTraversalKind::UnionArm)?.boxed(),
        };
        acc = Node::GraphUnion {
            all: true,
            align: UnionAlign::ByPosition,
            left: acc.boxed(),
            right: arm.boxed(),
        };
    }
    Ok(acc)
}

pub(super) fn lower_choose_predicate(
    input: Node,
    predicate: &Predicate,
    then: &[Step],
    else_branch: Option<&[Step]>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let true_arm = lower_child_traversal(then, lo, ctx, ChildTraversalKind::BranchArm)?;
    let false_arm = match else_branch {
        Some(steps) => lower_child_traversal(steps, lo, ctx, ChildTraversalKind::BranchArm)?,
        None => Node::GraphCorrelate {
            bindings: vec![CURRENT.into()],
        },
    };
    Ok(Node::GraphChoose {
        selector: ChooseSelector::Boolean(predicate_to_expr(
            IrExpr::Binding(CURRENT.into()),
            predicate,
        )?),
        output: CURRENT.into(),
        correlation: vec![CURRENT.into()],
        arms: vec![
            ChooseArm {
                key: None,
                body: true_arm,
            },
            ChooseArm {
                key: None,
                body: false_arm,
            },
        ],
        default: None,
        unmatched: ChooseUnmatched::PassThrough,
        input: input.boxed(),
    })
}

pub(super) fn lower_choose_traversal(
    input: Node,
    condition: &[Step],
    then: &[Step],
    else_branch: Option<&[Step]>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    // Lower `choose(traversal, then, else)` as semi-driven choose.
    // The condition becomes a Semi-Apply test rendered into an
    // IrExpr::IsBound on a fresh probe binding.
    let probe = lo.fresh("probe");
    let condition = Node::GraphSlice {
        slice: Slice {
            offset: 0,
            fetch: Some(1),
            tail: None,
        },
        input: lower_child_traversal(condition, lo, ctx, ChildTraversalKind::ChooseCondition)?
            .boxed(),
    };
    let probe_apply = Node::GraphApply {
        kind: ApplyKind::Optional,
        correlation: vec![CURRENT.into()],
        outputs: vec![probe.clone()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: probe.clone(),
                expr: IrExpr::lit_bool(true),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: condition.boxed(),
        }
        .boxed(),
    };
    let true_arm = lower_child_traversal(then, lo, ctx, ChildTraversalKind::BranchArm)?;
    let false_arm = match else_branch {
        Some(steps) => lower_child_traversal(steps, lo, ctx, ChildTraversalKind::BranchArm)?,
        None => Node::GraphCorrelate {
            bindings: vec![CURRENT.into()],
        },
    };
    Ok(Node::GraphChoose {
        selector: ChooseSelector::Boolean(IrExpr::IsBound(probe)),
        output: CURRENT.into(),
        correlation: vec![CURRENT.into()],
        arms: vec![
            ChooseArm {
                key: None,
                body: true_arm,
            },
            ChooseArm {
                key: None,
                body: false_arm,
            },
        ],
        default: None,
        unmatched: ChooseUnmatched::PassThrough,
        input: probe_apply.boxed(),
    })
}

pub(super) fn lower_branch_options(
    input: Node,
    dispatch: &[Step],
    options: &[TraversalOption],
    is_choose: bool,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let dispatch_key = lo.fresh(if is_choose {
        "choose_key"
    } else {
        "branch_key"
    });
    let keyed_input = lower_dispatch_key(input, dispatch, &dispatch_key, is_choose, lo, ctx)?;

    let mut regular = Vec::new();
    let mut pick_any = Vec::new();
    let mut pick_none = Vec::new();
    let mut pick_unproductive = Vec::new();
    for opt in options {
        match &opt.key {
            OptionKey::Value(value) => regular.push((
                option_value_condition(&dispatch_key, value)?,
                opt.traversal.clone(),
            )),
            OptionKey::Predicate(predicate) => regular.push((
                predicate_to_expr(IrExpr::Binding(dispatch_key.clone()), predicate)?,
                opt.traversal.clone(),
            )),
            OptionKey::PickAny if is_choose => {
                return Err(GremlinPlanError::Unsupported(
                    "choose().option(Pick.any, ...) is invalid because choose selects one option"
                        .to_string(),
                ));
            }
            OptionKey::PickAny => pick_any.push(opt.traversal.clone()),
            OptionKey::PickNone => pick_none.push(opt.traversal.clone()),
            OptionKey::PickUnproductive => pick_unproductive.push(opt.traversal.clone()),
            OptionKey::Traversal(_) => {
                return Err(GremlinPlanError::Unsupported(
                    "option(__.dispatch_traversal) keys are not yet wired".to_string(),
                ));
            }
        }
    }

    if is_choose {
        lower_choose_options(
            keyed_input,
            &dispatch_key,
            regular,
            pick_none,
            pick_unproductive,
            lo,
            ctx,
        )
    } else {
        lower_branch_option_union(
            keyed_input,
            &dispatch_key,
            regular,
            pick_any,
            pick_none,
            pick_unproductive,
            lo,
            ctx,
        )
    }
}

pub(super) fn lower_local_or_map(
    input: Node,
    sub: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
    kind: ChildTraversalKind,
) -> GremlinPlanResult<Node> {
    if matches!(kind, ChildTraversalKind::SideEffect) {
        return Ok(input);
    }
    let right = lower_child_traversal(sub, lo, ctx, kind)?;
    let right = if matches!(kind, ChildTraversalKind::Map) {
        Node::GraphSlice {
            slice: Slice {
                offset: 0,
                fetch: Some(1),
                tail: None,
            },
            input: right.boxed(),
        }
    } else {
        right
    };
    Ok(Node::GraphApply {
        kind: ApplyKind::Inner,
        correlation: vec![CURRENT.into()],
        outputs: vec![CURRENT.into()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: right.boxed(),
    })
}

fn lower_dispatch_key(
    input: Node,
    dispatch: &[Step],
    dispatch_key: &str,
    is_choose: bool,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let dispatch_node =
        lower_child_traversal(dispatch, lo, ctx, ChildTraversalKind::BranchDispatch)?;
    let dispatch_node = if is_choose {
        Node::GraphSlice {
            slice: Slice {
                offset: 0,
                fetch: Some(1),
                tail: None,
            },
            input: dispatch_node.boxed(),
        }
    } else {
        dispatch_node
    };
    Ok(Node::GraphApply {
        kind: ApplyKind::Optional,
        correlation: vec![CURRENT.into()],
        outputs: vec![dispatch_key.to_string()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: dispatch_key.to_string(),
                expr: IrExpr::Binding(CURRENT.into()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: dispatch_node.boxed(),
        }
        .boxed(),
    })
}

fn lower_choose_options(
    input: Node,
    dispatch_key: &str,
    regular: Vec<(IrExpr, Vec<Step>)>,
    pick_none: Vec<Vec<Step>>,
    pick_unproductive: Vec<Vec<Step>>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let none = lower_first_pick(pick_none, lo, ctx)?;
    let unproductive = lower_first_pick(pick_unproductive, lo, ctx)?;
    let mut fallback = productive_default(dispatch_key, none, unproductive);
    for (condition, steps) in regular.into_iter().rev() {
        let body = lower_child_traversal(&steps, lo, ctx, ChildTraversalKind::BranchArm)?;
        fallback = boolean_choose_correlated(
            condition,
            body,
            fallback,
            correlate_current_and(dispatch_key),
            vec![CURRENT.into(), dispatch_key.to_string()],
        );
    }
    Ok(attach_input(fallback, input))
}

fn lower_branch_option_union(
    input: Node,
    dispatch_key: &str,
    regular: Vec<(IrExpr, Vec<Step>)>,
    pick_any: Vec<Vec<Step>>,
    pick_none: Vec<Vec<Step>>,
    pick_unproductive: Vec<Vec<Step>>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let mut nodes = Vec::new();
    let mut regular_conditions = Vec::new();
    for (condition, steps) in regular {
        regular_conditions.push(condition.clone());
        let body = lower_child_traversal(&steps, lo, ctx, ChildTraversalKind::BranchArm)?;
        nodes.push(boolean_choose(
            condition,
            body,
            Node::GraphEmpty,
            input.clone(),
        ));
    }
    for steps in pick_any {
        let body = lower_child_traversal(&steps, lo, ctx, ChildTraversalKind::BranchArm)?;
        nodes.push(boolean_choose(
            productive_condition(dispatch_key),
            body,
            Node::GraphEmpty,
            input.clone(),
        ));
    }
    for steps in pick_none {
        let body = lower_child_traversal(&steps, lo, ctx, ChildTraversalKind::BranchArm)?;
        let none_body = no_regular_match(dispatch_key, regular_conditions.clone(), body);
        nodes.push(attach_input(none_body, input.clone()));
    }
    for steps in pick_unproductive {
        let body = lower_child_traversal(&steps, lo, ctx, ChildTraversalKind::BranchArm)?;
        nodes.push(boolean_choose(
            unproductive_condition(dispatch_key),
            body,
            Node::GraphEmpty,
            input.clone(),
        ));
    }
    Ok(union_all(nodes))
}

fn lower_first_pick(
    picks: Vec<Vec<Step>>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Option<Node>> {
    picks
        .into_iter()
        .next()
        .map(|steps| lower_child_traversal(&steps, lo, ctx, ChildTraversalKind::BranchArm))
        .transpose()
}

fn productive_default(dispatch_key: &str, none: Option<Node>, unproductive: Option<Node>) -> Node {
    match (none, unproductive) {
        (Some(none), Some(unproductive)) => boolean_choose_correlated(
            productive_condition(dispatch_key),
            none,
            unproductive,
            correlate_current_and(dispatch_key),
            vec![CURRENT.into(), dispatch_key.to_string()],
        ),
        (Some(none), None) => boolean_choose_correlated(
            productive_condition(dispatch_key),
            none,
            correlate_current(),
            correlate_current_and(dispatch_key),
            vec![CURRENT.into(), dispatch_key.to_string()],
        ),
        (None, Some(unproductive)) => boolean_choose_correlated(
            productive_condition(dispatch_key),
            correlate_current(),
            unproductive,
            correlate_current_and(dispatch_key),
            vec![CURRENT.into(), dispatch_key.to_string()],
        ),
        (None, None) => correlate_current_and(dispatch_key),
    }
}

fn no_regular_match(dispatch_key: &str, conditions: Vec<IrExpr>, body: Node) -> Node {
    let mut node = body;
    for condition in conditions.into_iter().rev() {
        node = boolean_choose_correlated(
            condition,
            Node::GraphEmpty,
            node,
            correlate_current_and(dispatch_key),
            vec![CURRENT.into(), dispatch_key.to_string()],
        );
    }
    node
}

fn option_value_condition(dispatch_key: &str, value: &GValue) -> GremlinPlanResult<IrExpr> {
    if matches!(value, GValue::Null) {
        Ok(IrExpr::IsNull(Box::new(IrExpr::Binding(
            dispatch_key.to_string(),
        ))))
    } else {
        Ok(IrExpr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(IrExpr::Binding(dispatch_key.to_string())),
            rhs: Box::new(gvalue_to_expr(value)?),
        })
    }
}

fn productive_condition(dispatch_key: &str) -> IrExpr {
    IrExpr::IsBound(dispatch_key.to_string())
}

fn unproductive_condition(dispatch_key: &str) -> IrExpr {
    IrExpr::Not(Box::new(productive_condition(dispatch_key)))
}

fn boolean_choose(condition: IrExpr, true_body: Node, false_body: Node, input: Node) -> Node {
    boolean_choose_correlated(
        condition,
        true_body,
        false_body,
        input,
        vec![CURRENT.into()],
    )
}

fn boolean_choose_correlated(
    condition: IrExpr,
    true_body: Node,
    false_body: Node,
    input: Node,
    correlation: Vec<String>,
) -> Node {
    Node::GraphChoose {
        selector: ChooseSelector::Boolean(condition),
        output: CURRENT.into(),
        correlation,
        arms: vec![
            ChooseArm {
                key: None,
                body: true_body,
            },
            ChooseArm {
                key: None,
                body: false_body,
            },
        ],
        default: None,
        unmatched: ChooseUnmatched::Drop,
        input: input.boxed(),
    }
}

fn attach_input(node: Node, input: Node) -> Node {
    match node {
        Node::GraphChoose {
            selector,
            output,
            correlation,
            arms,
            default,
            unmatched,
            ..
        } => Node::GraphChoose {
            selector,
            output,
            correlation,
            arms,
            default,
            unmatched,
            input: input.boxed(),
        },
        other => boolean_choose(IrExpr::lit_bool(true), other, Node::GraphEmpty, input),
    }
}

fn union_all(nodes: Vec<Node>) -> Node {
    let mut iter = nodes.into_iter();
    let Some(first) = iter.next() else {
        return Node::GraphEmpty;
    };
    iter.fold(first, |acc, node| Node::GraphUnion {
        all: true,
        align: UnionAlign::ByPosition,
        left: acc.boxed(),
        right: node.boxed(),
    })
}

fn correlate_current() -> Node {
    Node::GraphCorrelate {
        bindings: vec![CURRENT.into()],
    }
}

fn correlate_current_and(binding: &str) -> Node {
    Node::GraphCorrelate {
        bindings: vec![CURRENT.into(), binding.to_string()],
    }
}
