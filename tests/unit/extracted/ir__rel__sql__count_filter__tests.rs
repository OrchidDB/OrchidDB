
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

#[tokio::test]
async fn conditional_counts_preserve_optional_rows_and_empty_inputs() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for empty in [false, true] {
        let input = LogicalPlanBuilder::values(vec![
            vec![lit(ScalarValue::Int64(Some(7))), lit(ScalarValue::Boolean(Some(true)))],
            vec![lit(ScalarValue::Int64(None)), lit(ScalarValue::Boolean(Some(false)))],
            vec![lit(ScalarValue::Int64(None)), lit(ScalarValue::Boolean(None))],
        ]).unwrap().project(vec![col("column1").alias("value"), col("column2").alias("predicate")]).unwrap();
        let input = if empty { input.filter(lit(false)).unwrap() } else { input };
        let total = datafusion::logical_expr::when(col("value").is_not_null(), col("value"))
            .otherwise(lit(0_i64)).unwrap();
        let indicator = datafusion::logical_expr::when(col("predicate"), lit(9_i64))
            .otherwise(lit(ScalarValue::Int64(None))).unwrap();
        let plan = input.aggregate(Vec::<Expr>::new(), vec![count(total).alias("total"), count(indicator).alias("selected")])
            .unwrap().build().unwrap();
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres, SqlDialect::Custom(&super::super::trino::TRINO)] {
            let rewritten = rewrite(plan.clone(), dialect).unwrap();
            assert_eq!(rewritten.schema(), plan.schema());
            if matches!(dialect, SqlDialect::Custom(_)) { assert_eq!(rewritten, plan); }
            else { assert_ne!(rewritten, plan); }
            let batches = crate::ir::rel::execute_test_plan(rewritten).await.unwrap();
            assert_eq!(batches.iter().map(|batch| batch.num_rows()).sum::<usize>(), 1);
            let batch = batches.iter().find(|batch| batch.num_rows() > 0).unwrap();
            for (index, expected) in [if empty {0} else {3}, if empty {0} else {1}].into_iter().enumerate() {
                assert_eq!(ScalarValue::try_from_array(batch.column(index), 0).unwrap(), ScalarValue::Int64(Some(expected)));
            }
        }
    }
}

#[test]
fn conditional_counts_preserve_case_shapes_that_can_change_null_semantics() {
    use datafusion::logical_expr::expr::Case;
    let column = col("value");
    let cases = [
        Case::new(Some(Box::new(column.clone())), vec![(Box::new(lit(1_i64)), Box::new(lit(1_i64)))], None),
        Case::new(None, vec![], Some(Box::new(lit(1_i64)))),
        Case::new(None, vec![(Box::new(lit(true)), Box::new(lit(1_i64))),
            (Box::new(lit(false)), Box::new(lit(2_i64)))], None),
        Case::new(None, vec![(Box::new(column.clone().is_not_null()), Box::new(column))],
            Some(Box::new(lit(ScalarValue::Int64(None))))),
    ];
    for case in cases {
        let expression = count(Expr::Case(case));
        let result = rewrite_expr(expression.clone()).unwrap();
        assert!(!result.transformed);
        assert_eq!(result.data, expression);
    }
}

#[test]
fn conditional_count_without_else_rewrites_but_aggregate_modifiers_are_preserved() {
    use datafusion::logical_expr::expr::Case;
    let indicator = Expr::Case(Case::new(None,
        vec![(Box::new(col("predicate")), Box::new(lit(1_i64)))], None));
    let rewritten = rewrite_expr(count(indicator.clone())).unwrap();
    assert!(rewritten.transformed);
    let Expr::AggregateFunction(function) = rewritten.data else { panic!() };
    assert_eq!(function.params.filter, Some(Box::new(col("predicate"))));
    for expression in [
        count(indicator.clone()).order_by(vec![col("value").sort(true,false)]).build().unwrap(),
        datafusion::functions_aggregate::expr_fn::sum(indicator.clone()),
        count(col("value")),
    ] {
        let actual = rewrite_expr(expression.clone()).unwrap();
        assert!(!actual.transformed);
        assert_eq!(actual.data, expression);
    }
}
