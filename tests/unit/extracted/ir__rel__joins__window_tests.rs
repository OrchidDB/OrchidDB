
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
