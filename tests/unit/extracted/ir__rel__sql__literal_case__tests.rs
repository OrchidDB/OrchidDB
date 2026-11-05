
    use super::*;
    use datafusion::prelude::col;
    use datafusion::logical_expr::expr::Case;
    fn case(conditions: Vec<(Expr, Expr)>, otherwise: Option<Expr>) -> Expr {
        Expr::Case(Case::new(None, conditions.into_iter().map(|(a,b)| (Box::new(a),Box::new(b))).collect(), otherwise.map(Box::new)))
    }
    #[test]
    fn false_and_null_conditions_do_not_select_a_branch() {
        let value = case(vec![(lit(false),lit(1_i64)), (lit(ScalarValue::Boolean(None)),lit(2_i64))], Some(lit(3_i64)));
        assert_eq!(simplify(value, &DFSchema::empty()).unwrap().data, lit(3_i64));
        let value = case(vec![(lit(false),lit(1_i64))],None);
        assert_eq!(simplify(value, &DFSchema::empty()).unwrap().data,lit(ScalarValue::Int64(None)));
    }
    #[test]
    fn true_condition_discards_only_later_branches() {
        let condition = lit(1_i64).eq(lit(2_i64));
        let value = case(vec![(condition.clone(),lit(1_i64)),(lit(true),lit(2_i64)),(lit(true),lit(3_i64))],Some(lit(4_i64)));
        assert_eq!(simplify(value,&DFSchema::empty()).unwrap().data,case(vec![(condition,lit(1_i64))],Some(lit(2_i64))));
    }
    #[test]
    fn simple_case_boolean_values_are_not_conditions() {
        let value = Expr::Case(Case::new(Some(Box::new(lit(false))),vec![(Box::new(lit(false)),Box::new(lit(1_i64)))],Some(Box::new(lit(2_i64)))));
        assert_eq!(simplify(value.clone(),&DFSchema::empty()).unwrap().data,value);
    }

#[tokio::test]
async fn nullable_case_coalesce_preserves_row_values_names_and_result_types() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let input = LogicalPlanBuilder::values(vec![
        vec![lit(ScalarValue::Int64(Some(7)))],
        vec![lit(ScalarValue::Int64(None))],
        vec![lit(ScalarValue::Int64(Some(7)))],
    ]).unwrap().project(vec![col("column1").alias("value")]).unwrap().build().unwrap();
    for fallback in [ScalarValue::Int64(Some(9)), ScalarValue::Int64(None)] {
        let expression = case(vec![(col("value").is_not_null(),col("value"))],Some(lit(fallback.clone())));
        let simplified = simplify(expression.clone(),input.schema()).unwrap();
        assert!(simplified.transformed);
        assert_eq!(simplified.data.get_type(input.schema()).unwrap(),DataType::Int64);
        let plan = LogicalPlanBuilder::from(input.clone()).project(vec![expression.alias("chosen value")]).unwrap().build().unwrap();
        let rewritten = rewrite(plan.clone()).unwrap();
        assert_eq!(rewritten.schema(),plan.schema());
        let batches = crate::ir::rel::execute_test_plan(rewritten).await.unwrap();
        let mut values = batches.iter().flat_map(|batch| (0..batch.num_rows()).map(|row|
            ScalarValue::try_from_array(batch.column(0),row).unwrap())).collect::<Vec<_>>();
        values.sort_by_key(|v| match v {ScalarValue::Int64(v)=>*v,_=>panic!()});
        let mut expected = vec![ScalarValue::Int64(Some(7)),fallback,ScalarValue::Int64(Some(7))];
        expected.sort_by_key(|v| match v {ScalarValue::Int64(v)=>*v,_=>panic!()});
        assert_eq!(values,expected);
    }
}

#[test]
fn case_rewrites_keep_volatile_aggregate_evaluation_and_unselected_errors() {
    use datafusion::logical_expr::{create_udf,Volatility,ColumnarValue};
    let volatile = create_udf("volatile_case",vec![],DataType::Int64,Volatility::Volatile,
        Arc::new(|_| Ok(ColumnarValue::Scalar(ScalarValue::Int64(Some(7))))));
    for value in [volatile.call(vec![]),datafusion::functions_aggregate::expr_fn::sum(volatile.call(vec![]))] {
        let expression=case(vec![(value.clone().is_not_null(),value)],Some(lit(9_i64)));
        let result=simplify(expression.clone(),&DFSchema::empty()).unwrap();
        assert!(!result.transformed);
        assert_eq!(result.data,expression);
    }
    let expression=case(vec![(lit(true),lit(7_i64)),(lit(true),lit(1_i64)/lit(0_i64))],None);
    assert_eq!(simplify(expression,&DFSchema::empty()).unwrap().data,lit(7_i64));
}

#[tokio::test]
async fn aggregate_case_coalesce_preserves_empty_and_all_null_groups() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for empty in [false,true] {
        let input=LogicalPlanBuilder::values(vec![vec![lit(ScalarValue::Int64(None))]]).unwrap();
        let input=if empty {input.filter(lit(false)).unwrap()} else {input};
        let aggregate=datafusion::functions_aggregate::expr_fn::sum(col("column1"));
        let expression=case(vec![(aggregate.clone().is_not_null(),aggregate)],Some(lit(9_i64)));
        let plan=input.aggregate(Vec::<Expr>::new(),vec![expression.alias("total")]).unwrap().build().unwrap();
        let rewritten=rewrite(plan.clone()).unwrap();
        assert_eq!(rewritten.schema(),plan.schema());
        let batches=crate::ir::rel::execute_test_plan(rewritten).await.unwrap();
        assert_eq!(batches.iter().map(|b|b.num_rows()).sum::<usize>(),1);
        let batch=batches.iter().find(|b|b.num_rows()>0).unwrap();
        assert_eq!(ScalarValue::try_from_array(batch.column(0),0).unwrap(),ScalarValue::Int64(Some(9)));
    }
}
