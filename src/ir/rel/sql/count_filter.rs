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
mod tests {
    use super::*;
    use datafusion::logical_expr::{col, ExprFunctionExt};
    fn count(value: Expr) -> Expr { datafusion::functions_aggregate::expr_fn::count(value) }
    #[test]
    fn conditional_count_preserves_unknown_and_skips_distinct_or_computed_values() {
        let condition = col("p");
        let indicator = datafusion::logical_expr::when(condition.clone(), lit(9_i64))
            .otherwise(lit(ScalarValue::Int64(None))).unwrap();
        let result = rewrite_expr(count(indicator.clone())).unwrap();
        assert!(result.transformed);
        let Expr::AggregateFunction(result) = result.data else { panic!() };
        assert_eq!(result.params.filter, Some(Box::new(condition)));
        assert_eq!(result.params.args, vec![lit(1_i64)]);
        for expr in [count(indicator.clone()).distinct().build().unwrap(),
            count(indicator).filter(col("q")).build().unwrap(),
            count(datafusion::logical_expr::when(col("p"), lit(1_i64) / lit(0_i64))
                .otherwise(lit(ScalarValue::Int64(None))).unwrap()),
            count(datafusion::logical_expr::when(col("p"), lit(1_i64)).otherwise(lit(0_i64)).unwrap())] {
            assert!(!rewrite_expr(expr).unwrap().transformed);
        }
    }
}
