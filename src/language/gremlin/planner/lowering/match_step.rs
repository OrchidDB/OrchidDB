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
    // `and(p1, p2)` inside match parses to a run of WhereTraversal steps;
    // TinkerPop's MatchStep treats those as first-class patterns whose
    // labels join the match environment. Flatten them, then order the
    // patterns solver-style: binding patterns run start-label-bound-first
    // and pure filters (where clauses, or()-filters) run last.
    let patterns = solver_ordered_patterns(patterns);
    let mut acc = input;
    let mut seen_labels = Vec::new();
    for pattern in &patterns {
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

/// Flatten `and()`-style multi-WhereTraversal patterns and order the
/// result: binding patterns (leading `as(label)`) are arranged so each
/// one's start label is already bound when it runs (the incoming
/// traverser binds the first pattern's start), and filter-only patterns
/// run after every label they reference has been bound.
fn solver_ordered_patterns(patterns: &[Vec<Step>]) -> Vec<Vec<Step>> {
    fn flatten_one(pattern: &[Step], out: &mut Vec<Vec<Step>>) {
        let all_where =
            pattern.len() > 1 && pattern.iter().all(|s| matches!(s, Step::WhereTraversal(_)));
        if all_where {
            for step in pattern {
                if let Step::WhereTraversal(inner) = step {
                    flatten_one(inner, out);
                }
            }
        } else {
            out.push(pattern.to_vec());
        }
    }
    let mut flattened = Vec::new();
    for pattern in patterns {
        flatten_one(pattern, &mut flattened);
    }

    let (mut binding, filters): (Vec<Vec<Step>>, Vec<Vec<Step>>) = flattened
        .into_iter()
        .partition(|p| matches!(p.first(), Some(Step::As(_))));

    let start_label = |p: &[Step]| match p.first() {
        Some(Step::As(label)) => label.clone(),
        _ => String::new(),
    };
    // Labels produced by a pattern beyond its start label.
    let mut non_start_outputs: Vec<String> = Vec::new();
    for p in &binding {
        let start = start_label(p);
        let mut outputs = Vec::new();
        for step in p {
            collect_step_outputs(step, &mut outputs);
        }
        for label in outputs {
            if label != start && !non_start_outputs.contains(&label) {
                non_start_outputs.push(label);
            }
        }
    }

    let mut ordered = Vec::with_capacity(binding.len());
    let mut bound: Vec<String> = Vec::new();
    while !binding.is_empty() {
        let idx = binding
            .iter()
            .position(|p| bound.contains(&start_label(p)))
            .or_else(|| {
                if ordered.is_empty() {
                    // First pick: prefer a root — a start label no other
                    // pattern produces, so the incoming traverser can
                    // bind it without stranding another pattern.
                    binding
                        .iter()
                        .position(|p| !non_start_outputs.contains(&start_label(p)))
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let p = binding.remove(idx);
        let mut outputs = Vec::new();
        for step in &p {
            collect_step_outputs(step, &mut outputs);
        }
        for label in outputs {
            if !bound.contains(&label) {
                bound.push(label);
            }
        }
        ordered.push(p);
    }
    ordered.extend(filters);
    ordered
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
