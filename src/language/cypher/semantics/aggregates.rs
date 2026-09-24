//! Aggregate detection across expressions and query clauses.

use super::{Clause, Expr, ProjectionBody, Query, merge_pattern_properties, merge_set_item_exprs};
pub(super) fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::CountStar => true,
        Expr::Function { name, args, .. } => {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "count" | "sum" | "avg" | "min" | "max" | "collect"
            ) || args.iter().any(contains_aggregate)
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
