
    use super::*;
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
