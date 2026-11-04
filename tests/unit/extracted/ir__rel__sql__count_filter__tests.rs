
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
