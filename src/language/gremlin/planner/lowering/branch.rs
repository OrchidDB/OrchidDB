//! Branching: `union(t...)`, `coalesce(t...)`, `choose(P, then, else?)`,
//! `choose(t, then, else?)`, and `branch(t).option(...).option(...)`.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::literals::gvalue_to_expr;
use super::predicates::predicate_to_expr;
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{
    ApplyKind, ChooseArm, ChooseSelector, ChooseUnmatched, CoalesceSuccess, Node,
    ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice,
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
    let mut arms = Vec::new();
    for steps in branches {
        arms.push((
            IrExpr::lit_bool(true),
            lower_arm_continuation(stream_input(), steps, lo, ctx)?,
        ));
    }
    Ok(stream_choose(input, arms))
}

pub(super) fn lower_choose_predicate(
    input: Node,
    predicate: &Predicate,
    then: &[Step],
    else_branch: Option<&[Step]>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let condition = truth_test(predicate_to_expr(
        IrExpr::Binding(CURRENT.into()),
        predicate,
    )?);
    let true_arm = lower_arm_continuation(stream_input(), then, lo, ctx)?;
    let false_arm = lower_arm_continuation(stream_input(), else_branch.unwrap_or(&[]), lo, ctx)?;
    Ok(stream_choose(
        input,
        vec![
            (condition.clone(), true_arm),
            (IrExpr::Not(Box::new(condition)), false_arm),
        ],
    ))
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
    let condition = IrExpr::IsBound(probe);
    let true_arm = lower_arm_continuation(stream_input(), then, lo, ctx)?;
    let false_arm = lower_arm_continuation(stream_input(), else_branch.unwrap_or(&[]), lo, ctx)?;
    Ok(stream_choose(
        probe_apply,
        vec![
            (condition.clone(), true_arm),
            (IrExpr::Not(Box::new(condition)), false_arm),
        ],
    ))
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
    let mut regular_trav = Vec::new();
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
            OptionKey::Traversal(key_steps) if !is_choose => {
                regular_trav.push((key_steps.clone(), opt.traversal.clone()));
            }
            OptionKey::Traversal(_) => {
                return Err(GremlinPlanError::Unsupported(
                    "choose().option(__.dispatch_traversal) keys are not yet wired".to_string(),
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
            regular_trav,
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
    let right = lower_child_traversal(sub, lo, ctx, kind)?;
    if matches!(kind, ChildTraversalKind::SideEffect) {
        // The child is materialized once, including mutations on every result.
        // Its cardinality and current value must not replace the parent row.
        return Ok(Node::GraphApply {
            kind: ApplyKind::Optional,
            correlation: vec![CURRENT.into()],
            outputs: Vec::new(),
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: Node::GraphSlice {
                slice: Slice {
                    offset: 0,
                    fetch: Some(1),
                    tail: None,
                },
                input: right.boxed(),
            }
            .boxed(),
        });
    }
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
    if matches!(kind, ChildTraversalKind::Map) {
        let result=lo.fresh("map_result");
        let right=Node::GraphProject {mode:ProjectMode::PreserveVisible,
            items:vec![ProjectionItem {alias:result.clone(),expr:IrExpr::Binding(CURRENT.into())}],
            error_policy:ProjectErrorPolicy::PropagateError,input:right.boxed()};
        return Ok(super::project::project_value_with_path(Node::GraphApply {
            kind:ApplyKind::Inner,correlation:vec![CURRENT.into()],outputs:vec![result.clone()],
            optional_missing:OptionalMissing::Null,left:input.boxed(),right:right.boxed(),
        },IrExpr::Binding(result)));
    }
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
    let mut arms = Vec::new();
    let mut matched = Vec::new();
    for (condition, steps) in regular {
        let condition = truth_test(condition);
        let selected = IrExpr::and(vec![no_matches(&matched), condition.clone()]);
        matched.push(condition);
        arms.push((
            selected,
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    if let Some(steps) = pick_none.into_iter().next() {
        let condition = if pick_unproductive.is_empty() {
            no_matches(&matched)
        } else {
            IrExpr::and(vec![
                no_matches(&matched),
                productive_condition(dispatch_key),
            ])
        };
        arms.push((
            condition,
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    if let Some(steps) = pick_unproductive.into_iter().next() {
        arms.push((
            IrExpr::and(vec![
                no_matches(&matched),
                unproductive_condition(dispatch_key),
            ]),
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    Ok(stream_choose(input, arms))
}

#[allow(clippy::too_many_arguments)]
fn lower_branch_option_union(
    input: Node,
    dispatch_key: &str,
    regular: Vec<(IrExpr, Vec<Step>)>,
    regular_trav: Vec<(Vec<Step>, Vec<Step>)>,
    pick_any: Vec<Vec<Step>>,
    pick_none: Vec<Vec<Step>>,
    pick_unproductive: Vec<Vec<Step>>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let mut input = input;
    let mut arms = Vec::new();
    let mut matched = Vec::new();
    for (condition, steps) in regular {
        let condition = truth_test(condition);
        matched.push(condition.clone());
        arms.push((
            condition,
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    for (key_steps, steps) in regular_trav {
        // Probe each traversal key once against the dispatch value, preserving
        // the parent traverser for the selected option's body.
        let probe = lo.fresh("branch_match");
        let key_input = Node::GraphProject {
            mode: ProjectMode::ReplaceCurrent,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Binding(dispatch_key.into()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: stream_input().boxed(),
        };
        let key_body = super::sub_traversal::lower_stream_child_traversal(
            key_input,
            &key_steps,
            lo,
            ctx,
            ChildTraversalKind::WherePredicate,
        )?;
        input = Node::GraphApply {
            kind: ApplyKind::Optional,
            correlation: vec![dispatch_key.into()],
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
                input: Node::GraphSlice {
                    slice: Slice {
                        offset: 0,
                        fetch: Some(1),
                        tail: None,
                    },
                    input: key_body.boxed(),
                }
                .boxed(),
            }
            .boxed(),
        };
        let condition = IrExpr::IsBound(probe);
        matched.push(condition.clone());
        arms.push((
            condition,
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    for steps in pick_any {
        arms.push((
            productive_condition(dispatch_key),
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    for steps in pick_none {
        arms.push((
            no_matches(&matched),
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    for steps in pick_unproductive {
        arms.push((
            unproductive_condition(dispatch_key),
            lower_arm_continuation(stream_input(), &steps, lo, ctx)?,
        ));
    }
    Ok(stream_choose(input, arms))
}

fn truth_test(condition: IrExpr) -> IrExpr {
    IrExpr::Case {
        arms: vec![(condition, IrExpr::lit_bool(true))],
        otherwise: Some(Box::new(IrExpr::lit_bool(false))),
    }
}

fn no_matches(conditions: &[IrExpr]) -> IrExpr {
    IrExpr::and(
        conditions
            .iter()
            .cloned()
            .map(|condition| IrExpr::Not(Box::new(condition)))
            .collect(),
    )
}

fn stream_input() -> Node {
    // This existing correlate contract carries complete traverser rows,
    // including labels, paths and bulk, rather than projecting only current.
    Node::GraphCorrelate {
        bindings: vec!["__gremlin_group_members".into()],
    }
}

fn stream_choose(input: Node, arms: Vec<(IrExpr, Node)>) -> Node {
    let (conditions, arms): (Vec<_>, Vec<_>) = arms
        .into_iter()
        .map(|(condition, body)| (condition, ChooseArm { key: None, body }))
        .unzip();
    Node::GraphChoose {
        selector: ChooseSelector::Predicates(conditions),
        output: CURRENT.into(),
        correlation: vec![CURRENT.into()],
        arms,
        default: None,
        unmatched: ChooseUnmatched::Drop,
        input: input.boxed(),
    }
}

/// Arms consume the saved incoming stream; runtime routing skips empty arms.
fn lower_arm_continuation(
    input: Node,
    steps: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    super::sub_traversal::lower_stream_child_traversal(
        input,
        steps,
        lo,
        ctx,
        ChildTraversalKind::BranchArm,
    )
}

fn option_value_condition(dispatch_key: &str, value: &GValue) -> GremlinPlanResult<IrExpr> {
    if matches!(value, GValue::Null) {
        Ok(IrExpr::IsNull(Box::new(IrExpr::Binding(
            dispatch_key.to_string(),
        ))))
    } else {
        Ok(IrExpr::Call {
            name: "gremlin_compare".into(),
            args: vec![IrExpr::lit_str("eq"), IrExpr::Binding(dispatch_key.to_string()), gvalue_to_expr(value)?],
        })
    }
}

fn productive_condition(dispatch_key: &str) -> IrExpr {
    IrExpr::IsBound(dispatch_key.to_string())
}

fn unproductive_condition(dispatch_key: &str) -> IrExpr {
    IrExpr::Not(Box::new(productive_condition(dispatch_key)))
}
