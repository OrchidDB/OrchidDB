//! `match(t1, t2, ...)` — labelled traversal patterns sharing one
//! match environment.
//!
//! A correct implementation would run a join over the labelled
//! bindings declared by each pattern. As a first cut we fold the
//! patterns into a chain of `Apply Inner` against `current` so each
//! pattern's bindings flow into the row stream visible to downstream
//! `select(...)`. Repeated label declarations across patterns are
//! handled by the join semantics of nested Apply: a pattern that
//! redefines a binding constrains it to equality with the prior value.
//!
//! Rows pass `match` only when *every* pattern produces ≥1 result.
//! `Apply Inner` enforces that — empty arms drop the outer row.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{ApplyKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::ir::policy::OptionalMissing;
use crate::language::gremlin::ast::{CallArg, OptionKey, Pop, Step, TraversalOption};
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_match(
    input: Node,
    patterns: &[Vec<Step>],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let mut acc = input;
    let mut seen_labels = Vec::new();
    for pattern in patterns {
        let mut pattern = pattern.clone();
        if let Some(Step::As(label)) = pattern.first() {
            if seen_labels.iter().any(|seen| seen == label) {
                pattern.insert(0, Step::Select(label.clone(), Pop::Last));
            }
        }
        let probe = lower_child_traversal(&pattern, lo, ctx, ChildTraversalKind::MatchPattern)?;
        let outputs = pattern_outputs(&pattern);
        for output in &outputs {
            if !seen_labels.contains(output) {
                seen_labels.push(output.clone());
            }
        }
        acc = Node::GraphApply {
            kind: ApplyKind::Inner,
            correlation: Vec::new(),
            // Carry through whatever bindings the sub-traversal declared
            // so that downstream `select(...)` can see them.
            outputs,
            optional_missing: OptionalMissing::Null,
            left: acc.boxed(),
            right: probe.boxed(),
        };
    }
    // After all patterns have joined, Match's contract is to emit a
    // Map<label, value> as the current traverser. Downstream `select(...)`
    // by label still works because the bindings remain in scope, and any
    // consumer expecting the matched-row map (default Match output) sees
    // it directly.
    if !seen_labels.is_empty() {
        let mut entries = Vec::with_capacity(seen_labels.len() * 2);
        for label in &seen_labels {
            entries.push(IrExpr::Lit(Lit::String(label.clone())));
            entries.push(IrExpr::Binding(label.clone()));
        }
        acc = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Call {
                    name: "make_map".into(),
                    args: entries,
                },
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: acc.boxed(),
        };
    }
    Ok(acc)
}

/// Walk the pattern collecting `as(label)` declarations so the wrapping
/// Apply's `outputs` carries them out of the sub-traversal scope.
fn pattern_outputs(pattern: &[Step]) -> Vec<String> {
    let mut labels = Vec::new();
    for step in pattern {
        collect_step_outputs(step, &mut labels);
    }
    labels
}

fn push_output(labels: &mut Vec<String>, label: &str) {
    if !labels.iter().any(|existing| existing == label) {
        labels.push(label.to_string());
    }
}

fn collect_steps_outputs(steps: &[Step], labels: &mut Vec<String>) {
    for step in steps {
        collect_step_outputs(step, labels);
    }
}

fn collect_step_outputs(step: &Step, labels: &mut Vec<String>) {
    match step {
        Step::As(label) => push_output(labels, label),
        Step::Union(branches) | Step::Coalesce(branches) | Step::Match(branches) => {
            for branch in branches {
                collect_steps_outputs(branch, labels);
            }
        }
        Step::BranchOptions {
            dispatch, options, ..
        } => {
            collect_steps_outputs(dispatch, labels);
            for option in options {
                collect_option_outputs(option, labels);
            }
        }
        Step::ChoosePredicate {
            then, else_branch, ..
        } => {
            collect_steps_outputs(then, labels);
            if let Some(else_branch) = else_branch {
                collect_steps_outputs(else_branch, labels);
            }
        }
        Step::ChooseTraversal {
            condition,
            then,
            else_branch,
        } => {
            collect_steps_outputs(condition, labels);
            collect_steps_outputs(then, labels);
            if let Some(else_branch) = else_branch {
                collect_steps_outputs(else_branch, labels);
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
        | Step::ListOpTraversal(_, sub) => collect_steps_outputs(sub, labels),
        Step::Emit(Some(sub)) => collect_steps_outputs(sub, labels),
        Step::LocalScoped(inner) => collect_step_outputs(inner, labels),
        Step::By(spec) => {
            if let Some(sub) = &spec.traversal {
                collect_steps_outputs(sub, labels);
            }
        }
        Step::Call(_, args) => {
            for arg in args {
                if let CallArg::Traversal(sub) = arg {
                    collect_steps_outputs(sub, labels);
                }
            }
        }
        _ => {}
    }
}

fn collect_option_outputs(option: &TraversalOption, labels: &mut Vec<String>) {
    if let OptionKey::Traversal(sub) = &option.key {
        collect_steps_outputs(sub, labels);
    }
    collect_steps_outputs(&option.traversal, labels);
}
