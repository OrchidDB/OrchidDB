use super::*;
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::common::DFSchema;
use datafusion::logical_expr::{Cast, TryCast, Case};
use datafusion::prelude::{col, lit};

#[test]
fn access_reordering_treats_throwing_casts_as_boundaries_but_allows_identity_casts() {
    let schema = DFSchema::try_from(Schema::new(vec![
        Field::new("number",DataType::Int64,true),Field::new("text",DataType::Utf8,true)
    ])).unwrap();
    let cast = |expr,ty| Expr::Cast(Cast::new(Box::new(expr),ty));
    assert!(pure_in_schema(&cast(col("number"),DataType::Int64),&schema));
    assert!(pure_in_schema(&cast(col("text"),DataType::Utf8),&schema));
    assert!(pure_in_schema(&cast(col("number"),DataType::Utf8),&schema));
    assert!(!pure_in_schema(&cast(col("text"),DataType::Int64),&schema));
    assert!(!pure_in_schema(&cast(col("number"),DataType::Int8),&schema));
    assert!(!pure_in_schema(&cast(col("missing"),DataType::Int64),&schema));
    assert!(pure_in_schema(&Expr::TryCast(TryCast::new(Box::new(col("text")),DataType::Int64)),&schema));
    let throwing = col("number") / lit(0i64);
    assert!(!pure_in_schema(&throwing,&schema));
    for wrapped in [throwing.clone().alias("n"),throwing.clone().is_null(),throwing.clone().is_not_null(),Expr::Not(Box::new(throwing.clone())),col("number").in_list(vec![throwing.clone()],false),
        Expr::Case(Case::new(None,vec![(Box::new(lit(true)),Box::new(throwing.clone()))],Some(Box::new(lit(1i64))))),
        Expr::Case(Case::new(Some(Box::new(throwing.clone())),vec![(Box::new(lit(1i64)),Box::new(lit(2i64)))],None)),
        Expr::Case(Case::new(None,vec![(Box::new(lit(true)),Box::new(lit(1i64)))],Some(Box::new(throwing))))] {
        assert!(!pure_in_schema(&wrapped,&schema),"{wrapped}");
    }
}

#[test]
fn encoding_is_reorderable_only_with_known_total_format_and_arguments() {
    use datafusion::functions::encoding::expr_fn::encode;
    for format in ["hex","base64"] {
        assert!(pure_expr(&encode(col("value"),lit(format))));
        assert!(!pure_expr(&encode(col("number") / lit(0i64),lit(format))));
    }
    assert!(!pure_expr(&encode(col("value"),lit("unsupported"))));
    assert!(!pure_expr(&encode(col("value"),col("format"))));
    assert!(pure_expr(&Expr::Case(Case::new(None,vec![(Box::new(col("a").eq(lit(1i64))),Box::new(lit("yes")))],Some(Box::new(lit("no")))))));
}

fn access_scan(name: &str, collection: bool) -> LogicalPlan {
    use std::sync::Arc;
    use datafusion::datasource::{empty::EmptyTable, provider_as_source};
    let mut fields = vec![Field::new("id", DataType::Int64, false)];
    if collection {
        fields.push(Field::new("items", DataType::List(Arc::new(Field::new("item", DataType::Int64, true))), true));
    }
    LogicalPlanBuilder::scan(name, provider_as_source(Arc::new(EmptyTable::new(Arc::new(Schema::new(fields))))), None)
        .unwrap().build().unwrap()
}

#[test]
fn flatten_translates_projected_join_aliases_and_nested_predicates_to_original_sources() {
    let pair = LogicalPlanBuilder::from(access_scan("a", false))
        .join_on(access_scan("b", false), JoinType::Inner, vec![
            col("a.id").eq(col("b.id")).and(col("a.id").gt(lit(0i64)).and(col("b.id").lt(lit(9i64))))
        ]).unwrap()
        .project(vec![col("a.id").alias("left_id"), col("b.id").alias("right_id")]).unwrap()
        .alias("pair").unwrap().build().unwrap();
    let plan = LogicalPlanBuilder::from(pair)
        .join_on(access_scan("c", false), JoinType::Inner, vec![col("pair.right_id").eq(col("c.id"))])
        .unwrap().build().unwrap();
    let mut leaves = vec![];
    let mut predicates = vec![];
    let bindings = flatten(&plan, &mut leaves, &mut predicates).unwrap();
    assert_eq!(leaves.len(), 3);
    assert!(leaves.iter().all(|p| matches!(p, LogicalPlan::TableScan(_))));
    assert_eq!(predicates.len(), 4);
    assert_eq!(bindings[&datafusion::common::Column::from_qualified_name("pair.left_id")], col("a.id"));
    assert_eq!(bindings[&datafusion::common::Column::from_qualified_name("pair.right_id")], col("b.id"));
    assert!(predicates.contains(&col("b.id").eq(col("c.id"))));
    assert!(predicates.contains(&col("a.id").gt(lit(0i64))));
    assert!(predicates.contains(&col("b.id").lt(lit(9i64))));
    assert!(predicates.iter().all(|p| p.column_refs().iter().all(|c| c.relation.as_ref().is_none_or(|r| r.to_string() != "pair"))));
}

#[test]
fn collection_restriction_passes_parent_keys_through_distinct_but_preserves_expansion() {
    let expanded = LogicalPlanBuilder::from(access_scan("parent", true))
        .unnest_column(datafusion::common::Column::from_qualified_name("parent.items")).unwrap().build().unwrap();
    let distinct = LogicalPlanBuilder::from(expanded)
        .aggregate(vec![col("parent.id"), col("items")], Vec::<Expr>::new()).unwrap().build().unwrap();
    let right = access_scan("neighbor", false);
    let restricted = parent_restriction(&distinct, &[col("parent.id")], &right, &[col("neighbor.id")], false)
        .unwrap().expect("parent key restriction can move beneath expansion");
    assert_eq!(restricted.schema(), distinct.schema());
    let LogicalPlan::Aggregate(group) = restricted else { panic!("DISTINCT must remain above expansion") };
    assert_eq!(group.group_expr, vec![col("parent.id"), col("items")]);
    let LogicalPlan::Unnest(unnest) = group.input.as_ref() else { panic!("expansion must remain") };
    let LogicalPlan::Join(join) = unnest.input.as_ref() else { panic!("restrict before expansion") };
    assert_eq!(join.join_type, JoinType::LeftSemi);
    assert_eq!(join.left.schema(), access_scan("parent", true).schema());
    assert_eq!(join.right.as_ref(), &right);
    assert!(parent_restriction(&distinct, &[col("items")], &right, &[col("neighbor.id")], false).unwrap().is_none());
    assert!(parent_restriction(&distinct, &[col("parent.id") + lit(1i64)], &right, &[col("neighbor.id")], false).unwrap().is_none());
    assert!(parent_restriction(&distinct, &[col("absent")], &right, &[col("neighbor.id")], false).unwrap().is_none());
}

#[test]
fn collection_restriction_rejects_computed_grouping_keys() {
    let expanded = LogicalPlanBuilder::from(access_scan("parent", true))
        .unnest_column(datafusion::common::Column::from_qualified_name("parent.items")).unwrap().build().unwrap();
    let grouped = LogicalPlanBuilder::from(expanded)
        .aggregate(vec![col("parent.id") + lit(1i64)], Vec::<Expr>::new()).unwrap().build().unwrap();
    let key = Expr::Column(grouped.schema().columns()[0].clone());
    assert!(parent_restriction(&grouped, &[key], &access_scan("neighbor", false), &[col("neighbor.id")], false).unwrap().is_none());
}
