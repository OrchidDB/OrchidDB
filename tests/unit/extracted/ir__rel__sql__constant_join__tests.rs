
    use super::*;
    use datafusion::logical_expr::LogicalPlanBuilder;
    use datafusion::prelude::col;

    fn input(name: &str, value: ScalarValue) -> LogicalPlan {
        LogicalPlanBuilder::empty(true).project(vec![lit(value).alias("label")]).unwrap()
            .alias(name).unwrap().build().unwrap()
    }
    fn joined(a: ScalarValue, b: ScalarValue, kind: JoinType) -> LogicalPlan {
        LogicalPlanBuilder::from(input("a", a)).join(input("b", b), kind,
            (vec!["a.label"], vec!["b.label"]), None).unwrap().build().unwrap()
    }
    #[test]
    fn only_equal_non_null_constants_remove_keys() {
        for kind in [JoinType::Inner, JoinType::Left, JoinType::Full, JoinType::LeftSemi, JoinType::LeftAnti] {
            for (a,b,expected) in [
                (ScalarValue::from("Person"), ScalarValue::from("Person"), 0),
                (ScalarValue::from("Person"), ScalarValue::from("Other"), 1),
                (ScalarValue::Utf8(None), ScalarValue::Utf8(None), 1),
            ] {
                let plan = joined(a,b,kind);
                let rewritten = rewrite(plan.clone()).unwrap();
                assert_eq!(plan.schema(), rewritten.schema());
                let LogicalPlan::Join(j) = rewritten else { panic!() };
                assert_eq!(j.on.len(), expected);
            }
        }
    }
    #[test]
    fn outer_join_null_extension_does_not_establish_constants() {
        let text = ScalarValue::from("Person");
        assert_eq!(constants(&joined(text.clone(), text.clone(), JoinType::Left)), vec![Some(text.clone()), None]);
        assert_eq!(constants(&joined(text.clone(), text.clone(), JoinType::Right)), vec![None, Some(text.clone())]);
        assert_eq!(constants(&joined(text.clone(), text, JoinType::Full)), vec![None, None]);
    }

    #[test]
    fn residual_label_equality_preserves_other_predicates() {
        let text = ScalarValue::from("Person");
        let LogicalPlan::Join(mut join) = joined(text.clone(), text, JoinType::Left) else { panic!() };
        join.on.clear();
        let residual = col("a.label").is_not_null();
        join.filter = Some(col("a.label").eq(col("b.label")).and(residual.clone()));
        let LogicalPlan::Join(rewritten) = rewrite(LogicalPlan::Join(join)).unwrap() else { panic!() };
        assert_eq!(rewritten.filter, Some(residual));
    }

#[tokio::test]
async fn constant_join_keys_preserve_multiplicity_empty_inputs_and_outer_nulls() {
    fn rows(name: &str, text: ScalarValue, empty: bool) -> LogicalPlan {
        let builder = LogicalPlanBuilder::values(vec![vec![lit(1_i64)], vec![lit(1_i64)], vec![lit(2_i64)]]).unwrap();
        let builder = if empty { builder.filter(lit(false)).unwrap() } else { builder };
        builder.project(vec![col("column1").alias("id"), lit(text).alias("label")]).unwrap()
            .alias(name).unwrap().build().unwrap()
    }
    async fn results(plan: LogicalPlan, context: &str) -> Vec<Vec<String>> {
        let mut rows = vec![];
        let sql = super::super::unparse_plan(plan.clone(), SqlDialect::DuckDb);
        for batch in crate::ir::rel::execute_test_plan(plan).await.unwrap_or_else(|error| panic!("{context}: {error}; SQL {sql:?}")) {
            for row in 0..batch.num_rows() {
                rows.push(batch.columns().iter().map(|array| if array.is_null(row) { "NULL".to_owned() }
                    else { arrow::util::display::array_value_to_string(array, row).unwrap() }).collect());
            }
        }
        rows.sort(); rows
    }
    for kind in [JoinType::Inner, JoinType::Left, JoinType::Right, JoinType::Full,
        JoinType::LeftSemi, JoinType::LeftAnti, JoinType::RightSemi, JoinType::RightAnti] {
        for (left_empty, right_empty) in [(false,false),(true,false),(false,true),(true,true)] {
            for text in [ScalarValue::from("Person"), ScalarValue::Utf8(None)] {
                let left = rows("a", text.clone(), left_empty);
                let right = rows("b", text.clone(), right_empty);
                let plan = LogicalPlanBuilder::from(left).join(right, kind,
                    (vec!["a.id", "a.label"], vec!["b.id", "b.label"]), None).unwrap().build().unwrap();
                let rewritten = rewrite(plan.clone()).unwrap();
                assert_eq!(rewritten.schema(), plan.schema());
                let LogicalPlan::Join(join) = &rewritten else { panic!() };
                assert_eq!(join.on.len(), if text.is_null() {2} else {1});
                let context = format!("{kind:?}, left empty {left_empty}, right empty {right_empty}, label {text:?}");
                let actual = results(rewritten, &format!("rewritten {context}")).await;
                let left_count = if left_empty {0} else {3};
                let right_count = if right_empty {0} else {3};
                let matched = !left_empty && !right_empty && !text.is_null();
                let expected_count = match kind {
                    JoinType::Inner => if matched {5} else {0},
                    JoinType::Left => if matched {5} else {left_count},
                    JoinType::Right => if matched {5} else {right_count},
                    JoinType::Full => if matched {5} else {left_count + right_count},
                    JoinType::LeftSemi => if matched {left_count} else {0},
                    JoinType::RightSemi => if matched {right_count} else {0},
                    JoinType::LeftAnti => if matched {0} else {left_count},
                    JoinType::RightAnti => if matched {0} else {right_count},
                    _ => unreachable!(),
                };
                assert_eq!(actual.len(), expected_count, "{context}");
                assert_eq!(actual, results(plan, &format!("original {context}")).await, "{context}");
            }
        }
    }
}

#[test]
fn constants_follow_semi_join_outputs_and_transparent_row_operators() {
    let text = ScalarValue::from("Person");
    for kind in [JoinType::LeftSemi, JoinType::LeftAnti, JoinType::RightSemi, JoinType::RightAnti] {
        let plan = joined(text.clone(), text.clone(), kind);
        assert_eq!(constants(&plan), vec![Some(text.clone())]);
        let name = plan.schema().columns()[0].clone();
        let plan = LogicalPlanBuilder::from(plan).filter(Expr::Column(name.clone()).is_not_null()).unwrap()
            .sort(vec![Expr::Column(name).sort(true,false)]).unwrap().limit(0,Some(1)).unwrap().build().unwrap();
        assert_eq!(constants(&plan), vec![Some(text.clone())]);
    }
}
