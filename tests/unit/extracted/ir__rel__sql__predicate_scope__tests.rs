
    use super::*;
    use std::ops::Not;
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

#[test]
fn literal_predicates_keep_three_valued_boolean_and_typed_equality_semantics() {
    use datafusion::prelude::col;
    for value in [None, Some(false), Some(true)] {
        let e = lit(ScalarValue::Boolean(value));
        assert_eq!(literals(e.clone().not()).unwrap(), lit(ScalarValue::Boolean(value.map(|v| !v))));
        assert_eq!(literals(e.clone().is_null()).unwrap(), lit(value.is_none()));
        assert_eq!(literals(e.clone().is_not_null()).unwrap(), lit(value.is_some()));
        for constant in [false, true] {
            for reversed in [false, true] {
                let (left, right) = if reversed { (lit(constant), e.clone()) } else { (e.clone(), lit(constant)) };
                assert_eq!(literals(left.clone().and(right.clone())).unwrap(), if constant { e.clone() } else { lit(false) });
                assert_eq!(literals(left.or(right)).unwrap(), if constant { lit(true) } else { e.clone() });
            }
        }
    }
    for value in [ScalarValue::Int8(Some(7)), ScalarValue::Int16(Some(7)), ScalarValue::Int32(Some(7)),
        ScalarValue::Int64(Some(7)), ScalarValue::UInt8(Some(7)), ScalarValue::UInt16(Some(7)),
        ScalarValue::UInt32(Some(7)), ScalarValue::UInt64(Some(7)), ScalarValue::Utf8(Some("雪".into())),
        ScalarValue::LargeUtf8(Some("雪".into())), ScalarValue::Utf8View(Some("雪".into()))] {
        let null = ScalarValue::try_from(&value.data_type()).unwrap();
        assert_eq!(literals(lit(value.clone()).eq(lit(value.clone()))).unwrap(), lit(true));
        assert_eq!(literals(lit(value.clone()).not_eq(lit(value.clone()))).unwrap(), lit(false));
        assert_eq!(literals(lit(value).not_eq(lit(null))).unwrap(), lit(ScalarValue::Boolean(None)));
    }
    for expression in [col("x").is_null(), col("x").is_not_null(), col("x").not(),
        lit(1_i32).eq(lit(1_i64)), lit(1_f64).eq(lit(1_f64)), lit(1_i64).lt(lit(2_i64))] {
        assert_eq!(literals(expression.clone()).unwrap(), expression);
    }
}

#[test]
fn predicate_scope_only_substitutes_total_aliases_and_typed_nulls() {
    use datafusion::prelude::col;
    let plan = LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("x")]).unwrap().build().unwrap();
    let schema = plan.schema();
    for expression in [col("x"), lit(1_i64), col("x").alias("renamed"), col("x").is_null(),
        col("x").is_not_null(), col("x").eq(lit(1_i64)).not(),
        col("x").gt_eq(lit(1_i64)).and(col("x").lt_eq(lit(2_i64)))] {
        assert!(total(&expression, schema), "{expression}");
    }
    for expression in [col("missing").eq(lit(1_i64)), col("x").eq(lit("1")), col("x") + lit(1_i64)] {
        assert!(!total(&expression, schema), "{expression}");
    }
    let null = datafusion::logical_expr::expr_fn::cast(lit(ScalarValue::Null), DataType::Int64);
    assert!(value(&null));
    assert_eq!(unalias(null.alias("a").alias("b")), lit(ScalarValue::Int64(None)));
    assert!(!value(&(col("x") + lit(1_i64))));
    assert_eq!(filtered(plan.clone(), lit(true)).unwrap(), plan);
}
