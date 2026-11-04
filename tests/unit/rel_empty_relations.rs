use super::*;

fn relation(present: bool, name: &str, value: i64) -> LogicalPlan {
    LogicalPlanBuilder::empty(present)
        .project(vec![lit(value).alias(name)])
        .unwrap()
        .build()
        .unwrap()
}

#[tokio::test]
async fn unions_keep_all_surviving_occurrences_and_the_declared_schema() {
    for mask in 0..8 {
        let original = LogicalPlanBuilder::from(relation(mask & 1 != 0, "v", 1))
            .union(relation(mask & 2 != 0, "v", 2))
            .unwrap()
            .union(relation(mask & 4 != 0, "v", 3))
            .unwrap()
            .build()
            .unwrap();
        let result = simplify(original.clone()).unwrap();
        assert_eq!(result.schema(), original.schema());
        let batches = super::super::execute_test_plan(result).await.unwrap();
        let mut values = batches
            .iter()
            .flat_map(|batch| {
                (0..batch.num_rows()).map(|row| {
                    arrow::util::display::array_value_to_string(batch.column(0), row).unwrap()
                })
            })
            .collect::<Vec<_>>();
        values.sort();
        let expected = (0..3)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| (bit + 1).to_string())
            .collect::<Vec<_>>();
        assert_eq!(values, expected);
    }
}

#[tokio::test]
async fn empty_outer_join_sides_are_null_extended_in_the_original_column_order() {
    for left in [false, true] {
        let original = LogicalPlanBuilder::from(relation(left, "a", 7))
            .join(
                relation(!left, "b", 9),
                if left {
                    JoinType::Left
                } else {
                    JoinType::Right
                },
                (Vec::<Column>::new(), Vec::<Column>::new()),
                Some(lit(true)),
            )
            .unwrap()
            .build()
            .unwrap();
        let result = simplify(original.clone()).unwrap();
        assert_eq!(result.schema(), original.schema());
        assert!(matches!(result, LogicalPlan::Projection(_)));
        let batches = super::super::execute_test_plan(result).await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let batch = batches.iter().find(|batch| batch.num_rows() == 1).unwrap();
        assert_eq!(batch.column(usize::from(left)).null_count(), 1);
        let kept = usize::from(!left);
        assert_eq!(
            arrow::util::display::array_value_to_string(batch.column(kept), 0).unwrap(),
            if left { "7" } else { "9" }
        );
    }
}

#[test]
fn false_filter_does_not_erase_a_potentially_failing_projection() {
    let input = LogicalPlanBuilder::empty(true)
        .project(vec![
            Expr::Cast(Cast::new(Box::new(lit("invalid")), DataType::Int64)).alias("v"),
        ])
        .unwrap()
        .filter(lit(false))
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(simplify(input.clone()).unwrap(), input);
}
