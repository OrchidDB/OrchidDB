
    use super::*;
    use datafusion::{
        logical_expr::{col, lit},
        optimizer::{OptimizerContext, optimize_projections::OptimizeProjections},
    };

    fn recurrence(distinct: bool) -> Result<LogicalPlan> {
        let seed = LogicalPlanBuilder::empty(true)
            .project(vec![
                lit(999_i64).alias("unused"),
                lit(0_i64).alias("step"),
                lit(1_i64).alias("a"),
                lit(2_i64).alias("b"),
                lit(3_i64).alias("c"),
                lit("unused payload").alias("payload"),
            ])?
            .build()?;
        let work = Arc::new(CteWorkTable::new(
            "r",
            Arc::new(seed.schema().as_arrow().clone()),
        ));
        let recursive = LogicalPlanBuilder::scan("r", provider_as_source(work), None)?
            .filter(col("step").lt(lit(3_i64)))?
            .project(vec![
                col("unused"),
                (col("step") + lit(1_i64)).alias("step"),
                col("b").alias("a"),
                col("c").alias("b"),
                (col("c") + lit(1_i64)).alias("c"),
                col("payload"),
            ])?
            .build()?;
        Ok(LogicalPlan::RecursiveQuery(RecursiveQuery {
            name: "r".into(),
            static_term: Arc::new(seed),
            recursive_term: Arc::new(recursive),
            is_distinct: distinct,
        }))
    }

    fn optimize(plan: LogicalPlan) -> Result<LogicalPlan> {
        RecursiveProjection {
            inner: Arc::new(OptimizeProjections::new()),
        }
        .rewrite(plan, &OptimizerContext::new())
        .map(|t| t.data)
    }

    fn check_worktables(plan: &LogicalPlan, width: usize) -> Result<()> {
        let mut found = false;
        plan.apply_with_subqueries(|node| {
            if let LogicalPlan::TableScan(scan) = node {
                if let Ok(provider) = source_as_provider(&scan.source) {
                    if let Some(work) = provider.as_any().downcast_ref::<CteWorkTable>() {
                        found = true;
                        assert_eq!(work.schema().fields().len(), width, "{plan:?}");
                        assert!(scan.projection.as_ref().unwrap().iter().all(|i| *i < width));
                    }
                }
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        assert!(found);
        Ok(())
    }

    #[tokio::test]
    async fn recursive_projection_remaps_indices_and_closes_iteration_dependencies() -> Result<()> {
        let plan = LogicalPlanBuilder::from(recurrence(false)?)
            .project(vec![col("a")])?
            .build()?;
        let optimized = optimize(plan)?;
        check_worktables(&optimized, 4)?;
        assert_eq!(optimize(optimized.clone())?, optimized);
        let batches = crate::ir::rel::execute_test_plan(optimized).await?;
        let values = batches
            .iter()
            .flat_map(|b| {
                b.column(0)
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .unwrap()
                    .values()
                    .iter()
                    .copied()
            })
            .collect::<Vec<_>>();
        assert_eq!(values, vec![1, 2, 3, 4]);
        Ok(())
    }

    #[tokio::test]
    async fn zero_column_recursive_output_preserves_iteration_count() -> Result<()> {
        let plan = project(recurrence(false)?, &[])?;
        let optimized = optimize(plan)?;
        check_worktables(&optimized, 1)?;
        let batches = crate::ir::rel::execute_test_plan(optimized).await?;
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn recursive_distinct_retains_full_row_identity() -> Result<()> {
        let plan = LogicalPlanBuilder::from(recurrence(true)?)
            .project(vec![col("unused")])?
            .build()?;
        let optimized = optimize(plan)?;
        check_worktables(&optimized, 6)?;
        let batches = crate::ir::rel::execute_test_plan(optimized).await?;
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn shared_recursive_consumers_keep_one_worktable_layout() -> Result<()> {
        let query = recurrence(false)?;
        let left = LogicalPlanBuilder::from(query.clone())
            .project(vec![col("a")])?
            .build()?;
        let right = LogicalPlanBuilder::from(query)
            .project(vec![col("step").alias("a")])?
            .build()?;
        let optimized = optimize(LogicalPlanBuilder::from(left).union(right)?.build()?)?;
        check_worktables(&optimized, 4)?;
        let mut definitions = vec![];
        optimized.apply_with_subqueries(|node| {
            if let LogicalPlan::RecursiveQuery(query) = node {
                definitions.push(query.clone());
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0], definitions[1]);
        let batches = crate::ir::rel::execute_test_plan(optimized).await?;
        let mut values = batches
            .iter()
            .flat_map(|b| {
                b.column(0)
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .unwrap()
                    .values()
                    .iter()
                    .copied()
            })
            .collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, vec![0, 1, 1, 2, 2, 3, 3, 4]);
        Ok(())
    }
