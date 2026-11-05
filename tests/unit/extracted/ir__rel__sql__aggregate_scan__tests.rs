
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


#[test]
fn bounded_and_filtered_scans_keep_their_projection_boundary() {
    for limited in [false, true] {
        let LogicalPlan::TableScan(mut source) = scan() else { panic!() };
        if limited { source.fetch = Some(1); }
        else { source.filters = vec![col("age").gt(lit(18_i64))]; }
        let plan = LogicalPlanBuilder::from(LogicalPlan::TableScan(source))
            .project(vec![col("age").alias("person_age")]).unwrap()
            .aggregate(Vec::<Expr>::new(), vec![datafusion::functions_aggregate::count::count_all().alias("total")])
            .unwrap().build().unwrap();
        assert_eq!(rewrite(plan.clone()).unwrap(), plan);
    }
}

#[test]
fn aggregate_scan_substitution_preserves_filtered_distinct_operands_and_group_functions() {
    use datafusion::logical_expr::ExprFunctionExt;
    let input=LogicalPlanBuilder::from(scan()).project(vec![col("age").alias("person_age")]).unwrap().build().unwrap();
    let function=datafusion::functions_aggregate::expr_fn::count(col("person_age"))
        .distinct().filter(col("person_age").gt(lit(18_i64))).build().unwrap();
    let plan=LogicalPlanBuilder::from(input).aggregate(vec![(col("person_age")+lit(1_i64)).alias("bucket")],
        vec![function.alias("adults")]).unwrap().build().unwrap();
    let rewritten=rewrite(plan.clone()).unwrap();
    assert_eq!(rewritten.schema(),plan.schema());
    let LogicalPlan::Aggregate(aggregate)=rewritten else {panic!()};
    assert!(matches!(aggregate.input.as_ref(),LogicalPlan::TableScan(_)));
    let Expr::Alias(alias)=&aggregate.aggr_expr[0] else {panic!()};
    let Expr::AggregateFunction(function)=alias.expr.as_ref() else {panic!()};
    assert!(function.params.distinct);
    let expected=Expr::Column(aggregate.input.schema().columns()[0].clone());
    assert_eq!(function.params.args,vec![expected.clone()]);
    assert_eq!(function.params.filter,Some(Box::new(expected.clone().gt(lit(18_i64)))));
    let Expr::Alias(group)=&aggregate.group_expr[0] else {panic!()};
    assert_eq!(group.expr.as_ref(),&(expected+lit(1_i64)));
}
