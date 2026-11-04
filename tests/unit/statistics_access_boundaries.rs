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
