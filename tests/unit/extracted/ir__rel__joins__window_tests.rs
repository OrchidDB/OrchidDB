
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

#[tokio::test]
async fn unordered_distinct_deduplicates_keys_and_preserves_qualified_columns() {
    let input=LogicalPlanBuilder::values(vec![
        vec![lit(1_i64),lit(10_i64)],vec![lit(1_i64),lit(10_i64)],
        vec![lit(ScalarValue::Int64(None)),lit(20_i64)],vec![lit(ScalarValue::Int64(None)),lit(20_i64)],
    ]).unwrap().project(vec![col_exact("column1").alias("key"),col_exact("column2").alias("value"),
        lit(99_i64).alias("__distinct_key_0")]).unwrap().alias("source").unwrap().build().unwrap();
    let key=Expr::Column(input.schema().columns()[0].clone());
    let result=unordered_distinct_partition(input,vec![key.clone(),key],700).unwrap();
    assert_eq!(result.schema().fields().iter().map(|f|f.name().as_str()).collect::<Vec<_>>(),vec!["key","value","__distinct_key_0"]);
    let result=LogicalPlanBuilder::from(result).sort(vec![col_exact("value").sort(true,false)]).unwrap().build().unwrap();
    let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
    let values=batches.iter().flat_map(|b|b.column(1).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
    assert_eq!(values,vec![10,20]);
    for batch in batches { assert!(batch.column(2).as_any().downcast_ref::<Int64Array>().unwrap().values().iter().all(|v|*v==99)); }
}

#[tokio::test]
async fn unordered_distinct_retains_stable_float_representatives() {
    let input=LogicalPlanBuilder::values(vec![vec![lit(-0.0_f64),lit(1_i64)],vec![lit(0.0_f64),lit(2_i64)]]).unwrap()
        .project(vec![col_exact("column1").alias("key"),col_exact("column2").alias("order")]).unwrap()
        .sort(vec![col_exact("order").sort(true,false)]).unwrap().build().unwrap();
    let result=unordered_distinct_partition(input,vec![col_exact("key")],701).unwrap();
    let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(),1);
    assert_eq!(batches[0].column(0).as_any().downcast_ref::<arrow::array::Float64Array>().unwrap().value(0).to_bits(),(-0.0_f64).to_bits());
    assert_eq!(ScalarValue::try_from_array(batches[0].column(1),0).unwrap(),ScalarValue::Int64(Some(1)));
}

#[tokio::test]
async fn unordered_distinct_without_keys_preserves_empty_input_cardinality() {
    for name in ["value","__distinct_input_count"] {
    for value in [Some(7_i64),None] {
    for produce in [false,true] {
        let input=LogicalPlanBuilder::empty(produce).project(vec![lit(ScalarValue::Int64(value)).alias(name)]).unwrap().build().unwrap();
        let result=unordered_distinct_partition(input,vec![],702).unwrap();
        assert_eq!(result.schema().fields().iter().map(|f|f.name().as_str()).collect::<Vec<_>>(),vec![name]);
        let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(),usize::from(produce),"produce={produce}");
        if produce { assert_eq!(ScalarValue::try_from_array(batches[0].column(0),0).unwrap(),ScalarValue::Int64(value)); }
    }
    }
    }
    for produce in [false,true] {
        let input=LogicalPlanBuilder::empty(produce).build().unwrap();
        let result=unordered_distinct_partition(input,vec![],704).unwrap();
        assert!(result.schema().fields().is_empty());
        let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(),usize::from(produce));
    }
}

#[tokio::test]
async fn cypher_ordered_distinct_preserves_first_rows_across_transparent_scopes() {
    for mask in 0_u8..16 {
        let rows = [(1_i64,30_i64),(1,10),(2,20),(2,40)];
        let mut input = LogicalPlanBuilder::values(rows.iter().map(|(k,v)|vec![lit(*k),lit(*v)]).collect()).unwrap()
            .project(vec![col_exact("column1").alias("key"),col_exact("column2").alias("value"),
                lit(101_i64).alias("__distinct_ordinal_7500"),lit(102_i64).alias("__distinct_rank_7500"),
                lit(103_i64).alias("__distinct_input_guard_7500"),lit(104_i64).alias("__distinct_guard_7500")]).unwrap()
            .sort(vec![col_exact("value").sort(false,false)]).unwrap().build().unwrap();
        if mask & 1 != 0 { input=LogicalPlanBuilder::from(input).alias("ordered_source").unwrap().build().unwrap(); }
        if mask & 2 != 0 {
            let mut columns=existing_columns_by_name(&input,&BTreeSet::new());
            columns[1]=(col_exact("value")+lit(0_i64)).alias("value");
            input=LogicalPlanBuilder::from(input).project(columns).unwrap().build().unwrap();
        }
        if mask & 4 != 0 { input=LogicalPlanBuilder::from(input).filter(col_exact("value").not_eq(lit(30_i64))).unwrap().build().unwrap(); }
        if mask & 8 != 0 { input=LogicalPlanBuilder::from(input).limit(1,Some(2)).unwrap().build().unwrap(); }
        let fields=input.schema().as_arrow().clone();
        let result=keyed_distinct(input,&["key".into()],7500).unwrap();
        assert_eq!(result.schema().as_arrow(),&fields,"mask={mask}");
        let batches=crate::ir::rel::execute_test_plan(result).await
            .unwrap_or_else(|error|panic!("mask={mask}: {error}"));
        let mut actual=Vec::new();
        for batch in batches {
            for row in 0..batch.num_rows() {
                actual.push((batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(row),
                    batch.column(1).as_any().downcast_ref::<Int64Array>().unwrap().value(row)));
                for (column,value) in [(2,101_i64),(3,102),(4,103),(5,104)] {
                    assert_eq!(ScalarValue::try_from_array(batch.column(column),row).unwrap(),ScalarValue::Int64(Some(value)));
                }
            }
        }
        let mut expected=rows.to_vec(); expected.sort_by_key(|(_,v)|std::cmp::Reverse(*v));
        if mask & 4 != 0 { expected.retain(|(_,v)|*v!=30); }
        if mask & 8 != 0 { expected=expected.into_iter().skip(1).take(2).collect(); }
        let mut seen=BTreeSet::new(); expected.retain(|(k,_)|seen.insert(*k));
        assert_eq!(actual,expected,"mask={mask}");
    }
}

#[tokio::test]
async fn cypher_ordered_distinct_keeps_label_qualified_identity_and_correlation_groups() {
    let rows=[(1_i64,"A",1_i64,10_i64),(1,"A",1,11),(1,"B",1,12),(1,"A",2,13),(1,"A",2,14)];
    let input=LogicalPlanBuilder::values(rows.iter().map(|(id,label,corr,value)|vec![lit(*id),lit(*label),lit(*corr),lit(*value)]).collect()).unwrap()
        .project(vec![col_exact("column1").alias(id_col("n")),col_exact("column2").alias(label_col("n")),
            col_exact("column3").alias("__apply_corr_key_0"),col_exact("column4").alias(prop_col("n","value"))]).unwrap()
        .sort(vec![col_exact(prop_col("n","value")).sort(true,false)]).unwrap().build().unwrap();
    for (keys,expected) in [(vec!["n".into()],vec![10_i64,12,13]),
        (vec!["missing".into()],vec![10_i64,13]),(vec![],vec![10_i64,11,12,13,14])] {
        let result=keyed_distinct(input.clone(),&keys,7501).unwrap();
        let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
        let actual=batches.iter().flat_map(|b|b.column(3).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
        assert_eq!(actual,expected,"keys={keys:?}");
    }
}

#[tokio::test]
async fn cypher_ordered_distinct_preserves_null_key_and_empty_slice_cardinality() {
    for empty in [false,true] {
        let input=LogicalPlanBuilder::values(vec![vec![lit(ScalarValue::Int64(None)),lit(1_i64)],
            vec![lit(ScalarValue::Int64(None)),lit(2_i64)],vec![lit(9_i64),lit(3_i64)]]).unwrap()
            .project(vec![col_exact("column1").alias("key"),col_exact("column2").alias("value")]).unwrap()
            .sort(vec![col_exact("value").sort(false,false)]).unwrap()
            .limit(0,if empty {Some(0)}else{None}).unwrap().build().unwrap();
        let result=keyed_distinct(input,&["key".into()],7502).unwrap();
        let batches=crate::ir::rel::execute_test_plan(result).await.unwrap();
        let actual=batches.iter().flat_map(|b|b.column(1).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
        assert_eq!(actual,if empty {vec![]}else{vec![3,2]});
    }
}

#[tokio::test]
async fn cypher_ordered_distinct_preserves_nested_slices_and_qualified_renames() {
    for alias in [false,true] {
    for rename in [false,true] {
        let mut input=LogicalPlanBuilder::values(vec![vec![lit(1_i64),lit(60_i64)],
            vec![lit(1_i64),lit(50_i64)],vec![lit(2_i64),lit(40_i64)],
            vec![lit(2_i64),lit(30_i64)],vec![lit(3_i64),lit(20_i64)],vec![lit(3_i64),lit(10_i64)]]).unwrap()
            .project(vec![col_exact("column1").alias("key"),col_exact("column2").alias("value")]).unwrap()
            .sort(vec![col_exact("value").sort(false,false)]).unwrap()
            .limit(1,Some(4)).unwrap()
            .filter(col_exact("value").not_eq(lit(40_i64))).unwrap().build().unwrap();
        if alias { input=LogicalPlanBuilder::from(input).alias("slice_input").unwrap().build().unwrap(); }
        if rename { input=LogicalPlanBuilder::from(input).project(vec![col_exact("key").alias("renamed_key"),
            col_exact("value").alias("renamed_value")]).unwrap().build().unwrap(); }
        input=LogicalPlanBuilder::from(input).limit(1,Some(2)).unwrap().build().unwrap();
        let key=if rename {"renamed_key"}else{"key"};
        let result=keyed_distinct(input,&[key.into()],7503).unwrap();
        let batches=crate::ir::rel::execute_test_plan(result).await
            .unwrap_or_else(|error|panic!("alias={alias}, rename={rename}: {error}"));
        let actual=batches.iter().flat_map(|batch|(0..batch.num_rows()).map(|row|(
            batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(row),
            batch.column(1).as_any().downcast_ref::<Int64Array>().unwrap().value(row))).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(actual,vec![(2,30),(3,20)],"alias={alias}, rename={rename}");
    }
    }
}
