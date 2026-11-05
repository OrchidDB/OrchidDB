
    use super::*;

    fn scope() -> LogicalPlan {
        let values = datafusion::functions_nested::expr_fn::make_array(vec![lit(1_i64),lit(9_i64),lit(ScalarValue::Int64(None))]);
        LogicalPlanBuilder::empty(true).project(vec![values.alias("x")]).unwrap()
            .unnest_column(Column::new_unqualified("x")).unwrap().build().unwrap()
    }

    #[test]
    fn constructor_item_bounds_prove_runtime_arithmetic() {
        let plan=scope();
        let x=col_exact("x");
        assert_eq!(integer_bounds(&plan,&x),Some((1,9)));
        assert_eq!(integer_bounds(&plan,&(x.clone()*x.clone())),Some((1,81)));
        assert_eq!(integer_bounds(&plan,&(x.clone()+x.clone())),Some((2,18)));
        let remainder=Expr::BinaryExpr(datafusion::logical_expr::BinaryExpr::new(Box::new(x.clone()),Operator::Modulo,Box::new(x.clone())));
        assert!(arithmetic_is_total(&plan,&remainder));
        assert!(arithmetic_is_total(&plan,&datafusion::functions::math::expr_fn::abs(x-lit(5_i64))));
    }

    #[test]
    fn overflow_and_zero_divisor_are_not_total() {
        let plan=scope();
        for expr in [lit(i64::MAX)+lit(1_i64),lit(i64::MIN)-lit(1_i64),lit(i64::MAX)*lit(2_i64),datafusion::functions::math::expr_fn::abs(lit(i64::MIN))] {
            assert!(!arithmetic_is_total(&plan,&expr),"{expr}");
        }
        for divisor in [lit(0_i64),lit(-1_i64),col_exact("x")-lit(5_i64)] {
            let expr=Expr::BinaryExpr(datafusion::logical_expr::BinaryExpr::new(Box::new(lit(i64::MIN)),Operator::Modulo,Box::new(divisor)));
            assert!(!arithmetic_is_total(&plan,&expr),"{expr}");
        }
    }

    #[test]
    fn positive_modulo_is_total_for_unknown_integer_input() {
        let schema=Arc::new(Schema::new(vec![Field::new("x",DataType::Int64,true)]));
        let table=MemTable::try_new(schema,vec![vec![]]).unwrap();
        let plan=LogicalPlanBuilder::scan("unknown",provider_as_source(Arc::new(table)),None).unwrap().build().unwrap();
        let x=col_exact("x");
        assert_eq!(integer_bounds(&plan,&x),None);
        let expr=Expr::BinaryExpr(datafusion::logical_expr::BinaryExpr::new(Box::new(x.clone()),Operator::Modulo,Box::new(lit(2_i64))));
        assert_eq!(integer_bounds(&plan,&expr),Some((-1,1)));
        assert!(arithmetic_is_total(&plan,&expr));
        assert!(!arithmetic_is_total(&plan,&(x+lit(1_i64))));
    }

    #[test]
    fn shadowed_dynamic_source_does_not_retain_constructor_domain() {
        let plan=LogicalPlanBuilder::from(scope()).project(vec![col_exact("x").alias("list")]).unwrap().build().unwrap();
        assert_eq!(integer_list_item_bounds(&plan,&col_exact("list")),None);
        assert_eq!(integer_list_item_bounds(&plan,&datafusion::functions_nested::expr_fn::make_array(vec![])),None);
    }

#[test]
fn list_alignment_preserves_nulls_without_permitting_lossy_or_throwing_casts() {
    let plan = LogicalPlanBuilder::empty(true).build().unwrap();
    let array = datafusion::functions_nested::expr_fn::make_array;
    let list = |ty| DataType::List(Arc::new(Field::new("item", ty, true)));
    for (value, target, expected) in [
        (array(vec![lit(ScalarValue::Null)]), list(DataType::Int64), true),
        (array(vec![array(vec![lit(ScalarValue::Null)])]), list(list(DataType::Int64)), true),
        (array(vec![lit(9_007_199_254_740_993_i64)]), list(DataType::Float64), false),
        (array(vec![lit("invalid integer")]), list(DataType::Int64), false),
        (array(vec![lit(i64::MAX) + lit(1_i64)]), list(DataType::Int64), false),
    ] {
        let cast = Expr::Cast(Cast::new(Box::new(value), target));
        assert_eq!(repeatable(&plan, &cast), expected, "{cast}");
    }
}
