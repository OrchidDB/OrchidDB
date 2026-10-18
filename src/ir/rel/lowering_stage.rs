//! Required implementation normalization. This stage is independent of optional
//! optimization and constant folding; partitioning only sees its fixed point.
use super::*;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::optimizer::{OptimizerConfig, OptimizerRule};

pub(super) fn normalize(
    mut plan: LogicalPlan,
    config: &dyn OptimizerConfig,
    dialect: Option<sql::SqlDialect>,
    context: &crate::ir::rules::RuleContext,
) -> RelResult<LogicalPlan> {
    for _ in 0..context.budget.iterations {
        let before = plan.clone();
        plan = super::pipeline::typed_sql(plan, context, "rel.required_normalization", true, |plan| {
            let plan = plan.transform_up_with_subqueries(|node| {
                // DISTINCT is a physical implementation requirement. Do not use
                // DataFusion's weaker FD shortcut to discard duplicate rows.
                if let LogicalPlan::Distinct(datafusion::logical_expr::Distinct::All(input)) = &node {
                    let builder = datafusion::logical_expr::LogicalPlanBuilder::from(input.as_ref().clone());
                    let result = if input.schema().fields().is_empty() { builder.limit(0, Some(1))?.build()? }
                    else { builder.aggregate(input.schema().columns().into_iter().map(Expr::Column).collect::<Vec<_>>(), Vec::<Expr>::new())?.build()? };
                    return Ok(Transformed::yes(result));
                }
                datafusion::optimizer::replace_distinct_aggregate::ReplaceDistinctWithAggregate::new().rewrite(node, config)
            })?.data;
            let plan = super::empty_relations::simplify(plan).map_err(|e| sql::SqlError::Unsupported(e.to_string()))?;
            let plan = plan.transform_up_with_subqueries(|node| {
                let names = datafusion::logical_expr::expr_rewriter::NamePreserver::new(&node);
                node.map_expressions(|expr| {
                let name = names.save(&expr);
                expr.transform_up(|expr| {
                    if let Expr::Exists(e) = &expr
                        && matches!(e.subquery.subquery.as_ref(), LogicalPlan::EmptyRelation(r) if !r.produce_one_row) {
                        return Ok(Transformed::yes(lit(e.negated)));
                    }
                    Ok(Transformed::no(expr))
                })?.map_data(|expr| Ok(name.restore(expr)))
            })})?.data;
            match dialect { Some(dialect) => sql::lowering::rewrite_relations(plan, dialect), None => Ok(plan) }
        }).map_err(|e| match e {sql::SqlError::RulePipeline(error)=>RelError::RulePipeline(error), other=>RelError::Unsupported(other.to_string())})?;
        if plan == before {
            return Ok(plan);
        }
    }
    Err(RelError::Unsupported(
        "required relational lowering did not converge within the query iteration budget".into(),
    ))
}

/// No residual logical construct may be handed to a physical planner that has
/// no implementation for it. SQL regions expose no expressions here.
pub(super) fn validate_residual(plan: &LogicalPlan) -> RelResult<()> {
    use datafusion::common::tree_node::TreeNodeRecursion;
    plan.apply(|node| {
        if matches!(node, LogicalPlan::Distinct(_)) {
            return Err(DataFusionError::Plan("missing required transformation: residual DISTINCT must lower to an aggregate".into()));
        }
        for expr in node.expressions() {
            expr.apply(|expr| {
                if matches!(expr, Expr::Exists(_) | Expr::ScalarSubquery(_) | Expr::InSubquery(_) | Expr::SetComparison(_)) {
                    return Err(DataFusionError::Plan(format!("missing required transformation: residual subquery must be placed in SQL or decorrelated: {expr}")));
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::optimizer::OptimizerContext;
    #[test]
    fn distinct_normalization_is_required_with_optimization_disabled() {
        let input = LogicalPlanBuilder::empty(true)
            .project(vec![lit(1_i64).alias("x")])
            .unwrap()
            .distinct()
            .unwrap()
            .build()
            .unwrap();
        let mut context = crate::ir::rules::RuleContext::default();
        context.optimize = false;
        let output = normalize(
            input,
            &OptimizerContext::new(),
            Some(sql::SqlDialect::DuckDb),
            &context,
        )
        .unwrap();
        assert!(matches!(output, LogicalPlan::Aggregate(_)), "{output}");
        assert!(context.visits() > 0);
    }
    #[test]
    fn required_lowering_cannot_silently_skip_an_exhausted_budget() {
        let mut context = crate::ir::rules::RuleContext::default();
        context.budget.iterations = 0;
        assert!(
            normalize(
                LogicalPlanBuilder::empty(true).build().unwrap(),
                &OptimizerContext::new(),
                None,
                &context
            )
            .is_err()
        );
    }
    #[test]
    fn residual_validator_identifies_missing_transformations() {
        let input = LogicalPlanBuilder::empty(true)
            .project(vec![lit(1_i64).alias("x")])
            .unwrap()
            .distinct()
            .unwrap()
            .build()
            .unwrap();
        assert!(
            validate_residual(&input)
                .unwrap_err()
                .to_string()
                .contains("residual DISTINCT")
        );
    }
}

/// Residual normalization runs only after SQL has had first refusal. These
/// rules implement subqueries in the host; they are never a substitute for
/// trying the supported SQL representation first.
pub(super) fn normalize_residual(
    plan: LogicalPlan,
    config: &dyn OptimizerConfig,
    context: &crate::ir::rules::RuleContext,
) -> RelResult<LogicalPlan> {
    use datafusion::common::tree_node::TreeNodeRecursion;
    let mut needed = false;
    plan.apply(|node| {
        for expr in node.expressions() {
            expr.apply(|expr| {
                needed |= matches!(
                    expr,
                    Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_)
                );
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    if !needed {
        return Ok(plan);
    }
    let plan=datafusion::optimizer::Optimizer::with_rules(vec![
        Arc::new(datafusion::optimizer::decorrelate_predicate_subquery::DecorrelatePredicateSubquery::new()),
        Arc::new(datafusion::optimizer::scalar_subquery_to_join::ScalarSubqueryToJoin::new()),
    ]).optimize(plan,config,|_,_|{})?;
    normalize(plan, config, None, context)
}
