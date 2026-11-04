
    use super::*;
    use datafusion::common::{Constraint, Constraints};

    fn table(name: &str, values: Vec<Option<i64>>, primary: bool) -> LogicalPlan {
        let nullable = values.iter().any(Option::is_none);
        let schema = Arc::new(Schema::new(vec![Field::new(
            "x",
            DataType::Int64,
            nullable,
        )]));
        let batch =
            RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(values))]).unwrap();
        let constraints = Constraints::new_unverified(vec![if primary {
            Constraint::PrimaryKey(vec![0])
        } else {
            Constraint::Unique(vec![0])
        }]);
        let provider = MemTable::try_new(schema, vec![vec![batch]])
            .unwrap()
            .with_constraints(constraints);
        LogicalPlanBuilder::scan(name, provider_as_source(Arc::new(provider)), None)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn nullable_unique_does_not_prove_scalar_cardinality() {
        let nullable = table("nullable", vec![None, None], false);
        assert!(!unique_on(&nullable, &[col_exact("x")]));
        let guarded =
            apply::guard_scalar_cardinality(LoweredNode::new(nullable), &["x".into()]).unwrap();
        assert!(
            guarded
                .plan
                .display_indent()
                .to_string()
                .contains("WindowAggr")
        );
        let keyed = table("keyed", vec![Some(1), Some(2)], true);
        assert!(unique_on(&keyed, &[col_exact("x")]));
        let original = keyed.clone();
        let guarded =
            apply::guard_scalar_cardinality(LoweredNode::new(keyed), &["x".into()]).unwrap();
        assert_eq!(guarded.plan, original);
        assert!(!unique_on(&original, &[]));
    }

    #[tokio::test]
    async fn semi_anti_rule_preserves_left_duplicates() {
        for (kind, expected) in [(JoinType::LeftSemi, 2), (JoinType::LeftAnti, 1)] {
            let left = LogicalPlanBuilder::values(vec![
                vec![lit(1_i64)],
                vec![lit(1_i64)],
                vec![lit(2_i64)],
            ])
            .unwrap()
            .alias("l")
            .unwrap()
            .build()
            .unwrap();
            let right = LogicalPlanBuilder::values(vec![vec![lit(1_i64)], vec![lit(1_i64)]])
                .unwrap()
                .alias("r")
                .unwrap()
                .distinct()
                .unwrap()
                .build()
                .unwrap();
            let condition = Expr::Column(left.schema().columns()[0].clone())
                .eq(Expr::Column(right.schema().columns()[0].clone()));
            let joined = LogicalPlanBuilder::from(left)
                .join_on(right, kind, vec![condition])
                .unwrap()
                .build()
                .unwrap();
            let optimized = simplify_existence(joined.clone()).unwrap();
            assert!(!optimized.display_indent().to_string().contains("Distinct"));
            for plan in [joined, optimized] {
                let result = crate::ir::rel::execute_test_plan(plan).await.unwrap();
                assert_eq!(
                    result.iter().map(|batch| batch.num_rows()).sum::<usize>(),
                    expected
                );
            }
        }
    }
