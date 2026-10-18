//! Demand reduction for Cypher path payloads. Traversal and trail history are
//! deliberately unchanged: a dead result path does not make its rows dead.
use super::*;
use crate::ir::plan::{ApplyKind, PathMaterialization, PathUpdate, ProjectMode};

type Lengths = BTreeMap<String, i64>;

pub(super) fn specialize_plan(plan: &GraphPlan) -> GraphPlan {
    let mut result = plan.clone();
    if result.policy.language != Language::Cypher {
        return result;
    }
    // A name produced more than once may represent an extended path, or a
    // shadowed binding. Do not infer its length from one local expansion.
    let mut producers = BTreeMap::new();
    count_paths(&result.root, &mut producers);
    fold_lengths(&mut result.root, &producers);
    fold_nonnull_path_counts(&mut result.root);
    if let Node::GraphReturn { fields, .. } = result.root.as_ref() {
        let needed = fields.iter().cloned().collect();
        prune(&mut result.root, &needed);
    }
    result
}

// Presence is proved forward through scopes, independently of path length.
// Only ordinary (non-DISTINCT) COUNT can discard a non-null path payload.
fn fold_nonnull_path_counts(node: &mut Node) -> BTreeSet<String> {
    match node {
        Node::GraphExpand { input, source, target, rel_binding, path, path_materialization, .. } => {
            let mut paths = fold_nonnull_path_counts(input);
            paths.remove(source);
            paths.remove(target);
            if let Some(rel) = rel_binding { paths.remove(rel); }
            if let Some(path) = path {
                paths.remove(path);
                if *path_materialization != PathMaterialization::None { paths.insert(path.clone()); }
            }
            paths
        }
        Node::GraphProject { input, items, mode, .. } => {
            let paths = fold_nonnull_path_counts(input);
            let mut output = if *mode == ProjectMode::PreserveVisible { paths.clone() } else { BTreeSet::new() };
            for item in items {
                output.remove(&item.alias);
                if let IrExpr::Binding(binding) = &item.expr
                    && paths.contains(binding) { output.insert(item.alias.clone()); }
            }
            output
        }
        Node::GraphApply { kind, outputs, left, right, .. } => {
            let mut paths = fold_nonnull_path_counts(left);
            let right_paths = fold_nonnull_path_counts(right);
            for output in outputs {
                paths.remove(output);
                if *kind == ApplyKind::Inner && right_paths.contains(output) { paths.insert(output.clone()); }
            }
            paths
        }
        Node::GraphAggregate { aggs, input, .. } => {
            let paths = fold_nonnull_path_counts(input);
            for agg in aggs {
                if agg.kind == AggKind::CountRows && !agg.distinct
                    && matches!(&agg.arg, Some(IrExpr::Binding(binding)) if paths.contains(binding)) {
                    agg.arg = None;
                }
            }
            BTreeSet::new()
        }
        Node::GraphFilter { input, .. } | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. } | Node::GraphReturn { input, .. } => fold_nonnull_path_counts(input),
        _ => {
            for child in crate::ir::analysis::children_mut(node) { fold_nonnull_path_counts(child); }
            BTreeSet::new()
        }
    }
}

fn count_paths(node: &Node, counts: &mut BTreeMap<String, usize>) {
    if let Node::GraphExpand {
        path: Some(path), ..
    } = node
    {
        *counts.entry(path.clone()).or_default() += 1;
    }
    for child in crate::ir::analysis::children(node) {
        count_paths(child, counts);
    }
}

fn visit_expr(expr: &mut IrExpr, f: &mut impl FnMut(&mut IrExpr)) {
    match expr {
        IrExpr::Binary { lhs, rhs, .. } => {
            visit_expr(lhs, f);
            visit_expr(rhs, f);
        }
        IrExpr::Not(x) | IrExpr::IsNull(x) | IrExpr::IsNotNull(x) => visit_expr(x, f),
        IrExpr::StringPredicate {
            target, pattern, ..
        } => {
            visit_expr(target, f);
            visit_expr(pattern, f);
        }
        IrExpr::List(xs) | IrExpr::Call { args: xs, .. } => {
            for x in xs {
                visit_expr(x, f);
            }
        }
        // Lambda bindings can shadow outer path names. Their bodies retain
        // runtime semantics; references are collected separately below.
        IrExpr::ListReduce { collection, .. } => visit_expr(collection, f),
        IrExpr::ListTransform { list, .. } | IrExpr::ListFilter { list, .. } | IrExpr::Quantifier { list, .. } => visit_expr(list, f),
        IrExpr::Case { arms, otherwise } => {
            for (a, b) in arms {
                visit_expr(a, f);
                visit_expr(b, f);
            }
            if let Some(x) = otherwise {
                visit_expr(x, f);
            }
        }
        _ => {}
    }
    f(expr);
}

fn fold(expr: &mut IrExpr, lengths: &Lengths) {
    visit_expr(expr, &mut |expr| {
        if let IrExpr::Call { name, args } = expr {
            if name == "length" {
                if let [IrExpr::Binding(binding)] = args.as_slice() {
                    if let Some(n) = lengths.get(binding) {
                        *expr = IrExpr::lit_int(*n);
                    }
                }
            }
        }
    });
}

// Prove the expansion starts a new path instead of extending a value from an
// unmodelled input. A missing entry in Lengths alone is not such a proof.
fn absent_path(node: &Node, path: &str) -> bool {
    match node {
        Node::GraphNodeScan { binding, .. } | Node::GraphRelScan { binding, .. } => binding != path,
        Node::GraphOneRow => true,
        Node::GraphCorrelate { bindings, .. } | Node::GraphValues { bindings, .. } => {
            !bindings.iter().any(|b| b == path)
        }
        Node::GraphFilter { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. } => absent_path(input, path),
        Node::GraphExpand {
            path: previous,
            target,
            rel_binding,
            input,
            ..
        } => {
            previous.as_deref() != Some(path)
                && target != path
                && rel_binding.as_deref() != Some(path)
                && absent_path(input, path)
        }
        Node::GraphProject {
            items, mode, input, ..
        } => {
            !items.iter().any(|item| item.alias == path)
                && (*mode == ProjectMode::ReplaceScope || absent_path(input, path))
        }
        _ => false,
    }
}

fn fold_lengths(node: &mut Node, producers: &BTreeMap<String, usize>) -> Lengths {
    match node {
        Node::GraphExpand {
            input,
            source,
            target,
            rel_binding,
            path,
            length,
            path_update,
            ..
        } => {
            let mut lengths = fold_lengths(input, producers);
            lengths.remove(source);
            lengths.remove(target);
            if let Some(rel) = rel_binding {
                lengths.remove(rel);
            }
            if let Some(path) = path {
                lengths.remove(path);
                if length.is_single_hop()
                    && *path_update == PathUpdate::None
                    && producers.get(path) == Some(&1)
                    && absent_path(input, path)
                {
                    lengths.insert(path.clone(), 1);
                }
            }
            lengths
        }
        Node::GraphProject {
            items, mode, input, ..
        } => {
            let lengths = fold_lengths(input, producers);
            let mut output = if *mode == ProjectMode::PreserveVisible {
                lengths.clone()
            } else {
                Lengths::new()
            };
            for item in items {
                fold(&mut item.expr, &lengths);
                output.remove(&item.alias);
                if let IrExpr::Binding(binding) = &item.expr {
                    if let Some(n) = lengths.get(binding) {
                        output.insert(item.alias.clone(), *n);
                    }
                }
            }
            output
        }
        Node::GraphFilter { condition, input } => {
            let lengths = fold_lengths(input, producers);
            fold(condition, &lengths);
            lengths
        }
        Node::GraphSort { keys, input } => {
            let lengths = fold_lengths(input, producers);
            for key in keys {
                fold(&mut key.expr, &lengths);
            }
            lengths
        }
        Node::GraphAggregate {
            group, aggs, input, ..
        } => {
            let lengths = fold_lengths(input, producers);
            for item in group {
                fold(&mut item.expr, &lengths);
            }
            for agg in aggs {
                if let Some(arg) = &mut agg.arg {
                    fold(arg, &lengths);
                }
            }
            Lengths::new()
        }
        Node::GraphApply {
            kind,
            outputs,
            left,
            right,
            ..
        } => {
            let mut lengths = fold_lengths(left, producers);
            let right_lengths = fold_lengths(right, producers);
            for output in outputs {
                lengths.remove(output);
                // OPTIONAL and scalar apply can produce NULL on missing rows.
                if *kind == ApplyKind::Inner {
                    if let Some(n) = right_lengths.get(output) {
                        lengths.insert(output.clone(), *n);
                    }
                }
            }
            lengths
        }
        Node::GraphReturn { input, .. } | Node::GraphSlice { input, .. } => {
            fold_lengths(input, producers)
        }
        _ => {
            // Child scopes may still be optimized, but no fact crosses an
            // unmodelled operator, join, optional boundary or rebinding.
            for child in crate::ir::analysis::children_mut(node) {
                fold_lengths(child, producers);
            }
            Lengths::new()
        }
    }
}

fn refs(expr: &IrExpr, needed: &mut BTreeSet<String>) {
    match expr {
        IrExpr::Binding(b)
        | IrExpr::Id(b)
        | IrExpr::Label(b)
        | IrExpr::IsBound(b)
        | IrExpr::SimplePath(b)
        | IrExpr::Property { binding: b, .. }
        | IrExpr::HasLabel { binding: b, .. } => {
            needed.insert(b.clone());
        }
        IrExpr::Binary { lhs, rhs, .. } => {
            refs(lhs, needed);
            refs(rhs, needed);
        }
        IrExpr::Not(x) | IrExpr::IsNull(x) | IrExpr::IsNotNull(x) => refs(x, needed),
        IrExpr::StringPredicate {
            target, pattern, ..
        } => {
            refs(target, needed);
            refs(pattern, needed);
        }
        IrExpr::List(xs) | IrExpr::Call { args: xs, .. } => {
            for x in xs {
                refs(x, needed);
            }
        }
        IrExpr::ListReduce {
            collection, map, ..
        } => {
            refs(collection, needed);
            refs(map, needed);
        }
        IrExpr::ListTransform { list, map, .. } => {
            refs(list, needed);
            refs(map, needed);
        }
        IrExpr::ListFilter {
            list, predicate, ..
        } | IrExpr::Quantifier { list, predicate, .. } => {
            refs(list, needed);
            refs(predicate, needed);
        }
        IrExpr::Case { arms, otherwise } => {
            for (a, b) in arms {
                refs(a, needed);
                refs(b, needed);
            }
            if let Some(x) = otherwise {
                refs(x, needed);
            }
        }
        IrExpr::Lit(_) => {}
    }
}

fn prune(node: &mut Node, demanded: &BTreeSet<String>) {
    let mut needed = demanded.clone();
    match node {
        Node::GraphReturn { fields, input, .. } => {
            needed.extend(fields.iter().cloned());
            prune(input, &needed);
        }
        Node::GraphProject {
            items, mode, input, ..
        } => {
            if *mode != ProjectMode::PreserveVisible {
                needed.clear();
            }
            // Discard only unused aliases of proven non-null paths. Reading
            // such a binding has no evaluation effect, unlike an arbitrary
            // expression. This allows COUNT(path) through a WITH alias to
            // stop demanding the path payload after its count was simplified.
            let paths = fold_nonnull_path_counts(input);
            items.retain(|item| demanded.contains(&item.alias)
                || !matches!(&item.expr, IrExpr::Binding(binding) if paths.contains(binding)));
            for item in items {
                refs(&item.expr, &mut needed);
            }
            prune(input, &needed);
        }
        Node::GraphAggregate {
            group, aggs, input, ..
        } => {
            needed.clear();
            for item in group {
                refs(&item.expr, &mut needed);
            }
            for agg in aggs {
                if let Some(arg) = &agg.arg {
                    refs(arg, &mut needed);
                }
            }
            prune(input, &needed);
        }
        Node::GraphFilter { condition, input } => {
            refs(condition, &mut needed);
            prune(input, &needed);
        }
        Node::GraphSort { keys, input } => {
            for key in keys {
                refs(&key.expr, &mut needed);
            }
            prune(input, &needed);
        }
        Node::GraphSlice { input, .. } => prune(input, &needed),
        Node::GraphApply {
            kind,
            correlation,
            outputs,
            left,
            right,
            ..
        } if matches!(kind, ApplyKind::Inner | ApplyKind::Optional) => {
            needed.extend(correlation.iter().cloned());
            // Dropping only proven path output names avoids changing scalar
            // cardinality checks or the schema of unrelated outputs.
            let mut paths = BTreeMap::new();
            count_paths(right, &mut paths);
            outputs.retain(|name| needed.contains(name) || !paths.contains_key(name));
            let mut right_needed = needed.clone();
            right_needed.extend(outputs.iter().cloned());
            prune(right, &right_needed);
            prune(left, &needed);
        }
        Node::GraphExpand {
            source,
            target,
            rel_binding,
            path,
            history,
            path_materialization,
            path_mode,
            path_update,
            input,
            ..
        } => {
            if *path_mode == crate::ir::policy::PathMode::Walk
                && *path_update == PathUpdate::None
                && path.as_ref().is_some_and(|name| !needed.contains(name))
            {
                *path = None;
                *path_materialization = PathMaterialization::None;
            }
            needed.insert(source.clone());
            needed.insert(target.clone());
            if let Some(rel) = rel_binding {
                needed.insert(rel.clone());
            }
            if let Some(path) = path {
                needed.insert(path.clone());
            }
            if let Some(history) = history {
                needed.insert(history.clone());
            }
            prune(input, &needed);
        }
        // DISTINCT over the whole row, unknown operators and correlated
        // scopes are barriers. Never guess that a path cannot affect them.
        _ => {}
    }
}
