
    use super::*;
    #[test]
    fn separately_lowered_structured_constants_keep_semantic_function_identity() {
        let value = ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(1))], &DataType::Int64);
        let value = ScalarValue::List(value);
        let first = literal_expr(&value, SqlDialect::DuckDb).unwrap();
        let second = literal_expr(&value, SqlDialect::DuckDb).unwrap();
        assert_eq!(first, second);
        let different = ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(2))], &DataType::Int64));
        assert_ne!(first, literal_expr(&different, SqlDialect::DuckDb).unwrap());
    }
