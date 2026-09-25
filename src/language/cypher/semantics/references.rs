//! Free-variable collection and nested query scope tracking.

use super::validation::{default_procedure_yields, pattern_binding_names};
use super::{
    BTreeSet, BindingKind, Clause, Expr, PatternPart, ProjectionBody, Query, SemanticScope,
    merge_pattern_properties, merge_pattern_variables, merge_set_item_exprs,
};
pub(super) fn scope_from_candidates(candidates: &BTreeSet<String>) -> SemanticScope {
    SemanticScope {
        bindings: candidates
            .iter()
            .map(|binding| (binding.clone(), BindingKind::Unknown))
            .collect(),
    }
}

pub(super) fn collect_free_variables(
    expr: &Expr,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Variable(name) => {
            if !bound.contains(name) {
                out.insert(name.clone());
            }
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            collect_free_variables(target, bound, out);
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            collect_free_variables(expr, bound, out);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            collect_free_variables(lhs, bound, out);
            collect_free_variables(rhs, bound, out);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                collect_free_variables(arg, bound, out);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                collect_free_variables(value, bound, out);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                collect_free_variables(case, bound, out);
            }
            for (when, then) in arms {
                collect_free_variables(when, bound, out);
                collect_free_variables(then, bound, out);
            }
            if let Some(otherwise) = otherwise {
                collect_free_variables(otherwise, bound, out);
            }
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                if let Some(predicate) = predicate {
                    collect_free_variables(predicate, bound, out);
                }
                collect_free_variables(map, bound, out);
            });
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound_many(bound, [accumulator, variable], |bound| {
                collect_free_variables(map, bound, out);
            });
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                collect_free_variables(map, bound, out);
            });
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                collect_free_variables(predicate, bound, out);
            });
        }
        Expr::Quantifier {
            variable,
            collection,
            predicate,
            ..
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                collect_free_variables(predicate, bound, out);
            });
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let mut names = pattern_binding_names(pattern);
            if let Some(variable) = variable {
                names.insert(variable.clone());
            }
            let inserted = insert_bound_many(bound, names.iter());
            collect_pattern_property_variables(pattern, bound, out);
            if let Some(predicate) = predicate {
                collect_free_variables(predicate, bound, out);
            }
            collect_free_variables(map, bound, out);
            remove_inserted(bound, inserted);
        }
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                collect_query_references(query, bound, out);
            }
            for part in &exists.patterns {
                collect_pattern_property_variables(part, bound, out);
            }
            if let Some(predicate) = &exists.predicate {
                collect_free_variables(predicate, bound, out);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                out.extend(pattern_binding_names(part).into_iter().filter(|name| !bound.contains(name)));
                collect_pattern_property_variables(part, bound, out);
            }
        }
        Expr::Star | Expr::Parameter(_) | Expr::Literal(_) | Expr::CountStar => {}
    }
}

pub(super) fn collect_pattern_property_variables(
    pattern: &PatternPart,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        collect_free_variables(properties, bound, out);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            collect_free_variables(properties, bound, out);
        }
        if let Some(properties) = &chain.node.properties {
            collect_free_variables(properties, bound, out);
        }
    }
}

pub(super) fn collect_query_references(
    query: &Query,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    let mut query_bound = bound.clone();
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    collect_pattern_property_variables(part, &mut query_bound, out);
                    query_bound.extend(pattern_binding_names(part));
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, &mut query_bound, out);
                }
            }
            Clause::Unwind(clause) => {
                collect_free_variables(&clause.expr, &mut query_bound, out);
                query_bound.insert(clause.alias.clone());
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    collect_free_variables(arg, &mut query_bound, out);
                }
                for item in &clause.yields {
                    query_bound.insert(item.alias.clone());
                }
                if clause.yield_all || clause.standalone {
                    query_bound.extend(default_procedure_yields(&clause.name));
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, &mut query_bound, out);
                }
            }
            Clause::Merge(clause) => {
                for properties in merge_pattern_properties(&clause.pattern) {
                    collect_free_variables(properties, &mut query_bound, out);
                }
                for variable in merge_pattern_variables(&clause.pattern) {
                    query_bound.insert(variable);
                }
                for item in clause.on_create.iter().chain(clause.on_match.iter()) {
                    for expr in merge_set_item_exprs(item) {
                        collect_free_variables(expr, &mut query_bound, out);
                    }
                }
            }
            Clause::Create(clause) => {
                for part in &clause.patterns {
                    if let Some(properties) = &part.element.start.properties {
                        collect_free_variables(properties, &mut query_bound, out);
                    }
                    if let Some(variable) = &part.element.start.variable {
                        query_bound.insert(variable.clone());
                    }
                    for chain in &part.element.chains {
                        if let Some(properties) = &chain.relationship.properties {
                            collect_free_variables(properties, &mut query_bound, out);
                        }
                        if let Some(variable) = &chain.relationship.variable {
                            query_bound.insert(variable.clone());
                        }
                        if let Some(properties) = &chain.node.properties {
                            collect_free_variables(properties, &mut query_bound, out);
                        }
                        if let Some(variable) = &chain.node.variable {
                            query_bound.insert(variable.clone());
                        }
                    }
                }
            }
            Clause::Set(clause) => {
                for item in &clause.items {
                    match item {
                        crate::language::cypher::ast::SetItem::Property {
                            target, value, ..
                        } => {
                            collect_free_variables(target, &mut query_bound, out);
                            collect_free_variables(value, &mut query_bound, out);
                        }
                        crate::language::cypher::ast::SetItem::Replace { variable, value }
                        | crate::language::cypher::ast::SetItem::Merge { variable, value } => {
                            if !query_bound.contains(variable) {
                                out.insert(variable.clone());
                            }
                            collect_free_variables(value, &mut query_bound, out);
                        }
                        crate::language::cypher::ast::SetItem::Labels { variable, .. } => {
                            if !query_bound.contains(variable) {
                                out.insert(variable.clone());
                            }
                        }
                    }
                }
            }
            Clause::Delete(clause) => {
                for expr in &clause.expressions {
                    collect_free_variables(expr, &mut query_bound, out);
                }
            }
            Clause::With(clause) => {
                collect_projection_references(&clause.projection, &mut query_bound, out);
                query_bound = projection_output_names(&clause.projection, &query_bound);
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, &mut query_bound, out);
                }
            }
            Clause::Return(clause) => {
                collect_projection_references(&clause.projection, &mut query_bound, out);
            }
        }
    }
}

pub(super) fn collect_projection_references(
    body: &ProjectionBody,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    for item in &body.items {
        collect_free_variables(&item.expr, bound, out);
    }
    let mut projection_bound = bound.clone();
    projection_bound.extend(projection_output_names(body, bound));
    for item in &body.order_by {
        collect_free_variables(&item.expr, &mut projection_bound, out);
    }
    if let Some(skip) = &body.skip {
        collect_free_variables(skip, &mut projection_bound, out);
    }
    if let Some(limit) = &body.limit {
        collect_free_variables(limit, &mut projection_bound, out);
    }
}

pub(super) fn projection_output_names(
    body: &ProjectionBody,
    visible: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut outputs = if body.include_existing {
        visible.clone()
    } else {
        BTreeSet::new()
    };
    for item in &body.items {
        if let Some(alias) = item
            .alias
            .clone()
            .or_else(|| item.expr.variable_name().map(ToString::to_string))
        {
            outputs.insert(alias);
        }
    }
    outputs
}

pub(super) fn remove_local_exists_bindings(expr: &Expr, refs: &mut BTreeSet<String>) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                remove_query_outputs(query, refs);
            }
            for part in &exists.patterns {
                for name in pattern_binding_names(part) {
                    refs.remove(&name);
                }
            }
        }
        Expr::PatternComprehension { pattern, .. } => {
            for name in pattern_binding_names(pattern) {
                refs.remove(&name);
            }
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            remove_local_exists_bindings(expr, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            remove_local_exists_bindings(lhs, refs);
            remove_local_exists_bindings(rhs, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                remove_local_exists_bindings(arg, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                remove_local_exists_bindings(value, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                remove_local_exists_bindings(case, refs);
            }
            for (when, then) in arms {
                remove_local_exists_bindings(when, refs);
                remove_local_exists_bindings(then, refs);
            }
            if let Some(otherwise) = otherwise {
                remove_local_exists_bindings(otherwise, refs);
            }
        }
        _ => {}
    }
}

pub(super) fn remove_query_outputs(query: &Query, refs: &mut BTreeSet<String>) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    for name in pattern_binding_names(part) {
                        refs.remove(&name);
                    }
                }
            }
            Clause::Unwind(clause) => {
                refs.remove(&clause.alias);
            }
            Clause::Call(clause) => {
                for item in &clause.yields {
                    refs.remove(&item.alias);
                }
                if clause.yield_all || clause.standalone {
                    for name in default_procedure_yields(&clause.name) {
                        refs.remove(&name);
                    }
                }
            }
            Clause::Merge(clause) => {
                for variable in merge_pattern_variables(&clause.pattern) {
                    refs.remove(&variable);
                }
            }
            Clause::Create(clause) => {
                for part in &clause.patterns {
                    if let Some(variable) = &part.element.start.variable {
                        refs.remove(variable);
                    }
                    for chain in &part.element.chains {
                        if let Some(variable) = &chain.relationship.variable {
                            refs.remove(variable);
                        }
                        if let Some(variable) = &chain.node.variable {
                            refs.remove(variable);
                        }
                    }
                }
            }
            Clause::Set(_) | Clause::Delete(_) => {}
            Clause::With(clause) => {
                for name in projection_output_names(&clause.projection, &BTreeSet::new()) {
                    refs.remove(&name);
                }
            }
            Clause::Return(clause) => {
                for name in projection_output_names(&clause.projection, &BTreeSet::new()) {
                    refs.remove(&name);
                }
            }
        }
    }
}

pub(super) fn with_bound<F>(bound: &mut BTreeSet<String>, name: &str, f: F)
where
    F: FnOnce(&mut BTreeSet<String>),
{
    let inserted = bound.insert(name.to_string());
    f(bound);
    if inserted {
        bound.remove(name);
    }
}

pub(super) fn with_bound_many<'a, I, F>(bound: &mut BTreeSet<String>, names: I, f: F)
where
    I: IntoIterator<Item = &'a String>,
    F: FnOnce(&mut BTreeSet<String>),
{
    let inserted = insert_bound_many(bound, names);
    f(bound);
    remove_inserted(bound, inserted);
}

pub(super) fn insert_bound_many<'a, I>(bound: &mut BTreeSet<String>, names: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a String>,
{
    names
        .into_iter()
        .filter_map(|name| bound.insert(name.clone()).then(|| name.clone()))
        .collect()
}

pub(super) fn remove_inserted(bound: &mut BTreeSet<String>, inserted: Vec<String>) {
    for name in inserted {
        bound.remove(&name);
    }
}
