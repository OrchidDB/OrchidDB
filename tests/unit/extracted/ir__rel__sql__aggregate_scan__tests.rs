
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
    fn grouped_columns_use_physical_scan_and_preserve_schema() {
        let input = LogicalPlanBuilder::from(scan()).project(vec![col("age").alias("person_age")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input).aggregate(vec![col("person_age")], vec![datafusion::functions_aggregate::count::count_all()]).unwrap().build().unwrap();
        let schema = plan.schema().clone();
        let rewritten = rewrite(plan).unwrap();
        assert_eq!(rewritten.schema(), &schema);
        let LogicalPlan::Aggregate(aggregate) = &rewritten else { panic!("aggregate expected") };
        assert!(matches!(aggregate.input.as_ref(), LogicalPlan::TableScan(_)));
        let sql = super::super::unparse_plan(rewritten, SqlDialect::DuckDb).unwrap();
        assert!(!sql.contains("FROM (SELECT"), "{sql}");
        assert!(sql.contains("GROUP BY"), "{sql}");
    }
    #[test]
    fn literal_grouping_keys_and_grouping_sets_keep_their_scope() {
        use datafusion::logical_expr::GroupingSet;
        let input = LogicalPlanBuilder::from(scan()).project(vec![
            col("age").alias("person_age"), lit(1_i64).alias("constant_key"),
        ]).unwrap().build().unwrap();
        for groups in [vec![col("constant_key")], vec![Expr::GroupingSet(GroupingSet::Rollup(vec![col("person_age")]))]] {
            let plan = LogicalPlanBuilder::from(input.clone()).aggregate(groups,
                vec![datafusion::functions_aggregate::count::count_all()]).unwrap().build().unwrap();
            assert_eq!(rewrite(plan.clone()).unwrap(), plan);
        }
    }

    #[test]
    fn correlated_grouping_expression_keeps_its_input_scope() {
        let input = LogicalPlanBuilder::from(scan()).project(vec![col("age").alias("person_age")]).unwrap().build().unwrap();
        let correlated = Expr::OuterReferenceColumn(Arc::new(Field::new("outer_age", DataType::Int64, true)),
            Column::new_unqualified("outer_age"));
        let plan = LogicalPlanBuilder::from(input).aggregate(vec![col("person_age") + correlated],
            vec![datafusion::functions_aggregate::count::count_all()]).unwrap().build().unwrap();
        assert_eq!(rewrite(plan.clone()).unwrap(), plan);
    }

