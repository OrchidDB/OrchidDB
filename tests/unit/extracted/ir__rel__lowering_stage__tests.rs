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

#[tokio::test]
async fn zero_column_distinct_preserves_empty_and_singleton_cardinality() {
    for produce_one_row in [false, true] {
        let plan = LogicalPlanBuilder::empty(produce_one_row)
            .distinct()
            .unwrap()
            .build()
            .unwrap();
        let mut context = crate::ir::rules::RuleContext::default();
        context.optimize = false;
        let normalized = normalize(plan, &OptimizerContext::new(), None, &context).unwrap();
        validate_residual(&normalized).unwrap();
        let batches = crate::ir::rel::execute_test_plan(normalized).await.unwrap();
        assert_eq!(
            batches.iter().map(RecordBatch::num_rows).sum::<usize>(),
            usize::from(produce_one_row)
        );
        assert!(batches.iter().all(|batch| batch.num_columns() == 0));
    }
}

#[tokio::test]
async fn residual_scalar_and_filter_subqueries_are_executable_after_normalization() {
    use datafusion::logical_expr::{
        Subquery,
        expr::{Exists, InSubquery},
    };
    let subquery = Subquery {
        subquery: Arc::new(
            LogicalPlanBuilder::empty(true)
                .project(vec![lit(7_i64).alias("v")])
                .unwrap()
                .build()
                .unwrap(),
        ),
        outer_ref_columns: vec![],
        spans: Default::default(),
    };
    for (expression, expected) in [
        (Expr::ScalarSubquery(subquery.clone()), "7"),
        (Expr::Exists(Exists::new(subquery.clone(), false)), "true"),
        (
            Expr::InSubquery(InSubquery::new(Box::new(lit(7_i64)), subquery, false)),
            "true",
        ),
    ] {
        let plan = if matches!(&expression, Expr::ScalarSubquery(_)) {
            LogicalPlanBuilder::empty(true)
                .project(vec![expression.alias("result")])
                .unwrap()
                .build()
                .unwrap()
        } else {
            LogicalPlanBuilder::empty(true)
                .filter(expression)
                .unwrap()
                .project(vec![lit(true).alias("result")])
                .unwrap()
                .build()
                .unwrap()
        };
        assert!(
            validate_residual(&plan)
                .unwrap_err()
                .to_string()
                .contains("residual subquery")
        );
        let context = crate::ir::rules::RuleContext::default();
        let normalized = normalize_residual(plan, &OptimizerContext::new(), &context).unwrap();
        validate_residual(&normalized).unwrap();
        let batches = crate::ir::rel::execute_test_plan(normalized).await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let batch = batches.iter().find(|batch| batch.num_rows() == 1).unwrap();
        assert_eq!(
            arrow::util::display::array_value_to_string(batch.column(0), 0).unwrap(),
            expected
        );
    }
}

#[test]
fn empty_exists_folds_with_both_negations_and_preserves_output_alias() {
    use datafusion::logical_expr::{Subquery, expr::Exists};
    for negated in [false, true] {
        let subquery = Subquery {
            subquery: Arc::new(LogicalPlanBuilder::empty(false).build().unwrap()),
            outer_ref_columns: vec![],
            spans: Default::default(),
        };
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![
                Expr::Exists(Exists::new(subquery, negated)).alias("answer"),
            ])
            .unwrap()
            .build()
            .unwrap();
        let context = crate::ir::rules::RuleContext::default();
        let normalized = normalize(plan, &OptimizerContext::new(), None, &context).unwrap();
        validate_residual(&normalized).unwrap();
        assert_eq!(normalized.schema().field(0).name(), "answer");
        let LogicalPlan::Projection(project) = normalized else {
            panic!("projection missing")
        };
        assert_eq!(project.expr, vec![lit(negated).alias("answer")]);
    }
}
