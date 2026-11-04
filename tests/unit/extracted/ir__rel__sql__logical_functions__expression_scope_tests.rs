
    use super::*;

    fn list_literal(value: i64) -> (Expr, String) {
        let scalar = ScalarValue::List(ScalarValue::new_list_nullable(
            &[ScalarValue::Int64(Some(value))], &DataType::Int64,
        ));
        let expr = super::super::structured::literal_expr(&scalar, SqlDialect::DuckDb).unwrap();
        let Expr::ScalarFunction(call) = &expr else { panic!("logical literal expected") };
        let name = call.func.name().to_owned();
        (expr, name)
    }

    #[test]
    fn nested_lambda_literal_mappings_restore_outer_scope() {
        let schema = datafusion::common::DFSchema::empty();
        let (outer, outer_name) = list_literal(71);
        let (inner, inner_name) = list_literal(93);
        assert!(mapping(&outer_name, 0).is_none());
        with_expression(&outer, &schema, SqlDialect::DuckDb, || {
            let expected = mapping(&outer_name, 0).unwrap();
            with_expression(&inner, &schema, SqlDialect::DuckDb, || {
                assert_eq!(mapping(&outer_name, 0), Some(expected.clone()));
                assert!(mapping(&inner_name, 0).is_some());
                Ok(())
            })?;
            assert_eq!(mapping(&outer_name, 0), Some(expected));
            assert!(mapping(&inner_name, 0).is_none());
            Ok(())
        }).unwrap();
        assert!(mapping(&outer_name, 0).is_none());
    }

    #[test]
    fn failed_lambda_emission_does_not_leak_literal_mappings() {
        let schema = datafusion::common::DFSchema::empty();
        let (expr, name) = list_literal(59);
        let error = with_expression(&expr, &schema, SqlDialect::DuckDb, || {
            assert!(mapping(&name, 0).is_some());
            Err::<(), _>(SqlError::Unsupported("failed body".into()))
        }).unwrap_err();
        assert!(error.to_string().contains("failed body"));
        assert!(mapping(&name, 0).is_none());
    }
