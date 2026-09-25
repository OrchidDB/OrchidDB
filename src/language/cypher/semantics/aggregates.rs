//! Aggregate detection across expressions and query clauses.

use super::{CypherPlanError, CypherPlanResult, CypherSemanticError, Clause, Expr, ProjectionBody, Query, merge_pattern_properties, merge_set_item_exprs};

/// Within an aggregate expression, row-dependent leaves must be explicit
/// grouping keys. A projected composite expression does not implicitly group
/// each of its operands.
pub(super) fn validate_grouping(body: &ProjectionBody) -> CypherPlanResult<()> {
    let keys = body.items.iter().filter(|item| !contains_aggregate(&item.expr))
        .filter_map(|item| matches!(item.expr, Expr::Variable(_) | Expr::Property { .. }).then_some(&item.expr))
        .collect::<Vec<_>>();
    let aliases = body.items.iter().filter_map(|item| item.alias.as_ref()).collect::<Vec<_>>();
    fn visit(expr: &Expr, keys: &[&Expr], aliases: &[&String]) -> bool {
        match expr {
            Expr::CountStar => true,
            Expr::Function { name, args, distinct } => {
                let root = Expr::Function { name: name.clone(), args: vec![], distinct: *distinct };
                contains_aggregate(&root) || args.iter().all(|arg| visit(arg, keys, aliases))
            }
            Expr::Variable(name) => keys.contains(&expr) || aliases.contains(&name),
            Expr::Property { .. } => keys.contains(&expr),
            Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => visit(expr, keys, aliases),
            Expr::Binary { lhs, rhs, .. } | Expr::StringPredicate { target: lhs, pattern: rhs, .. } =>
                visit(lhs, keys, aliases) && visit(rhs, keys, aliases),
            Expr::List(items) => items.iter().all(|item| visit(item, keys, aliases)),
            Expr::Map(items) => items.iter().all(|(_,item)| visit(item, keys, aliases)),
            Expr::Case { case, arms, otherwise } => case.as_deref().is_none_or(|expr| visit(expr, keys, aliases))
                && arms.iter().all(|(a,b)| visit(a, keys, aliases) && visit(b, keys, aliases))
                && otherwise.as_deref().is_none_or(|expr| visit(expr, keys, aliases)),
            _ => true,
        }
    }
    for item in &body.items {
        if contains_aggregate(&item.expr) && !visit(&item.expr, &keys, &[]) {
            return Err(ambiguous_grouping());
        }
    }
    for item in &body.order_by {
        if contains_aggregate(&item.expr) && !visit(&item.expr, &keys, &aliases) {
            return Err(ambiguous_grouping());
        }
    }
    Ok(())
}

fn ambiguous_grouping() -> CypherPlanError {
    CypherPlanError::Invalid("Aggregate expression contains an implicit grouping key".into())
        .classified(CypherSemanticError::AmbiguousAggregationExpression)
}

pub(super) fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::CountStar => true,
        Expr::Function { name, args, .. } => {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "count"
                    | "count_if"
                    | "sum"
                    | "avg"
                    | "min"
                    | "max"
                    | "collect"
                    | "stdev"
                    | "stdevp"
                    | "percentilecont"
                    | "percentiledisc"
            ) || (!matches!(name.to_ascii_lowercase().as_str(),"last"|"head"|"tail")
                && crate::ir::functions::is_native_aggregate(name))
                || args.iter().any(contains_aggregate)
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            contains_aggregate(expr)
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => contains_aggregate(lhs) || contains_aggregate(rhs),
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            contains_aggregate(target)
        }
        Expr::List(items) => items.iter().any(contains_aggregate),
        Expr::Map(items) => items.iter().any(|(_, value)| contains_aggregate(value)),
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            case.as_deref().is_some_and(contains_aggregate)
                || arms
                    .iter()
                    .any(|(when, then)| contains_aggregate(when) || contains_aggregate(then))
                || otherwise.as_deref().is_some_and(contains_aggregate)
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            contains_aggregate(collection)
                || predicate.as_deref().is_some_and(contains_aggregate)
                || contains_aggregate(map)
        }
        Expr::ListReduce {
            collection, map, ..
        }
        | Expr::ListTransform {
            collection, map, ..
        } => contains_aggregate(collection) || contains_aggregate(map),
        Expr::ListFilter {
            collection,
            predicate,
            ..
        }
        | Expr::Quantifier {
            collection,
            predicate,
            ..
        } => contains_aggregate(collection) || contains_aggregate(predicate),
        Expr::PatternComprehension { predicate, map, .. } => {
            predicate.as_deref().is_some_and(contains_aggregate) || contains_aggregate(map)
        }
        Expr::Exists(exists) => {
            exists.predicate.as_deref().is_some_and(contains_aggregate)
                || exists
                    .query
                    .as_deref()
                    .is_some_and(query_contains_aggregate)
        }
        Expr::PatternPredicate(_) => false,
        Expr::Star | Expr::Variable(_) | Expr::Parameter(_) | Expr::Literal(_) => false,
    }
}

pub(super) fn query_contains_aggregate(query: &Query) -> bool {
    query.clauses.iter().any(|clause| match clause {
        Clause::With(clause) => projection_contains_aggregate(&clause.projection),
        Clause::Return(clause) => projection_contains_aggregate(&clause.projection),
        Clause::Match(clause) => clause.predicate.as_ref().is_some_and(contains_aggregate),
        Clause::Unwind(clause) => contains_aggregate(&clause.expr),
        Clause::Call(clause) => {
            clause.args.iter().any(contains_aggregate)
                || clause.predicate.as_ref().is_some_and(contains_aggregate)
        }
        Clause::Merge(clause) => {
            merge_pattern_properties(&clause.pattern)
                .into_iter()
                .any(contains_aggregate)
                || clause
                    .on_create
                    .iter()
                    .chain(clause.on_match.iter())
                    .flat_map(merge_set_item_exprs)
                    .any(contains_aggregate)
        }
        Clause::Create(clause) => clause.patterns.iter().any(|part| {
            part.element
                .start
                .properties
                .as_ref()
                .is_some_and(contains_aggregate)
                || part.element.chains.iter().any(|chain| {
                    chain
                        .relationship
                        .properties
                        .as_ref()
                        .is_some_and(contains_aggregate)
                        || chain
                            .node
                            .properties
                            .as_ref()
                            .is_some_and(contains_aggregate)
                })
        }),
        Clause::Set(clause) => clause.items.iter().any(|item| match item {
            crate::language::cypher::ast::SetItem::Property { target, value, .. } => {
                contains_aggregate(target) || contains_aggregate(value)
            }
            crate::language::cypher::ast::SetItem::Replace { value, .. }
            | crate::language::cypher::ast::SetItem::Merge { value, .. } => {
                contains_aggregate(value)
            }
            crate::language::cypher::ast::SetItem::Labels { .. } => false,
        }),
        Clause::Delete(clause) => clause.expressions.iter().any(contains_aggregate),
    })
}

pub(super) fn projection_contains_aggregate(body: &ProjectionBody) -> bool {
    body.items.iter().any(|item| contains_aggregate(&item.expr))
        || body
            .order_by
            .iter()
            .any(|item| contains_aggregate(&item.expr))
}

/// An aggregate consumes query rows, not values in one row's list iteration.
/// Aggregates in the collection expression itself (e.g. collect(n)) remain valid.
pub(super) fn validate_iteration_aggregate(expr: &Expr) -> CypherPlanResult<()> {
    if contains_aggregate(expr) {
        return Err(CypherPlanError::Invalid("Aggregation is not allowed inside a list iteration body".into())
            .classified(CypherSemanticError::InvalidAggregation));
    }
    Ok(())
}
