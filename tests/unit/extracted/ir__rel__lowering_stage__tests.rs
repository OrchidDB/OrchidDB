
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
