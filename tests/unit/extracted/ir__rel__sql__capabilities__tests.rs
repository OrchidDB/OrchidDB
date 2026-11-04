use super::*;
use datafusion::logical_expr::LogicalPlanBuilder;
#[test]
fn correlated_limit_is_a_shape_requirement_not_a_subquery_switch() {
    let mut req = SubqueryRequirements {
        kind: SubqueryKind::Scalar,
        correlated: true,
        unnest: true,
        aggregate: true,
        window: false,
        correlated_limit: false,
        output_types: vec![DataType::Int64],
    };
    assert!(SqlDialect::DuckDb.check_subquery(&req).is_ok());
    req.correlated_limit = true;
    assert!(SqlDialect::DuckDb.check_subquery(&req).is_err());
    assert!(SqlDialect::Postgres.check_subquery(&req).is_err());
}

#[test]
fn scalar_membership_and_exists_have_distinct_output_arity_contracts() {
    for kind in [
        SubqueryKind::Scalar,
        SubqueryKind::Exists,
        SubqueryKind::In,
        SubqueryKind::SetComparison,
    ] {
        for columns in 0..=2 {
            let requirements = SubqueryRequirements {
                kind,
                correlated: false,
                unnest: false,
                aggregate: false,
                window: false,
                correlated_limit: false,
                output_types: vec![DataType::Int64; columns],
            };
            for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
                assert_eq!(
                    dialect.check_subquery(&requirements).is_ok(),
                    kind == SubqueryKind::Exists || columns == 1,
                    "{dialect:?}, {kind:?}, {columns}"
                );
            }
        }
    }
    assert!(SubqueryRequirements::from_expr(&lit(1_i64)).is_none());
}

#[test]
fn requirement_extraction_finds_outer_references_inside_limit_expressions() {
    use datafusion::logical_expr::{
        Limit, Subquery,
        expr::{Exists, InSubquery, SetComparison, SetQuantifier},
    };
    let input = LogicalPlanBuilder::empty(true)
        .project(vec![lit(7_i64).alias("v")])
        .unwrap()
        .build()
        .unwrap();
    let reference = Expr::OuterReferenceColumn(
        input.schema().field(0).clone(),
        Column::new_unqualified("outer_limit"),
    );
    for skip in [false, true] {
        let limit = LogicalPlan::Limit(Limit {
            input: Arc::new(input.clone()),
            skip: skip.then(|| Box::new(reference.clone() + lit(1_i64))),
            fetch: (!skip).then(|| Box::new(reference.clone() + lit(1_i64))),
        });
        let subquery = Subquery {
            subquery: Arc::new(limit),
            outer_ref_columns: vec![reference.clone()],
            spans: Default::default(),
        };
        for (expression, kind) in [
            (Expr::ScalarSubquery(subquery.clone()), SubqueryKind::Scalar),
            (
                Expr::Exists(Exists::new(subquery.clone(), false)),
                SubqueryKind::Exists,
            ),
            (
                Expr::InSubquery(InSubquery::new(
                    Box::new(lit(7_i64)),
                    subquery.clone(),
                    false,
                )),
                SubqueryKind::In,
            ),
            (
                Expr::SetComparison(SetComparison::new(
                    Box::new(lit(7_i64)),
                    subquery,
                    datafusion::logical_expr::Operator::Eq,
                    SetQuantifier::Any,
                )),
                SubqueryKind::SetComparison,
            ),
        ] {
            let requirements = SubqueryRequirements::from_expr(&expression).unwrap();
            assert_eq!(requirements.kind, kind);
            assert!(requirements.correlated && requirements.correlated_limit);
            assert!(!requirements.unnest && !requirements.aggregate && !requirements.window);
            assert_eq!(requirements.output_types, vec![DataType::Int64]);
            for dialect in [
                SqlDialect::DuckDb,
                SqlDialect::Postgres,
                SqlDialect::Custom(&super::super::trino::TRINO),
            ] {
                assert!(dialect.check_subquery(&requirements).is_err());
            }
        }
    }
}
