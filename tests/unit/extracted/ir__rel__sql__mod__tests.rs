
    use super::*;

    #[test]
    fn quotes_and_escapes_identifiers() {
        assert_eq!(SqlDialect::DuckDb.quote_ident("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn renders_literals() {
        assert_eq!(
            sql_literal(
                SqlDialect::DuckDb,
                &ScalarValue::Utf8(Some("o'hare".into()))
            )
            .unwrap(),
            "'o''hare'"
        );
        assert_eq!(
            sql_literal(SqlDialect::DuckDb, &ScalarValue::Float64(Some(2.0))).unwrap(),
            "2.0"
        );
        assert_eq!(
            sql_literal(SqlDialect::Postgres, &ScalarValue::Float64(Some(f64::NAN))).unwrap(),
            "CAST('NaN' AS DOUBLE PRECISION)"
        );
        assert_eq!(
            sql_literal(SqlDialect::DuckDb, &ScalarValue::Int64(None)).unwrap(),
            "NULL"
        );
        let nul = sql_literal(SqlDialect::DuckDb, &ScalarValue::Utf8(Some("a\0b".into()))).unwrap();
        assert_eq!(nul, "'a' || chr(0) || 'b'");
        assert!(!nul.contains('\0'));
    }

    #[test]
    fn rejects_unsupported_ddl_types() {
        let err = SqlDialect::Postgres
            .ddl_type(&DataType::Duration(arrow::datatypes::TimeUnit::Microsecond))
            .unwrap_err();
        assert!(matches!(err, SqlError::Unsupported(_)));
    }

    #[test]
    fn duckdb_fixups_alias_unnest_outputs() {
        let sql = SqlDialect::DuckDb.fixup_query(
            "SELECT UNNEST(\"items\"), UNNEST(\"items\") AS \"copy\", array_min(\"items\"), array_intersect(\"items\", \"other\")".into(),
        );
        assert_eq!(
            sql,
            "SELECT UNNEST(\"items\") AS \"items\", UNNEST(\"items\") AS \"copy\", array_min(\"items\"), array_intersect(\"items\", \"other\")"
        );
    }

#[test]
fn sql_row_reconstruction_preserves_wide_strings_and_rejects_type_mismatches() {
    for ty in [DataType::Utf8,DataType::LargeUtf8,DataType::Utf8View] {
        let field=Field::new("value",ty.clone(),true);
        let values=vec![vec![SqlValue::Text("雪\0'".into())],vec![SqlValue::Null],vec![SqlValue::Text("".into())]];
        let array=rows::build_column(&field,&values,0).unwrap();
        assert_eq!(array.data_type(),&ty);
        assert!(array.is_null(1));
        assert_eq!(arrow::util::display::array_value_to_string(array.as_ref(),0).unwrap(),"雪\0'");
        assert_eq!(arrow::util::display::array_value_to_string(array.as_ref(),2).unwrap(),"");
    }
    for (ty,value) in [(DataType::Int8,SqlValue::Text("7".into())),
        (DataType::UInt64,SqlValue::Bool(true)),(DataType::Float32,SqlValue::Binary(vec![1])),
        (DataType::Decimal128(10,2),SqlValue::Float(1.25))] {
        let error=rows::build_column(&Field::new("value",ty,true),&[vec![value]],0).unwrap_err();
        assert!(error.to_string().contains("value"),"{error}");
    }
    assert!(rows::build_column(&Field::new("value",DataType::Duration(arrow::datatypes::TimeUnit::Second),true),&[vec![SqlValue::Int(1)]],0).unwrap_err().to_string().contains("result column"));
    assert!(rows::parse_decimal_i128("12",-1).is_err());
}
