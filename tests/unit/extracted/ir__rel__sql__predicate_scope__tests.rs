
    use super::*;
    #[test]
    fn literal_branch_specialization_keeps_null_semantics() {
        assert_eq!(literals(lit("A").eq(lit("B"))).unwrap(),lit(false));
        assert_eq!(literals(lit(ScalarValue::Utf8(None)).eq(lit("B"))).unwrap(),lit(ScalarValue::Boolean(None)));
    }
    #[test]
    fn filter_crosses_renames_but_not_limits() {
        let base=LogicalPlanBuilder::empty(true).project(vec![lit(1_i64).alias("x")]).unwrap().build().unwrap();
        let plan=LogicalPlanBuilder::from(base.clone()).project(vec![datafusion::prelude::col("x").alias("y")]).unwrap()
            .filter(datafusion::prelude::col("y").eq(lit(1_i64))).unwrap().build().unwrap();
        let optimized=rewrite(plan).unwrap();
        assert!(matches!(optimized,LogicalPlan::Projection(_)));
        let plan=LogicalPlanBuilder::from(base).limit(0,Some(1)).unwrap()
            .filter(datafusion::prelude::col("x").eq(lit(1_i64))).unwrap().build().unwrap();
        assert_eq!(rewrite(plan.clone()).unwrap(),plan);
    }
