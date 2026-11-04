
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
