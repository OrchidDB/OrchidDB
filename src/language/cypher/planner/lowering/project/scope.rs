//! Expression reference validation and nested scope traversal.

use super::{
    BTreeSet, Clause, CypherPlanError, CypherPlanResult, Expr, Lowerer, PatternPart,
    ProjectionBody, Query,
};
use crate::language::cypher::planner::CypherSemanticError;
pub(super) fn free_variable_names(expr: &Expr) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_free_variables(expr, &mut BTreeSet::new(), &mut refs);
    refs
}

pub(super) fn free_variable_names_for_sort(
    expr: &Expr,
    source_fields: &[String],
    projected_fields: &[String],
) -> BTreeSet<String> {
    let mut refs = free_variable_names(expr);
    let candidates = source_fields
        .iter()
        .chain(projected_fields.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    remove_local_exists_bindings(expr, &candidates, &mut refs);
    add_candidate_pattern_bindings(expr, &candidates, &mut refs);
    refs
}

pub(super) fn free_variable_names_for_local_scope(
    expr: &Expr,
    locals: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, locals, &mut refs);
    add_candidate_pattern_bindings(expr, locals, &mut refs);
    refs
}

pub(crate) fn validate_expression_scope(
    lowerer: &Lowerer,
    expr: &Expr,
    clause: &str,
) -> CypherPlanResult<()> {
    let candidates = lowerer
        .visible_fields()
        .into_iter()
        .collect::<BTreeSet<_>>();
    validate_expression_refs(expr, &candidates, clause)
}

pub(crate) fn expression_candidate_refs(
    expr: &Expr,
    candidates: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut refs);
    add_candidate_pattern_bindings(expr, candidates, &mut refs);
    refs.into_iter()
        .filter(|name| candidates.contains(name))
        .collect()
}

pub(crate) fn validate_expression_refs(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    clause: &str,
) -> CypherPlanResult<()> {
    let mut refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut refs);
    add_candidate_pattern_bindings(expr, candidates, &mut refs);
    let missing = refs
        .into_iter()
        .filter(|name| !candidates.contains(name))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "{clause} references variables that are not in scope: {}",
            missing.join(", ")
        ))
        .classified(CypherSemanticError::UndefinedVariable))
    }
}

pub(super) fn add_candidate_pattern_bindings(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                add_candidate_bindings_from_query(query, candidates, refs);
            }
            for part in &exists.patterns {
                add_candidate_bindings_from_pattern(part, candidates, refs);
            }
            if let Some(predicate) = &exists.predicate {
                add_candidate_pattern_bindings(predicate, candidates, refs);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                add_candidate_bindings_from_pattern(part, candidates, refs);
            }
        }
        Expr::PatternComprehension {
            pattern,
            predicate,
            map,
            ..
        } => {
            add_candidate_bindings_from_pattern(pattern, candidates, refs);
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings(predicate, candidates, refs);
            }
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            add_candidate_pattern_bindings(expr, candidates, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            add_candidate_pattern_bindings(lhs, candidates, refs);
            add_candidate_pattern_bindings(rhs, candidates, refs);
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            add_candidate_pattern_bindings(target, candidates, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                add_candidate_pattern_bindings(arg, candidates, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                add_candidate_pattern_bindings(value, candidates, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                add_candidate_pattern_bindings(case, candidates, refs);
            }
            for (when, then) in arms {
                add_candidate_pattern_bindings(when, candidates, refs);
                add_candidate_pattern_bindings(then, candidates, refs);
            }
            if let Some(otherwise) = otherwise {
                add_candidate_pattern_bindings(otherwise, candidates, refs);
            }
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings(predicate, candidates, refs);
            }
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::ListReduce {
            collection, map, ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::ListTransform {
            collection, map, ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::ListFilter {
            collection,
            predicate,
            ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(predicate, candidates, refs);
        }
        Expr::Quantifier {
            collection,
            predicate,
            ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(predicate, candidates, refs);
        }
        Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => {}
    }
}

pub(super) fn add_candidate_bindings_from_pattern(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for name in pattern_binding_names(pattern) {
        if candidates.contains(&name) {
            refs.insert(name);
        }
    }
    add_candidate_refs_from_pattern_properties(pattern, candidates, refs);
}

pub(super) fn add_candidate_refs_from_expr(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    let mut expr_refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut expr_refs);
    add_candidate_pattern_bindings(expr, candidates, &mut expr_refs);
    refs.extend(
        expr_refs
            .into_iter()
            .filter(|name| candidates.contains(name)),
    );
}

pub(super) fn add_candidate_refs_from_pattern_properties(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        add_candidate_refs_from_expr(properties, candidates, refs);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            add_candidate_refs_from_expr(properties, candidates, refs);
        }
        if let Some(properties) = &chain.node.properties {
            add_candidate_refs_from_expr(properties, candidates, refs);
        }
    }
}

pub(super) fn add_candidate_bindings_from_query(
    query: &Query,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    add_candidate_bindings_from_query_scoped(query, candidates, &BTreeSet::new(), refs);
}

pub(super) fn add_candidate_bindings_from_query_scoped(
    query: &Query,
    candidates: &BTreeSet<String>,
    outer_locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    let mut locals = outer_locals.clone();
    add_candidate_bindings_from_query_body(query, candidates, &mut locals, refs);
    for branch in &query.unions {
        let mut branch_locals = outer_locals.clone();
        add_candidate_bindings_from_query_body(&branch.query, candidates, &mut branch_locals, refs);
    }
}

pub(super) fn add_candidate_bindings_from_query_body(
    query: &Query,
    candidates: &BTreeSet<String>,
    locals: &mut BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    let names = pattern_binding_names(part);
                    add_candidate_bindings_from_pattern_scoped(part, candidates, locals, refs);
                    for name in names {
                        if !candidates.contains(&name) || locals.contains(&name) {
                            locals.insert(name);
                        }
                    }
                }
                if let Some(predicate) = &clause.predicate {
                    add_candidate_refs_from_expr_scoped(predicate, candidates, locals, refs);
                }
            }
            Clause::Unwind(clause) => {
                add_candidate_refs_from_expr_scoped(&clause.expr, candidates, locals, refs);
                locals.insert(clause.alias.clone());
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    add_candidate_refs_from_expr_scoped(arg, candidates, locals, refs);
                }
                for item in &clause.yields {
                    locals.insert(item.alias.clone());
                }
                if clause.yield_all || clause.standalone {
                    locals.extend(default_query_procedure_yields(&clause.name));
                }
                if let Some(predicate) = &clause.predicate {
                    add_candidate_refs_from_expr_scoped(predicate, candidates, locals, refs);
                }
            }
            // MERGE binds the same variables as CREATE over one pattern,
            // then applies its ON CREATE / ON MATCH sets.
            Clause::Merge(clause) => {
                add_candidate_bindings_from_pattern_scoped(
                    &clause.pattern,
                    candidates,
                    locals,
                    refs,
                );
                for name in pattern_binding_names(&clause.pattern) {
                    locals.insert(name);
                }
                for item in clause.on_create.iter().chain(clause.on_match.iter()) {
                    for expr in set_item_exprs(item) {
                        add_candidate_refs_from_expr_scoped(expr, candidates, locals, refs);
                    }
                }
            }
            Clause::Create(clause) => {
                for part in &clause.patterns {
                    if let Some(properties) = &part.element.start.properties {
                        add_candidate_refs_from_expr_scoped(properties, candidates, locals, refs);
                    }
                    if let Some(variable) = &part.element.start.variable {
                        locals.insert(variable.clone());
                    }
                    for chain in &part.element.chains {
                        if let Some(properties) = &chain.relationship.properties {
                            add_candidate_refs_from_expr_scoped(
                                properties, candidates, locals, refs,
                            );
                        }
                        if let Some(variable) = &chain.relationship.variable {
                            locals.insert(variable.clone());
                        }
                        if let Some(properties) = &chain.node.properties {
                            add_candidate_refs_from_expr_scoped(
                                properties, candidates, locals, refs,
                            );
                        }
                        if let Some(variable) = &chain.node.variable {
                            locals.insert(variable.clone());
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
                            add_candidate_refs_from_expr_scoped(target, candidates, locals, refs);
                            add_candidate_refs_from_expr_scoped(value, candidates, locals, refs);
                        }
                        crate::language::cypher::ast::SetItem::Replace { value, .. }
                        | crate::language::cypher::ast::SetItem::Merge { value, .. } => {
                            add_candidate_refs_from_expr_scoped(value, candidates, locals, refs);
                        }
                        crate::language::cypher::ast::SetItem::Labels { .. } => {}
                    }
                }
            }
            Clause::Delete(clause) => {
                for expr in &clause.expressions {
                    add_candidate_refs_from_expr_scoped(expr, candidates, locals, refs);
                }
            }
            Clause::With(clause) => {
                let outputs = add_candidate_bindings_from_projection_scoped(
                    &clause.projection,
                    candidates,
                    locals,
                    refs,
                );
                *locals = outputs;
                if let Some(predicate) = &clause.predicate {
                    add_candidate_refs_from_expr_scoped(predicate, candidates, locals, refs);
                }
            }
            Clause::Return(clause) => {
                add_candidate_bindings_from_projection_scoped(
                    &clause.projection,
                    candidates,
                    locals,
                    refs,
                );
            }
        }
    }
}

pub(super) fn add_candidate_bindings_from_projection_scoped(
    body: &ProjectionBody,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) -> BTreeSet<String> {
    for item in &body.items {
        add_candidate_refs_from_expr_scoped(&item.expr, candidates, locals, refs);
    }
    let outputs = projection_output_names(body, locals);
    let mut order_locals = locals.clone();
    order_locals.extend(outputs.iter().cloned());
    for item in &body.order_by {
        add_candidate_refs_from_expr_scoped(&item.expr, candidates, &order_locals, refs);
    }
    if let Some(skip) = &body.skip {
        add_candidate_refs_from_expr_scoped(skip, candidates, &order_locals, refs);
    }
    if let Some(limit) = &body.limit {
        add_candidate_refs_from_expr_scoped(limit, candidates, &order_locals, refs);
    }
    outputs
}

pub(super) fn add_candidate_bindings_from_pattern_scoped(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for name in pattern_binding_names(pattern) {
        if candidates.contains(&name) && !locals.contains(&name) {
            refs.insert(name);
        }
    }
    add_candidate_refs_from_pattern_properties_scoped(pattern, candidates, locals, refs);
}

pub(super) fn add_candidate_refs_from_expr_scoped(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    let mut expr_refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut expr_refs);
    refs.extend(
        expr_refs
            .into_iter()
            .filter(|name| candidates.contains(name) && !locals.contains(name)),
    );
    add_candidate_pattern_bindings_scoped(expr, candidates, locals, refs);
}

pub(super) fn add_candidate_pattern_bindings_scoped(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                add_candidate_bindings_from_query_scoped(query, candidates, locals, refs);
            }
            for part in &exists.patterns {
                add_candidate_bindings_from_pattern_scoped(part, candidates, locals, refs);
            }
            if let Some(predicate) = &exists.predicate {
                add_candidate_pattern_bindings_scoped(predicate, candidates, locals, refs);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                add_candidate_bindings_from_pattern_scoped(part, candidates, locals, refs);
            }
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let mut pattern_locals = locals.clone();
            if let Some(variable) = variable {
                pattern_locals.insert(variable.clone());
            }
            let names = pattern_binding_names(pattern);
            add_candidate_bindings_from_pattern_scoped(pattern, candidates, &pattern_locals, refs);
            pattern_locals.extend(names);
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings_scoped(predicate, candidates, &pattern_locals, refs);
            }
            add_candidate_pattern_bindings_scoped(map, candidates, &pattern_locals, refs);
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings_scoped(predicate, candidates, &item_locals, refs);
            }
            add_candidate_pattern_bindings_scoped(map, candidates, &item_locals, refs);
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(accumulator.clone());
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(map, candidates, &item_locals, refs);
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(map, candidates, &item_locals, refs);
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(predicate, candidates, &item_locals, refs);
        }
        Expr::Quantifier {
            variable,
            collection,
            predicate,
            ..
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(predicate, candidates, &item_locals, refs);
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            add_candidate_pattern_bindings_scoped(expr, candidates, locals, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            add_candidate_pattern_bindings_scoped(lhs, candidates, locals, refs);
            add_candidate_pattern_bindings_scoped(rhs, candidates, locals, refs);
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            add_candidate_pattern_bindings_scoped(target, candidates, locals, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                add_candidate_pattern_bindings_scoped(arg, candidates, locals, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                add_candidate_pattern_bindings_scoped(value, candidates, locals, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                add_candidate_pattern_bindings_scoped(case, candidates, locals, refs);
            }
            for (when, then) in arms {
                add_candidate_pattern_bindings_scoped(when, candidates, locals, refs);
                add_candidate_pattern_bindings_scoped(then, candidates, locals, refs);
            }
            if let Some(otherwise) = otherwise {
                add_candidate_pattern_bindings_scoped(otherwise, candidates, locals, refs);
            }
        }
        Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => {}
    }
}

pub(super) fn add_candidate_refs_from_pattern_properties_scoped(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        add_candidate_refs_from_expr_scoped(properties, candidates, locals, refs);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            add_candidate_refs_from_expr_scoped(properties, candidates, locals, refs);
        }
        if let Some(properties) = &chain.node.properties {
            add_candidate_refs_from_expr_scoped(properties, candidates, locals, refs);
        }
    }
}

pub(super) fn remove_local_exists_bindings(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                remove_local_query_bindings(query, candidates, refs);
            }
            for part in &exists.patterns {
                for name in pattern_binding_names(part) {
                    if !candidates.contains(&name) {
                        refs.remove(&name);
                    }
                }
            }
            if let Some(predicate) = &exists.predicate {
                remove_local_exists_bindings(predicate, candidates, refs);
            }
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            remove_local_exists_bindings(expr, candidates, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            remove_local_exists_bindings(lhs, candidates, refs);
            remove_local_exists_bindings(rhs, candidates, refs);
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            remove_local_exists_bindings(target, candidates, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                remove_local_exists_bindings(arg, candidates, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                remove_local_exists_bindings(value, candidates, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                remove_local_exists_bindings(case, candidates, refs);
            }
            for (when, then) in arms {
                remove_local_exists_bindings(when, candidates, refs);
                remove_local_exists_bindings(then, candidates, refs);
            }
            if let Some(otherwise) = otherwise {
                remove_local_exists_bindings(otherwise, candidates, refs);
            }
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            if let Some(predicate) = predicate {
                remove_local_exists_bindings(predicate, candidates, refs);
            }
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::ListReduce {
            collection, map, ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::ListTransform {
            collection, map, ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::ListFilter {
            collection,
            predicate,
            ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(predicate, candidates, refs);
        }
        Expr::PatternComprehension { predicate, map, .. } => {
            if let Some(predicate) = predicate {
                remove_local_exists_bindings(predicate, candidates, refs);
            }
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::Quantifier {
            collection,
            predicate,
            ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(predicate, candidates, refs);
        }
        Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => {}
    }
}

pub(super) fn remove_local_query_bindings(
    query: &Query,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    remove_local_query_body_bindings(query, candidates, refs);
    for branch in &query.unions {
        remove_local_query_body_bindings(&branch.query, candidates, refs);
    }
}

pub(super) fn remove_local_query_body_bindings(
    query: &Query,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    remove_local_pattern_bindings(part, candidates, refs);
                    collect_local_pattern_expression_bindings(part, candidates, refs);
                }
                if let Some(predicate) = &clause.predicate {
                    remove_local_exists_bindings(predicate, candidates, refs);
                }
            }
            Clause::Unwind(clause) => {
                remove_local_exists_bindings(&clause.expr, candidates, refs);
                remove_query_local_name(&clause.alias, candidates, refs);
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    remove_local_exists_bindings(arg, candidates, refs);
                }
                for item in &clause.yields {
                    remove_query_local_name(&item.alias, candidates, refs);
                }
                if clause.yield_all || clause.standalone {
                    for name in default_query_procedure_yields(&clause.name) {
                        remove_query_local_name(&name, candidates, refs);
                    }
                }
                if let Some(predicate) = &clause.predicate {
                    remove_local_exists_bindings(predicate, candidates, refs);
                }
            }
            Clause::Merge(clause) => {
                for name in pattern_binding_names(&clause.pattern) {
                    remove_query_local_name(&name, candidates, refs);
                }
                for item in clause.on_create.iter().chain(clause.on_match.iter()) {
                    for expr in set_item_exprs(item) {
                        remove_local_exists_bindings(expr, candidates, refs);
                    }
                }
            }
            Clause::Create(clause) => {
                for part in &clause.patterns {
                    if let Some(properties) = &part.element.start.properties {
                        remove_local_exists_bindings(properties, candidates, refs);
                    }
                    if let Some(variable) = &part.element.start.variable {
                        remove_query_local_name(variable, candidates, refs);
                    }
                    for chain in &part.element.chains {
                        if let Some(properties) = &chain.relationship.properties {
                            remove_local_exists_bindings(properties, candidates, refs);
                        }
                        if let Some(variable) = &chain.relationship.variable {
                            remove_query_local_name(variable, candidates, refs);
                        }
                        if let Some(properties) = &chain.node.properties {
                            remove_local_exists_bindings(properties, candidates, refs);
                        }
                        if let Some(variable) = &chain.node.variable {
                            remove_query_local_name(variable, candidates, refs);
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
                            remove_local_exists_bindings(target, candidates, refs);
                            remove_local_exists_bindings(value, candidates, refs);
                        }
                        crate::language::cypher::ast::SetItem::Replace { value, .. }
                        | crate::language::cypher::ast::SetItem::Merge { value, .. } => {
                            remove_local_exists_bindings(value, candidates, refs);
                        }
                        crate::language::cypher::ast::SetItem::Labels { .. } => {}
                    }
                }
            }
            Clause::Delete(clause) => {
                for expr in &clause.expressions {
                    remove_local_exists_bindings(expr, candidates, refs);
                }
            }
            Clause::With(clause) => {
                remove_local_projection_bindings(&clause.projection, candidates, refs);
                if let Some(predicate) = &clause.predicate {
                    remove_local_exists_bindings(predicate, candidates, refs);
                }
            }
            Clause::Return(clause) => {
                remove_local_projection_bindings(&clause.projection, candidates, refs);
            }
        }
    }
}

pub(super) fn remove_local_projection_bindings(
    body: &ProjectionBody,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for item in &body.items {
        remove_local_exists_bindings(&item.expr, candidates, refs);
        if let Some(alias) = item.alias.as_deref().or_else(|| item.expr.variable_name()) {
            remove_query_local_name(alias, candidates, refs);
        }
    }
    for item in &body.order_by {
        remove_local_exists_bindings(&item.expr, candidates, refs);
    }
    if let Some(skip) = &body.skip {
        remove_local_exists_bindings(skip, candidates, refs);
    }
    if let Some(limit) = &body.limit {
        remove_local_exists_bindings(limit, candidates, refs);
    }
}

pub(super) fn collect_local_pattern_expression_bindings(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        remove_local_exists_bindings(properties, candidates, refs);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            remove_local_exists_bindings(properties, candidates, refs);
        }
        if let Some(properties) = &chain.node.properties {
            remove_local_exists_bindings(properties, candidates, refs);
        }
    }
}

pub(super) fn remove_local_pattern_bindings(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for name in pattern_binding_names(pattern) {
        remove_query_local_name(&name, candidates, refs);
    }
}

pub(super) fn remove_query_local_name(
    name: &str,
    _candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    refs.remove(name);
}

pub(crate) fn collect_free_variables(
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
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            if let Some(predicate) = predicate {
                collect_free_variables(predicate, bound, out);
            }
            collect_free_variables(map, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            let acc_was_bound = bound.contains(accumulator);
            let variable_was_bound = bound.contains(variable);
            bound.insert(accumulator.clone());
            bound.insert(variable.clone());
            collect_free_variables(map, bound, out);
            if !acc_was_bound {
                bound.remove(accumulator);
            }
            if !variable_was_bound {
                bound.remove(variable);
            }
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            collect_free_variables(map, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            collect_free_variables(collection, bound, out);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            collect_free_variables(predicate, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::Quantifier {
            variable,
            collection,
            predicate,
            ..
        } => {
            collect_free_variables(collection, bound, out);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            collect_free_variables(predicate, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let variable_was_bound = variable.as_ref().is_some_and(|name| bound.contains(name));
            if let Some(variable) = variable {
                bound.insert(variable.clone());
            }
            let pattern_bound = pattern_binding_names(pattern);
            let previously_bound = pattern_bound
                .iter()
                .filter(|name| bound.contains(*name))
                .cloned()
                .collect::<BTreeSet<_>>();
            for name in &pattern_bound {
                bound.insert(name.clone());
            }
            collect_pattern_property_variables(pattern, bound, out);
            if let Some(predicate) = predicate {
                collect_free_variables(predicate, bound, out);
            }
            collect_free_variables(map, bound, out);
            for name in &pattern_bound {
                if !previously_bound.contains(name) {
                    bound.remove(name);
                }
            }
            if let Some(variable) = variable {
                if !variable_was_bound {
                    bound.remove(variable);
                }
            }
        }
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                collect_query_variable_references(query, bound, out);
            }
            for part in &exists.patterns {
                collect_pattern_variable_references(part, out);
                collect_pattern_property_variables(part, bound, out);
            }
            if let Some(predicate) = &exists.predicate {
                collect_free_variables(predicate, bound, out);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                collect_pattern_variable_references(part, out);
                collect_pattern_property_variables(part, bound, out);
            }
        }
        Expr::Star | Expr::Parameter(_) | Expr::Literal(_) | Expr::CountStar => {}
    }
}

pub(super) fn pattern_binding_names(pattern: &PatternPart) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(variable) = &pattern.variable {
        names.insert(variable.clone());
    }
    if let Some(variable) = &pattern.element.start.variable {
        names.insert(variable.clone());
    }
    for chain in &pattern.element.chains {
        if let Some(variable) = &chain.relationship.variable {
            names.insert(variable.clone());
        }
        if let Some(variable) = &chain.node.variable {
            names.insert(variable.clone());
        }
    }
    names
}

pub(super) fn collect_pattern_variable_references(
    pattern: &PatternPart,
    out: &mut BTreeSet<String>,
) {
    out.extend(pattern_binding_names(pattern));
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

pub(super) fn collect_query_variable_references(
    query: &Query,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    let mut query_bound = bound.clone();
    collect_query_body_variable_references(query, &mut query_bound, out);
    for branch in &query.unions {
        let mut branch_bound = bound.clone();
        collect_query_body_variable_references(&branch.query, &mut branch_bound, out);
    }
}

pub(super) fn collect_query_body_variable_references(
    query: &Query,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    collect_pattern_variable_references(part, out);
                    collect_pattern_property_variables(part, bound, out);
                    bound.extend(pattern_binding_names(part));
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, bound, out);
                }
            }
            Clause::Unwind(clause) => {
                collect_free_variables(&clause.expr, bound, out);
                bound.insert(clause.alias.clone());
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    collect_free_variables(arg, bound, out);
                }
                for item in &clause.yields {
                    bound.insert(item.alias.clone());
                }
                if clause.yield_all || clause.standalone {
                    for item in default_query_procedure_yields(&clause.name) {
                        bound.insert(item);
                    }
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, bound, out);
                }
            }
            Clause::Merge(clause) => {
                for name in pattern_binding_names(&clause.pattern) {
                    bound.insert(name);
                }
                for item in clause.on_create.iter().chain(clause.on_match.iter()) {
                    for expr in set_item_exprs(item) {
                        collect_free_variables(expr, bound, out);
                    }
                }
            }
            Clause::Create(clause) => {
                for part in &clause.patterns {
                    if let Some(properties) = &part.element.start.properties {
                        collect_free_variables(properties, bound, out);
                    }
                    if let Some(variable) = &part.element.start.variable {
                        bound.insert(variable.clone());
                    }
                    for chain in &part.element.chains {
                        if let Some(properties) = &chain.relationship.properties {
                            collect_free_variables(properties, bound, out);
                        }
                        if let Some(variable) = &chain.relationship.variable {
                            bound.insert(variable.clone());
                        }
                        if let Some(properties) = &chain.node.properties {
                            collect_free_variables(properties, bound, out);
                        }
                        if let Some(variable) = &chain.node.variable {
                            bound.insert(variable.clone());
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
                            collect_free_variables(target, bound, out);
                            collect_free_variables(value, bound, out);
                        }
                        crate::language::cypher::ast::SetItem::Replace { value, .. }
                        | crate::language::cypher::ast::SetItem::Merge { value, .. } => {
                            collect_free_variables(value, bound, out);
                        }
                        crate::language::cypher::ast::SetItem::Labels { .. } => {}
                    }
                }
            }
            Clause::Delete(clause) => {
                for expr in &clause.expressions {
                    collect_free_variables(expr, bound, out);
                }
            }
            Clause::With(clause) => {
                collect_projection_variable_references(&clause.projection, bound, out);
                let outputs = projection_output_names(&clause.projection, bound);
                bound.clear();
                bound.extend(outputs);
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, bound, out);
                }
            }
            Clause::Return(clause) => {
                collect_projection_variable_references(&clause.projection, bound, out);
            }
        }
    }
}

pub(super) fn collect_projection_variable_references(
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

pub(super) fn default_query_procedure_yields(name: &str) -> Vec<String> {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" => vec!["label".to_string()],
        "db.relationshiptypes" => vec!["relationshipType".to_string()],
        "db.propertykeys" => vec!["propertyKey".to_string()],
        _ => vec!["value".to_string()],
    }
}

/// Every expression a `SET` item evaluates, for scope walkers.
pub(super) fn set_item_exprs(item: &crate::language::cypher::ast::SetItem) -> Vec<&Expr> {
    use crate::language::cypher::ast::SetItem;
    match item {
        SetItem::Property { target, value, .. } => vec![target, value],
        SetItem::Replace { value, .. } | SetItem::Merge { value, .. } => vec![value],
        SetItem::Labels { .. } => Vec::new(),
    }
}
