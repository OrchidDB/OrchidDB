//! Label liveness at SelectStep path-processor boundaries.
//!
//! TinkerPop retracts labels after their last scoped use. In a repeat it
//! retains the body's direct scope keys, while child traversal requirements
//! propagate backwards to their parent boundary. A child-only label may
//! therefore start a new history on the next iteration. Full path consumers
//! disable retraction, as in the reference PathRetractionStrategy.
use std::collections::BTreeSet;

use super::context::Lowerer;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::language::gremlin::ast::{CallArg, MutationArgument, OptionKey, Step, StringOp};

type Labels = BTreeSet<String>;

pub(super) fn configure(steps: &[Step], lo: &mut Lowerer) {
    lo.retract_labels = true;
    visit(steps, &mut |step| match step {
        Step::Call(name, _) if name.starts_with("crabgraph.jvm") => lo.retract_labels = false,
        Step::As(label) => { lo.path_labels.insert(label.clone()); }
        Step::WithoutPathRetraction | Step::Path | Step::SimplePath | Step::CyclicPath
        | Step::Tree(_) | Step::ShortestPath
        // These scopes have additional strategy rewrites or dynamic keys;
        // retain their complete label history conservatively.
        | Step::Match(_) | Step::SelectMapValueBy(_) => lo.retract_labels = false,
        _ => {}
    });
}

pub(super) fn direct_references(steps: &[Step]) -> Labels {
    let mut labels = Labels::new();
    for step in steps {
        match step {
            Step::Select(label, _) | Step::SelectMapValueBy(label) | Step::WhereAnchor(label)
            | Step::PathFrom(label) | Step::PathTo(label) => { labels.insert(label.clone()); }
            Step::SelectMulti(keys, _) | Step::DedupLabels(keys) => labels.extend(keys.iter().cloned()),
            Step::WhereTraversal(sub) => {
                // where(as(a)...as(b)) turns boundary labels into scope
                // lookups; ordinary as() steps remain label writes.
                for boundary in sub.first().into_iter().chain(sub.last()) {
                    if let Step::As(label) | Step::WhereAnchor(label) = boundary {
                        labels.insert(label.clone());
                    }
                }
            }
            Step::WhereString { label, predicate } => {
                labels.insert(label.clone());
                let mut refs = Vec::new();
                super::sub_traversal::collect_predicate_binding_refs(predicate, &mut refs);
                labels.extend(refs);
            }
            Step::AddE { from, to, .. } => labels.extend(from.iter().chain(to).cloned()),
            Step::AddDynamicE { label, from, to } => {
                for argument in std::iter::once(label).chain(from).chain(to) {
                    if let MutationArgument::Label(label) = argument { labels.insert(label.clone()); }
                }
            }
            Step::AddDynamicV { label } => {
                if let MutationArgument::Label(label) = label { labels.insert(label.clone()); }
            }
            Step::PropertyDynamic { key, value } | Step::PropertyNative { key, value, .. } => {
                for argument in [key, value] {
                    if let MutationArgument::Label(label) = argument { labels.insert(label.clone()); }
                }
            }
            Step::DynamicMerge { criteria, options, .. } => {
                for argument in std::iter::once(criteria).chain(options.values()) {
                    if let MutationArgument::Label(label) = argument { labels.insert(label.clone()); }
                }
            }
            Step::Format(parts) => {
                for part in parts {
                    if let crate::language::gremlin::ast::FormatPart::Placeholder { key: Some(key) } = part {
                        labels.insert(key.clone());
                    }
                }
            }
            Step::Math(expr) => {
                use crate::language::gremlin::ast::MathExpr;
                match expr {
                    MathExpr::SelfRhsName(_, key) | MathExpr::SelfLhsName(_, key) | MathExpr::Var(key) => { labels.insert(key.clone()); }
                    MathExpr::BothNamed(_, a, b) => { labels.insert(a.clone()); labels.insert(b.clone()); }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    labels
}

pub(super) fn references_iter<'a>(steps: impl Iterator<Item = &'a Step>) -> Labels {
    let mut labels = Labels::new();
    for step in steps {
        labels.extend(direct_references(std::slice::from_ref(step)));
        for child in children(step) { labels.extend(references_iter(child.iter())); }
    }
    labels
}

pub(super) fn retract(input: Node, labels: &Labels, keep: &Labels) -> Node {
    let items = labels.difference(keep).flat_map(|label| [
        ProjectionItem { alias: label.clone(), expr: IrExpr::Lit(Lit::Null) },
        ProjectionItem { alias: format!("__gremlin_select_history_{label}"), expr: IrExpr::Lit(Lit::Null) },
    ]).collect::<Vec<_>>();
    if items.is_empty() { return input; }
    Node::GraphProject { mode: ProjectMode::PreserveVisible, items,
        error_policy: ProjectErrorPolicy::PropagateError, input: input.boxed() }
}

fn visit(steps: &[Step], f: &mut impl FnMut(&Step)) {
    for step in steps {
        f(step);
        for child in children(step) { visit(child, f); }
    }
}

fn children(step: &Step) -> Vec<&[Step]> {
    let mut out = Vec::new();
    match step {
        Step::By(spec) => { if let Some(sub) = &spec.traversal { out.push(sub.as_slice()); } }
        Step::Union(subs) | Step::Coalesce(subs) | Step::Match(subs) => out.extend(subs.iter().map(Vec::as_slice)),
        Step::BranchOptions { dispatch, options, .. } => {
            out.push(dispatch.as_slice());
            for option in options {
                if let OptionKey::Traversal(sub) = &option.key { out.push(sub.as_slice()); }
                out.push(option.traversal.as_slice());
            }
        }
        Step::ChoosePredicate { then, else_branch, .. } => {
            out.push(then.as_slice());
            if let Some(sub) = else_branch { out.push(sub.as_slice()); }
        }
        Step::ChooseTraversal { condition, then, else_branch } => {
            out.push(condition.as_slice()); out.push(then.as_slice());
            if let Some(sub) = else_branch { out.push(sub.as_slice()); }
        }
        Step::Local(sub) | Step::Map(sub) | Step::FlatMap(sub) | Step::SideEffect(sub)
        | Step::WhereTraversal(sub) | Step::NotTraversal(sub) | Step::Repeat(_, sub)
        | Step::Until(sub) | Step::ListOpTraversal(_, sub) | Step::Emit(Some(sub))
        | Step::PropertyTraversal { traversal: sub, .. }
        | Step::WithOption { traversal: Some(sub), .. }
        | Step::StringOp(StringOp::ConcatTraversal(sub)) => out.push(sub.as_slice()),
        Step::WithStrategy { vertex_filter, edge_filter, vertex_property_filter, .. } => {
            out.extend([vertex_filter, edge_filter, vertex_property_filter].into_iter().flatten().map(Vec::as_slice));
        }
        Step::Call(_, args) => { for arg in args { if let CallArg::Traversal(sub) = arg { out.push(sub.as_slice()); } } }
        Step::DynamicMerge { criteria, options, .. } => {
            for arg in std::iter::once(criteria).chain(options.values()) { argument_child(arg, &mut out); }
        }
        Step::AddDynamicV { label } => argument_child(label, &mut out),
        Step::AddDynamicE { label, from, to } => {
            for arg in std::iter::once(label).chain(from).chain(to) { argument_child(arg, &mut out); }
        }
        Step::PropertyDynamic { key, value } | Step::PropertyNative { key, value, .. } => { argument_child(key, &mut out); argument_child(value, &mut out); }
        Step::LocalScoped(inner) => out.push(std::slice::from_ref(inner.as_ref())),
        _ => {}
    }
    out
}

fn argument_child<'a>(arg: &'a MutationArgument, out: &mut Vec<&'a [Step]>) {
    if let MutationArgument::Traversal(sub) = arg { out.push(sub.as_slice()); }
}
