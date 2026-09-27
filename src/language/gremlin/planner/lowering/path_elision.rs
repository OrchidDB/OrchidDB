//! Remove path bookkeeping only after proving that the complete traversal,
//! including every child scope, cannot observe it. Step labels remain columns;
//! historical pop selections keep their separate label histories.
use super::label_liveness;
use crate::ir::plan::Node;
use crate::language::gremlin::ast::{Pop, Step};

pub(super) fn elide_unobserved(steps: &[Step], root: &mut Node) {
    fn safe(steps: &[Step]) -> bool {
        steps.iter().all(|step| {
            matches!(step,
                Step::V { .. } | Step::E { .. } | Step::Inject(_)
                | Step::ExpandVertex { .. } | Step::ExpandEdge { .. }
                | Step::EndpointVertex { .. } | Step::OtherVertex
                | Step::Has { .. } | Step::HasLabel(_) | Step::HasId { .. }
                | Step::HasIdPredicate { .. } | Step::HasNot { .. }
                | Step::Identity | Step::Is { .. } | Step::Values(_)
                | Step::Id | Step::Label | Step::As(_) | Step::Select(_, _)
                | Step::SelectMulti(_, _) | Step::Count | Step::Aggregate(_)
                | Step::Dedup | Step::DedupLabels(_) | Step::Times(_) | Step::Loops(_)
                | Step::Limit(_) | Step::Range { .. } | Step::Skip(_) | Step::Tail(_)
                | Step::WithBulk(_) | Step::Barrier | Step::Constant(_)
                | Step::Group | Step::GroupCount | Step::By(_)
                | Step::Repeat(_, _) | Step::Until(_) | Step::Emit(_)
                | Step::Local(_) | Step::Map(_) | Step::FlatMap(_)
                | Step::WhereString { .. } | Step::WhereAnchor(_)
                | Step::WhereTraversal(_) | Step::NotTraversal(_) | Step::Match(_)
                | Step::Union(_) | Step::Coalesce(_)
            ) && label_liveness::children(step).into_iter().all(safe)
        })
    }
    fn history_observed(steps: &[Step]) -> bool {
        steps.iter().any(|step| matches!(step,
            Step::Select(_, Pop::First | Pop::All | Pop::Mixed)
            | Step::SelectMulti(_, Pop::First | Pop::All | Pop::Mixed))
            || label_liveness::children(step).into_iter().any(history_observed))
    }
    if !safe(steps) { return; }
    // A productive null label is distinguishable from an absent label only
    // by its history. Keep histories unless every as() sees a proven present
    // non-null producer; removing last-pop history otherwise drops nulls.
    fn labels_nonnull(steps: &[Step], mut nonnull: bool, nullable: &mut std::collections::BTreeSet<String>) -> bool {
        for step in steps {
            let children = label_liveness::children(step).into_iter()
                .map(|child| labels_nonnull(child, nonnull, nullable)).collect::<Vec<_>>();
            match step {
                Step::As(label) if !nonnull => { nullable.insert(label.clone()); }
                Step::V { .. } | Step::E { .. } | Step::ExpandVertex { .. }
                | Step::ExpandEdge { .. } | Step::EndpointVertex { .. }
                | Step::OtherVertex | Step::Count => nonnull = true,
                Step::Values(_) | Step::Constant(_) | Step::Inject(_)
                | Step::Select(_, _) | Step::SelectMulti(_, _) | Step::Aggregate(_) => nonnull = false,
                Step::Repeat(_, _) => nonnull &= children.into_iter().all(|present| present),
                Step::Map(_) | Step::FlatMap(_) | Step::Local(_)
                | Step::Union(_) | Step::Coalesce(_) => nonnull = children.into_iter().all(|present| present),
                _ => {}
            }
        }
        nonnull
    }
    let mut nullable = std::collections::BTreeSet::new();
    labels_nonnull(steps, false, &mut nullable);
    let remove_history = (!history_observed(steps)).then_some(&nullable);
    fn dead(name: &str, histories: Option<&std::collections::BTreeSet<String>>) -> bool {
        name == "__path" || name == "__path_labels"
            || name.strip_prefix("__gremlin_select_history_").is_some_and(|label|
                histories.is_some_and(|nullable| !nullable.contains(label)))
    }
    fn rewrite(node: &mut Node, histories: Option<&std::collections::BTreeSet<String>>) {
        for child in crate::ir::analysis::children_mut(node) { rewrite(child, histories); }
        match node {
            Node::GraphExpand { path, .. } | Node::GraphRepeat { path, .. } => { *path = None; }
            Node::GraphProject { items, input, .. } => {
                items.retain(|item| !dead(&item.alias, histories)
                    || (item.alias.starts_with("__gremlin_select_history_")
                        && matches!(item.expr, crate::ir::expr::IrExpr::Lit(crate::ir::expr::Lit::Null))));
                if items.is_empty() { *node = *std::mem::replace(input, Box::new(Node::GraphEmpty)); }
            }
            Node::GraphCorrelate { bindings } => bindings.retain(|name| !dead(name, histories)),
            _ => {}
        }
    }
    rewrite(root, remove_history);
}
