//! Resolve aggregate operands through a trivial physical-scan naming layer.
//! This removes no computation: every projected value is a column or literal.
use super::*;
use datafusion::logical_expr::Aggregate;

fn value(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Alias(alias) => value(&alias.expr),
        Expr::Column(_) | Expr::Literal(..) => Some(expr.clone()),
        _ => None,
    }
}

pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Aggregate(aggregate) = &node else { return Ok(Transformed::no(node)); };
        // Constant grouping keys have dialect-specific ordinal interpretation.
        // Keep grouping sets and grouped aggregates outside this narrow rule.
        if !aggregate.group_expr.is_empty() { return Ok(Transformed::no(node)); }
        let LogicalPlan::Projection(projection) = aggregate.input.as_ref() else { return Ok(Transformed::no(node)); };
        let LogicalPlan::TableScan(scan) = projection.input.as_ref() else { return Ok(Transformed::no(node)); };
        if scan.fetch.is_some() || !scan.filters.is_empty() { return Ok(Transformed::no(node)); }
        let Some(values) = projection.expr.iter().map(value).collect::<Option<Vec<_>>>() else { return Ok(Transformed::no(node)); };
        let mut subquery = false;
        for expr in &aggregate.aggr_expr {
            expr.apply(|expr| {
                if matches!(expr, Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_) | Expr::OuterReferenceColumn(..)) {
                    subquery = true;
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        if subquery { return Ok(Transformed::no(node)); }
        let expressions = aggregate.aggr_expr.iter().zip(aggregate.schema.iter())
            .map(|(expr, (qualifier, field))| {
                let expr = expr.clone().transform_up(|expr| {
                    if let Expr::Column(column) = &expr {
                        let index = projection.schema.index_of_column(column)?;
                        return Ok(Transformed::yes(values[index].clone()));
                    }
                    Ok(Transformed::no(expr))
                })?.data;
                let expr = match expr { Expr::Alias(alias) => *alias.expr, expr => expr };
                Ok(expr.alias_qualified(qualifier.cloned(), field.name()))
            }).collect::<datafusion::common::Result<Vec<_>>>()?;
        let rewritten = Aggregate::try_new_with_schema(projection.input.clone(), vec![], expressions, aggregate.schema.clone())?;
        Ok(Transformed::yes(LogicalPlan::Aggregate(rewritten)))
    })?.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::datasource::{empty::EmptyTable, provider_as_source};
    use datafusion::logical_expr::LogicalPlanBuilder;
    use datafusion::prelude::col;
    fn scan() -> LogicalPlan {
        let schema = Arc::new(Schema::new(vec![Field::new("age", DataType::Int64, true)]));
        LogicalPlanBuilder::scan("people", provider_as_source(Arc::new(EmptyTable::new(schema))), None).unwrap().build().unwrap()
    }
    #[test]
    fn count_and_average_use_physical_scan_and_preserve_names() {
        let input = LogicalPlanBuilder::from(scan()).project(vec![col("age").alias("person_age"), lit("Person").alias("label")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input).aggregate(Vec::<Expr>::new(), vec![
            datafusion::functions_aggregate::count::count_all().alias("row_count"),
            datafusion::functions_aggregate::average::avg(col("person_age")).alias("mean_age"),
        ]).unwrap().build().unwrap();
        let schema = plan.schema().clone();
        let rewritten = rewrite(plan).unwrap();
        assert_eq!(rewritten.schema(), &schema);
        let LogicalPlan::Aggregate(aggregate) = &rewritten else { panic!("aggregate expected") };
        assert!(matches!(aggregate.input.as_ref(), LogicalPlan::TableScan(_)));
        let sql = super::super::unparse_plan(rewritten, SqlDialect::DuckDb).unwrap();
        assert!(sql.contains("FROM \"people\""), "{sql}");
        assert!(!sql.contains("FROM (SELECT"), "{sql}");
        assert!(sql.contains("avg(\"age\")"), "{sql}");
    }
    #[test]
    fn keeps_computed_projection_even_when_count_does_not_reference_it() {
        let input = LogicalPlanBuilder::from(scan()).project(vec![(col("age") / lit(0_i64)).alias("computed")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input).aggregate(Vec::<Expr>::new(), vec![datafusion::functions_aggregate::count::count_all()]).unwrap().build().unwrap();
        assert_eq!(rewrite(plan.clone()).unwrap(), plan);
    }
    #[test]
    fn keeps_grouping_scope() {
        let input = LogicalPlanBuilder::from(scan()).project(vec![col("age").alias("person_age")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input).aggregate(vec![col("person_age")], vec![datafusion::functions_aggregate::count::count_all()]).unwrap().build().unwrap();
        assert_eq!(rewrite(plan.clone()).unwrap(), plan);
    }
}
