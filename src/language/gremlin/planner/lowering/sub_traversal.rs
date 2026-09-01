//! Contextful traversal entry points.
//!
//! Root traversals start from an explicit source step. Anonymous child
//! traversals (`where(__...)`, `local(__...)`, `repeat(__...)`, branch arms,
//! etc.) start from the parent traverser's `current` binding through
//! `GraphCorrelate`. The parent must name the child contract so the lowerer
//! does not infer semantics from a raw `Vec<Step>`.

use super::context::{
    CURRENT, ChildTraversalKind, Lowerer, PATH, TraversalContext, TraversalStart,
};
use super::dispatch::lower_step_with_context;
use super::sources::source_node;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::language::gremlin::ast::{
    CallArg, FormatPart, MathExpr, OptionKey, Step, TraversalOption,
};
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};
use crate::language::gremlin::semantics::{GValue, Predicate};

pub(super) fn lower_source_traversal_with_context(
    steps: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    debug_assert_eq!(ctx.contract().start, TraversalStart::Source);
    let rewritten = rewrite_infix_connectives(steps);
    let steps = rewritten.as_deref().unwrap_or(steps);
    let (first, rest) = steps
        .split_first()
        .ok_or_else(|| GremlinPlanError::Parse("empty traversal".to_string()))?;
    let node = source_node(first, lo, ctx)?;
    lower_remaining_steps(node, rest, lo, ctx)
}

/// ConnectiveStrategy-style rewrite: fold infix `.and()` / `.or()` markers
/// into explicit filter combinators.
///
/// `prefix a1 .and() a2 .or() b` becomes
/// `prefix where(union(and-branch(a1, a2), b))` where the and-branch is a
/// chain of `WhereTraversal` semi-joins. The retained prefix is the leading
/// run of spawn / labelling steps (`V`/`E`/`inject`/`as`/`identity`) that
/// produce the stream the connective filters.
///
/// Returns `None` when the step list contains no infix markers.
pub(super) fn rewrite_infix_connectives(steps: &[Step]) -> Option<Vec<Step>> {
    if !steps
        .iter()
        .any(|s| matches!(s, Step::InfixAnd | Step::InfixOr))
    {
        return None;
    }
    let prefix_len = steps
        .iter()
        .take_while(|s| {
            matches!(
                s,
                Step::V { .. } | Step::E { .. } | Step::Inject(_) | Step::As(_) | Step::Identity
            )
        })
        .count();
    let (prefix, rest) = steps.split_at(prefix_len);

    // Split at top-level `InfixOr` first (lowest precedence), then each
    // or-branch at `InfixAnd`.
    let or_segments: Vec<Vec<Step>> = split_at_marker(rest, |s| matches!(s, Step::InfixOr));
    let mut or_branches: Vec<Vec<Step>> = Vec::with_capacity(or_segments.len());
    for or_seg in &or_segments {
        let and_segments: Vec<Vec<Step>> = split_at_marker(or_seg, |s| matches!(s, Step::InfixAnd));
        if and_segments.len() == 1 {
            or_branches.push(and_segments.into_iter().next().unwrap_or_default());
        } else {
            or_branches.push(and_segments.into_iter().map(Step::WhereTraversal).collect());
        }
    }

    let mut out: Vec<Step> = prefix.to_vec();
    if or_branches.len() == 1 {
        // Pure and-chain: keep the filters inline on the main stream.
        out.extend(or_branches.into_iter().next().unwrap_or_default());
    } else {
        out.push(Step::WhereTraversal(vec![Step::Union(or_branches)]));
    }
    Some(out)
}

fn split_at_marker(steps: &[Step], is_marker: impl Fn(&Step) -> bool) -> Vec<Vec<Step>> {
    let mut segments = vec![Vec::new()];
    for step in steps {
        if is_marker(step) {
            segments.push(Vec::new());
        } else {
            segments
                .last_mut()
                .expect("segments is never empty")
                .push(step.clone());
        }
    }
    segments.retain(|seg| !seg.is_empty());
    if segments.is_empty() {
        segments.push(Vec::new());
    }
    segments
}

pub(super) fn lower_child_traversal(
    steps: &[Step],
    lo: &mut Lowerer,
    parent: &TraversalContext,
    kind: ChildTraversalKind,
) -> GremlinPlanResult<Node> {
    let ctx = lo.child_context(parent, kind);
    lo.enter_context(ctx, |lo, ctx| {
        let contract = ctx.contract();
        let _expected_output = contract.output;
        match contract.start {
            TraversalStart::Source => lower_source_traversal_with_context(steps, lo, ctx),
            TraversalStart::CorrelatedCurrent => {
                lower_correlated_traversal_with_context(steps, lo, ctx)
            }
        }
    })
}

fn lower_correlated_traversal_with_context(
    steps: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    debug_assert_eq!(ctx.contract().start, TraversalStart::CorrelatedCurrent);
    let rewritten = rewrite_infix_connectives(steps);
    let steps = rewritten.as_deref().unwrap_or(steps);
    let mut node = Node::GraphCorrelate {
        bindings: correlated_bindings(ctx.current_binding(), steps),
    };
    if let Some(Step::As(label)) = steps.first() {
        node = prefer_existing_label_as_current(node, label);
    }
    lower_remaining_steps(node, steps, lo, ctx)
}

fn lower_remaining_steps(
    mut node: Node,
    steps: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let mut iter = steps.iter().peekable();
    while let Some(step) = iter.next() {
        node = lower_step_with_context(node, step, &mut iter, lo, ctx)?;
    }
    Ok(node)
}

fn correlated_bindings(current: &str, steps: &[Step]) -> Vec<String> {
    let mut bindings = vec![current.to_string()];
    push_binding(&mut bindings, PATH);
    collect_label_refs(steps, &mut bindings);
    bindings
}

fn push_binding(bindings: &mut Vec<String>, label: &str) {
    if !label.is_empty() && label != "_" && !bindings.iter().any(|existing| existing == label) {
        bindings.push(label.to_string());
    }
}

fn collect_label_refs(steps: &[Step], bindings: &mut Vec<String>) {
    for step in steps {
        collect_step_refs(step, bindings);
    }
}

fn collect_step_refs(step: &Step, bindings: &mut Vec<String>) {
    match step {
        Step::As(label) | Step::WhereAnchor(label) => {
            push_binding(bindings, label);
            push_select_history_binding(bindings, label);
        }
        Step::Select(label, _) => {
            push_binding(bindings, label);
            push_select_history_binding(bindings, label);
        }
        Step::SelectMulti(labels, _) => {
            for label in labels {
                push_binding(bindings, label);
                push_select_history_binding(bindings, label);
            }
        }
        Step::DedupLabels(labels) => {
            for label in labels {
                push_binding(bindings, label);
            }
        }
        Step::WhereString { label, predicate } => {
            push_binding(bindings, label);
            collect_predicate_binding_refs(predicate, bindings);
        }
        Step::PathFrom(label) | Step::PathTo(label) => push_binding(bindings, label),
        Step::Sack | Step::SackOp(_) => push_binding(bindings, super::side_effects::SACK),
        Step::Format(parts) => collect_format_refs(parts, bindings),
        Step::Math(expr) => collect_math_refs(expr, bindings),
        Step::By(spec) => {
            if let Some(sub) = &spec.traversal {
                collect_label_refs(sub, bindings);
            }
        }
        Step::Union(branches) | Step::Coalesce(branches) => {
            for branch in branches {
                collect_label_refs(branch, bindings);
            }
        }
        Step::BranchOptions {
            dispatch, options, ..
        } => {
            collect_label_refs(dispatch, bindings);
            for option in options {
                collect_option_refs(option, bindings);
            }
        }
        Step::ChoosePredicate {
            then, else_branch, ..
        } => {
            collect_label_refs(then, bindings);
            if let Some(else_branch) = else_branch {
                collect_label_refs(else_branch, bindings);
            }
        }
        Step::ChooseTraversal {
            condition,
            then,
            else_branch,
        } => {
            collect_label_refs(condition, bindings);
            collect_label_refs(then, bindings);
            if let Some(else_branch) = else_branch {
                collect_label_refs(else_branch, bindings);
            }
        }
        Step::Local(sub)
        | Step::Map(sub)
        | Step::FlatMap(sub)
        | Step::SideEffect(sub)
        | Step::WhereTraversal(sub)
        | Step::NotTraversal(sub)
        | Step::Repeat(_, sub)
        | Step::Until(sub)
        | Step::ListOpTraversal(_, sub) => collect_label_refs(sub, bindings),
        Step::Emit(Some(sub)) => collect_label_refs(sub, bindings),
        Step::Match(patterns) => {
            for pattern in patterns {
                collect_label_refs(pattern, bindings);
            }
        }
        Step::LocalScoped(inner) => collect_step_refs(inner, bindings),
        Step::Call(_, args) => {
            for arg in args {
                if let CallArg::Traversal(sub) = arg {
                    collect_label_refs(sub, bindings);
                }
            }
        }
        _ => {}
    }
}

fn push_select_history_binding(bindings: &mut Vec<String>, label: &str) {
    if !label.is_empty() && label != "_" {
        push_binding(bindings, &format!("__gremlin_select_history_{label}"));
    }
}

fn prefer_existing_label_as_current(input: Node, label: &str) -> Node {
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.into(),
            expr: IrExpr::Case {
                arms: vec![(
                    IrExpr::IsBound(label.to_string()),
                    IrExpr::Binding(label.to_string()),
                )],
                otherwise: Some(Box::new(IrExpr::Binding(CURRENT.into()))),
            },
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

fn collect_option_refs(option: &TraversalOption, bindings: &mut Vec<String>) {
    if let OptionKey::Traversal(sub) = &option.key {
        collect_label_refs(sub, bindings);
    }
    collect_label_refs(&option.traversal, bindings);
}

fn collect_format_refs(parts: &[FormatPart], bindings: &mut Vec<String>) {
    for part in parts {
        if let FormatPart::Placeholder { key: Some(label) } = part {
            push_binding(bindings, label);
        }
    }
}

fn collect_math_refs(expr: &MathExpr, bindings: &mut Vec<String>) {
    match expr {
        MathExpr::SelfRhsName(_, label)
        | MathExpr::SelfLhsName(_, label)
        | MathExpr::Var(label) => push_binding(bindings, label),
        MathExpr::BothNamed(_, lhs, rhs) => {
            push_binding(bindings, lhs);
            push_binding(bindings, rhs);
        }
        _ => {}
    }
}

fn collect_predicate_binding_refs(predicate: &Predicate, bindings: &mut Vec<String>) {
    match predicate {
        Predicate::Compare {
            value: GValue::String(label),
            ..
        } => push_binding(bindings, label),
        Predicate::And(lhs, rhs) | Predicate::Or(lhs, rhs) => {
            collect_predicate_binding_refs(lhs, bindings);
            collect_predicate_binding_refs(rhs, bindings);
        }
        Predicate::Not(inner) => collect_predicate_binding_refs(inner, bindings),
        _ => {}
    }
}
