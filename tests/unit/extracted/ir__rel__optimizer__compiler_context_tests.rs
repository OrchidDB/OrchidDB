
    #[test]
    fn query_clock_is_fixed_without_an_execution_environment() {
        use datafusion::optimizer::OptimizerConfig;
        let compiler = super::session();
        let query = compiler.state();
        let start = query.query_execution_start_time();
        assert!(start.is_some());
        assert_eq!(start, query.query_execution_start_time());
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        assert!(matches!(query.optimize(&plan).unwrap(), datafusion::logical_expr::LogicalPlan::EmptyRelation(_)));
    }
