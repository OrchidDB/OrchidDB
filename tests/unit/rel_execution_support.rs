use super::*;

/// Compiler rewrite tests execute the resulting SQL through the supported engine.
pub(crate) async fn execute_test_plan(plan: LogicalPlan) -> datafusion::common::Result<Vec<RecordBatch>> {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    // The upstream SQL unparser does not emit a bare Values operator. Express
    // fixture rows as SELECT/UNION ALL, retaining multiplicity and column types.
    let plan = plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Values(values) = &node else { return Ok(Transformed::no(node)); };
        let mut branches = Vec::new();
        for row in &values.values {
            let seed = LogicalPlanBuilder::empty(true).build()?;
            let projection = row.iter().zip(values.schema.fields()).map(|(expr, field)| {
                Ok(expr.clone().cast_to(field.data_type(), seed.schema())?.alias(field.name()))
            }).collect::<datafusion::common::Result<Vec<_>>>()?;
            branches.push(LogicalPlanBuilder::from(seed).project(projection)?.build()?);
        }
        let replacement = if let Some(first) = branches.first().cloned() {
            branches.into_iter().skip(1).try_fold(first, |plan, branch| LogicalPlanBuilder::from(plan).union(branch)?.build())?
        } else {
            LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation { produce_one_row: false, schema: values.schema.clone() })
        };
        Ok(Transformed::yes(replacement))
    })?.data;
    let fields = plan.schema().fields().iter().map(|field| field.name().clone()).collect();
    let lowered = LoweredPlan { plan, fields, rule_context: Default::default(), result_form: ResultForm::RowSet, islands: Default::default() };
    execute_lowered(lowered).await.map(|result| vec![result.batch])
        .map_err(|error| DataFusionError::Plan(error.to_string()))
}

