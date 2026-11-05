
    use super::*;

    #[tokio::test]
    async fn partitioned_slice_retains_hidden_order_after_filter_and_projection() {
        let input = LogicalPlanBuilder::values(vec![
            vec![lit(0_i64), lit(1_i64)], vec![lit(1_i64), lit(2_i64)],
            vec![lit(0_i64), lit(3_i64)], vec![lit(1_i64), lit(4_i64)],
        ]).unwrap().project(vec![col_exact("column1").alias("g"), col_exact("column2").alias("x")]).unwrap()
            .sort(vec![col_exact("x").sort(false, true)]).unwrap()
            .project(vec![col_exact("g"), (col_exact("x") * lit(10_i64)).alias("y")]).unwrap()
            .filter(col_exact("y").gt_eq(lit(20_i64))).unwrap().build().unwrap();
        let limited = partitioned_limit(input, &["g".into()], 0, Some(1)).unwrap();
        assert!(limited.display_indent().to_string().contains("__apply_order_ordinal ASC"), "{}", limited.display_indent());
        let plan = LogicalPlanBuilder::from(limited).sort(vec![col_exact("g").sort(true, false)]).unwrap().build().unwrap();
        let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
        let values = batches.iter().flat_map(|b| b.column(1).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
        assert_eq!(values, [30, 40]);
    }

    #[tokio::test]
    async fn partitioned_slice_saturates_oversized_upper_bound() {
        let input = LogicalPlanBuilder::values(vec![vec![lit(1_i64)]]).unwrap().build().unwrap();
        let plan = partitioned_limit(input, &["column1".into()], u64::MAX - 1, Some(10)).unwrap();
        let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 0);
    }

#[tokio::test]
async fn correlation_keys_preserve_nulls_and_avoid_existing_internal_aliases() {
    let input = LogicalPlanBuilder::empty(true).project(vec![
        lit(ScalarValue::Null).alias("value"),lit(7_i64).alias("__apply_corr_key_0"),
        lit(8_i64).alias("__apply_corr_key_0_1"),
    ]).unwrap().build().unwrap();
    let (plan, keys, cleanup) = with_apply_correlation_keys(input.clone(), &["value".into(),"value".into()]).unwrap();
    assert_eq!(keys,vec!["__apply_corr_key_0_2"]);
    assert_eq!(cleanup,BTreeSet::from([keys[0].clone()]));
    assert_eq!(plan.schema().field_with_unqualified_name(&keys[0]).unwrap().data_type(),&DataType::Int64);
    let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
    assert_eq!(batches[0].num_columns(),4);
    assert!(ScalarValue::try_from_array(batches[0].column(3),0).unwrap().is_null());
    assert_eq!(ScalarValue::try_from_array(batches[0].column(1),0).unwrap(),ScalarValue::Int64(Some(7)));
    let (unchanged,keys,cleanup) = with_apply_correlation_keys(input.clone(),&[]).unwrap();
    assert_eq!(unchanged,input); assert!(keys.is_empty() && cleanup.is_empty());
}

#[test]
fn correlation_columns_expand_entities_without_crossing_neighbor_bindings() {
    let fields = vec![Field::new(id_col("n"),DataType::Int64,true),Field::new(label_col("n"),DataType::Utf8,true),
        Field::new(prop_col("n","age"),DataType::Int64,true),Field::new(prop_col("neighbor","age"),DataType::Int64,true),
        Field::new("scalar",DataType::Boolean,true),Field::new("__pattern_result",DataType::Boolean,true)];
    let plan = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
        produce_one_row:false,schema:Arc::new(datafusion::common::DFSchema::try_from(Schema::new(fields)).unwrap()),
    });
    let mut expected = vec![id_col("n"),label_col("n"),"scalar".into()];expected.sort();
    assert_eq!(correlation_key_columns(&plan,&["n".into(),"scalar".into(),"n".into()]).unwrap(),expected);
    let mut expected = vec![id_col("n"),label_col("n"),prop_col("n","age"),"__pattern_result".into()];expected.sort();
    assert_eq!(right_apply_output_columns(&plan,&["n".into(),"__pattern_result".into(),"n".into()]).unwrap(),expected);
    assert!(correlation_key_columns(&plan,&["missing".into()]).unwrap_err().to_string().contains("not available relationally"));
    assert!(right_apply_output_columns(&plan,&["__missing_pattern".into()]).unwrap_err().to_string().contains("not available relationally"));
    assert!(binding_column_names(&plan,"scalar").unwrap_err().to_string().contains("not an element binding"));
}

#[tokio::test]
async fn apply_join_preparation_matches_nullable_nested_and_uncorrelated_rows() {
    let lists = |v| ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(None),ScalarValue::Int64(Some(v))],&DataType::Int64));
    for (left_values,right_values,correlated,expected) in [
        (vec![ScalarValue::Int64(None),ScalarValue::Int64(Some(1)),ScalarValue::Int64(Some(1))],vec![ScalarValue::Int64(None),ScalarValue::Int64(Some(1))],true,3),
        (vec![lists(1),lists(2)],vec![lists(1),lists(2)],true,2),
        (vec![ScalarValue::Int64(Some(1)),ScalarValue::Int64(Some(2))],vec![ScalarValue::Int64(Some(9)),ScalarValue::Int64(Some(8)),ScalarValue::Int64(Some(7))],false,6),
    ] {
        let source = |values:Vec<ScalarValue>,right:bool| {
            LogicalPlanBuilder::values(values.into_iter().map(|v|vec![lit(v)]).collect()).unwrap().project(if right {
                vec![col_exact("column1").alias("key"),lit(10_i64).alias("out"),lit(99_i64).alias("__apply_right_key_0")]
            } else {vec![col_exact("column1").alias("key")]}).unwrap().build().unwrap()
        };
        let left=source(left_values,false);let right=source(right_values,true);
        let keys=if correlated {vec!["key".into()]} else {vec![]};
        let (left,right,predicates,cleanup)=prepare_apply_join_inputs(left,right,&keys,&["out".into()]).unwrap();
        assert!(cleanup.contains("__apply_right_key_0_1"));
        let plan=LogicalPlanBuilder::from(left).join_on(right,JoinType::Inner,predicates).unwrap().build().unwrap();
        let batches=crate::ir::rel::execute_test_plan(plan).await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(),expected);
    }
    let left=LogicalPlanBuilder::empty(true).project(vec![lit(1_i64).alias("key")]).unwrap().build().unwrap();
    let right=LogicalPlanBuilder::empty(true).project(vec![lit(2_i64).alias("output")]).unwrap().build().unwrap();
    assert!(prepare_apply_join_inputs(left,right,&["key".into()],&["output".into()]).unwrap_err().to_string().contains("dropped correlation key"));
}

#[tokio::test]
async fn partitioned_limit_preserves_alias_wrapped_order_and_existing_internal_names() {
    let input = LogicalPlanBuilder::values((1..=6_i64).map(|n|vec![lit((n-1)%2),lit(n)]).collect()).unwrap()
        .project(vec![col_exact("column1").alias("g"),col_exact("column2").alias("x")]).unwrap()
        .sort(vec![col_exact("x").sort(false,false)]).unwrap()
        .project(vec![col_exact("g"),col_exact("x").alias("value"),
            lit(99_i64).alias("__apply_row_number"),lit(88_i64).alias("__apply_order_ordinal")]).unwrap()
        .alias("visible").unwrap().build().unwrap();
    let result=partitioned_limit(input.clone(),&["g".into()],1,Some(1)).unwrap();
    let expected_names=input.schema().fields().iter().map(|f|f.name().clone()).collect::<Vec<_>>();
    assert_eq!(result.schema().fields().iter().map(|f|f.name().clone()).collect::<Vec<_>>(),expected_names);
    let plan=LogicalPlanBuilder::from(result).sort(vec![col_exact("g").sort(true,false)]).unwrap().build().unwrap();
    let batches=crate::ir::rel::execute_test_plan(plan).await.unwrap();
    let rows=batches.iter().flat_map(|b|(0..b.num_rows()).map(|r|(0..b.num_columns()).map(|c|
        ScalarValue::try_from_array(b.column(c),r).unwrap()).collect::<Vec<_>>())).collect::<Vec<_>>();
    assert_eq!(rows,vec![vec![ScalarValue::Int64(Some(0)),ScalarValue::Int64(Some(3)),ScalarValue::Int64(Some(99)),ScalarValue::Int64(Some(88))],
        vec![ScalarValue::Int64(Some(1)),ScalarValue::Int64(Some(4)),ScalarValue::Int64(Some(99)),ScalarValue::Int64(Some(88))]]);
}

#[tokio::test]
async fn partitioned_limit_handles_identity_empty_and_unbounded_slices() {
    let input=LogicalPlanBuilder::values((1..=4_i64).map(|n|vec![lit(n)]).collect()).unwrap()
        .sort(vec![col_exact("column1").sort(true,false)]).unwrap().build().unwrap();
    assert_eq!(partitioned_limit(input.clone(),&[],0,None).unwrap(),input);
    for (offset,fetch,expected) in [(1,Some(2),vec![2,3]),(2,None,vec![3,4]),(0,Some(0),vec![]),(9,Some(1),vec![])] {
        let result=partitioned_limit(input.clone(),&[],offset,fetch).unwrap();
        let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
        let actual=batches.iter().flat_map(|b|b.column(0).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
        assert_eq!(actual,expected,"offset={offset}, fetch={fetch:?}");
    }
    let grouped=LogicalPlanBuilder::values((1..=6_i64).map(|n|vec![lit(n%2),lit(n)]).collect()).unwrap()
        .sort(vec![col_exact("column2").sort(true,false)]).unwrap().build().unwrap();
    let result=partitioned_limit(grouped,&["column1".into()],1,None).unwrap();
    let result=LogicalPlanBuilder::from(result).sort(vec![col_exact("column2").sort(true,false)]).unwrap().build().unwrap();
    let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
    let values=batches.iter().flat_map(|b|b.column(1).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
    assert_eq!(values,vec![3,4,5,6]);
}
