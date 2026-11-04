//! Aggregate detection across expressions and query clauses.

use super::{CypherPlanError, CypherPlanResult, CypherSemanticError, Expr, ProjectionBody};

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
            Expr::Property { target, .. } => keys.contains(&expr)
                || (contains_aggregate(target) && visit(target, keys, aliases)),
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
            if !body.items.iter().any(|item| !contains_aggregate(&item.expr)
                && !matches!(item.expr, Expr::Variable(_) | Expr::Property { .. } | Expr::Literal(_))) {
                return Err(CypherPlanError::Invalid("ORDER BY references a value outside the aggregate projection".into())
                    .classified(CypherSemanticError::UndefinedVariable));
            }
            return Err(ambiguous_grouping());
        }
    }
    Ok(())
}

fn ambiguous_grouping() -> CypherPlanError {
    CypherPlanError::Invalid("Aggregate expression contains an implicit grouping key".into())
        .classified(CypherSemanticError::AmbiguousAggregationExpression)
}

pub(crate) fn contains_volatile(expr: &Expr) -> bool {
    match expr {
        Expr::Function { name, args, .. } => matches!(name.to_ascii_lowercase().as_str(), "rand" | "random" | "randomuuid")
            || crate::ir::functions::is_volatile_function(name) || args.iter().any(contains_volatile),
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr)
            | Expr::Property { target: expr, .. } => contains_volatile(expr),
        Expr::Binary { lhs, rhs, .. } | Expr::StringPredicate { target: lhs, pattern: rhs, .. } =>
            contains_volatile(lhs) || contains_volatile(rhs),
        Expr::List(items) => items.iter().any(contains_volatile),
        Expr::Map(items) => items.iter().any(|(_, value)| contains_volatile(value)),
        Expr::Case { case, arms, otherwise } => case.as_deref().is_some_and(contains_volatile)
            || arms.iter().any(|(a,b)| contains_volatile(a) || contains_volatile(b))
            || otherwise.as_deref().is_some_and(contains_volatile),
        _ => false,
    }
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
        // The subquery validates its own aggregation scope. Its scalar
        // existence result is not an aggregate over the enclosing query rows.
        Expr::Exists(_) => false,
        Expr::PatternPredicate(_) => false,
        Expr::Star | Expr::Variable(_) | Expr::Parameter(_) | Expr::Literal(_) => false,
    }
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
