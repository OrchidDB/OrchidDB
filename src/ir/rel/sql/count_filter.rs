//! Express conditional row counts with SQL FILTER instead of nullable CASE.
use super::*;
use datafusion::logical_expr::expr_rewriter::NamePreserver;

fn rewrite_expr(expr: Expr) -> datafusion::common::Result<Transformed<Expr>> {
    let Expr::AggregateFunction(mut aggregate) = expr else {
        return Ok(Transformed::no(expr));
    };
    // Match the actual builtin, never a similarly named user aggregate.
    if !aggregate.func.inner().as_any().is::<datafusion::functions_aggregate::count::Count>()
        || aggregate.params.distinct || aggregate.params.filter.is_some()
        || !aggregate.params.order_by.is_empty() || aggregate.params.null_treatment.is_some()
    { return Ok(Transformed::no(Expr::AggregateFunction(aggregate))); }
    let [Expr::Case(case)] = aggregate.params.args.as_slice() else {
        return Ok(Transformed::no(Expr::AggregateFunction(aggregate)));
    };
    let [(condition, value)] = case.when_then_expr.as_slice() else {
        return Ok(Transformed::no(Expr::AggregateFunction(aggregate)));
    };
    if case.expr.is_none()
        && let Expr::IsNotNull(test) = condition.as_ref()
        && matches!(value.as_ref(), Expr::Column(_)) && test == value
        && matches!(case.else_expr.as_deref(), Some(Expr::Literal(v,_)) if !v.is_null())
    {
        // Both CASE branches are non-null. Counting it counts input rows,
        // including an optional match's null-extended occurrence.
        aggregate.params.args=vec![lit(1_i64)];
        return Ok(Transformed::yes(Expr::AggregateFunction(aggregate)));
    }
    if case.expr.is_some()
        || !matches!(value.as_ref(), Expr::Literal(value, _) if !value.is_null())
        || case.else_expr.as_ref().is_some_and(|value| !matches!(value.as_ref(), Expr::Literal(value, _) if value.is_null()))
    { return Ok(Transformed::no(Expr::AggregateFunction(aggregate))); }
    let filter = condition.clone();
    // No predicate evaluation is moved between rows or duplicated. False and
    // unknown both contributed NULL before, and both fail the FILTER now.
    aggregate.params.args = vec![lit(1_i64)];
    aggregate.params.filter = Some(filter);
    Ok(Transformed::yes(Expr::AggregateFunction(aggregate)))
}

pub(super) fn rewrite(plan: LogicalPlan, dialect: SqlDialect) -> SqlResult<LogicalPlan> {
    if !matches!(dialect, SqlDialect::DuckDb | SqlDialect::Postgres) { return Ok(plan); }
    Ok(plan.transform_up_with_subqueries(|node| {
        let names = NamePreserver::new(&node);
        node.map_expressions(|expr| {
            let saved = names.save(&expr);
            expr.transform_up(rewrite_expr)?.map_data(|expr| Ok(saved.restore(expr)))
        })
    })?.data)
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__sql__count_filter__tests.rs"]
mod tests;
