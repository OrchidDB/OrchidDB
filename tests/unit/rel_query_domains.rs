use super::*;
use datafusion::logical_expr::col;

fn projected(value: Expr, name: &str) -> LogicalPlan {
    LogicalPlanBuilder::empty(true)
        .project(vec![value.alias(name)])
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn null_empty_and_unknown_domains_stay_distinct_through_unions() {
    let known = projected(lit("Person"), "kind");
    let null = projected(lit(ScalarValue::Utf8(None)), "kind");
    let expected = Some(BTreeSet::from(["Person".into()]));
    assert_eq!(strings(&known, "kind"), expected);
    assert_eq!(strings(&null, "kind"), Some(BTreeSet::new()));
    assert_eq!(strings(&known, "missing"), None);
    let union = LogicalPlanBuilder::from(known.clone())
        .union(null)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(strings(&union, "kind"), expected);
    let unknown = projected(lit(7_i64), "kind");
    assert_eq!(strings(&unknown, "kind"), None);
    let empty = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
        produce_one_row: false,
        schema: known.schema().clone(),
    });
    assert_eq!(strings(&empty, "kind"), Some(BTreeSet::new()));
    let opaque = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
        produce_one_row: true,
        schema: known.schema().clone(),
    });
    let union = LogicalPlanBuilder::from(known)
        .union(opaque)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(strings(&union, "kind"), None);
}

#[test]
fn discriminator_domains_follow_aliases_filters_sorts_limits_and_both_join_sides() {
    let left = LogicalPlanBuilder::from(projected(lit("Person"), "kind"))
        .alias("l")
        .unwrap()
        .filter(lit(true))
        .unwrap()
        .sort(vec![col("l.kind").sort(true, false)])
        .unwrap()
        .limit(0, Some(1))
        .unwrap()
        .build()
        .unwrap();
    let right = projected(lit("Company"), "other");
    for kind in [
        JoinType::Inner,
        JoinType::Left,
        JoinType::Right,
        JoinType::Full,
    ] {
        let joined = LogicalPlanBuilder::from(left.clone())
            .join(
                right.clone(),
                kind,
                (Vec::<Column>::new(), Vec::<Column>::new()),
                Some(lit(true)),
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            strings(&joined, "kind"),
            Some(BTreeSet::from(["Person".into()]))
        );
        assert_eq!(
            strings(&joined, "other"),
            Some(BTreeSet::from(["Company".into()]))
        );
    }
    let projected = LogicalPlanBuilder::from(left)
        .project(vec![col("l.kind").alias("renamed")])
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        strings(&projected, "renamed"),
        Some(BTreeSet::from(["Person".into()]))
    );
}
