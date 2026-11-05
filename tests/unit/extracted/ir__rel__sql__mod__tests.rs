
    use super::*;

#[test]
fn sql_decimal_rows_preserve_negative_scale_and_reject_lossy_values() {
    let field=Field::new("amount",DataType::Decimal128(10,-2),true);
    for (input,expected) in [(SqlValue::Int(12300),123),(SqlValue::Int(-12300),-123),
        (SqlValue::ExactNumber("12300".into()),123),(SqlValue::ExactNumber("-12300.00".into()),-123)] {
        let array=rows::build_column(&field,&[vec![input],vec![SqlValue::Null]],0).unwrap();
        assert_eq!(ScalarValue::try_from_array(&array,0).unwrap(),ScalarValue::Decimal128(Some(expected),10,-2));
        assert!(array.is_null(1));
    }
    for input in [SqlValue::Int(12301),SqlValue::ExactNumber("12300.1".into())] {
        assert!(rows::build_column(&field,&[vec![input]],0).is_err());
    }
}

#[test]
fn sql_decimal_parser_rejects_malformed_unicode_without_panicking() {
    for input in ["", ".", "-", "-.","1.0雪","1.雪0","1.00.0","1e3"," 1","1 "] {
        for scale in [-2,0,1,2,3] {
            let result=std::panic::catch_unwind(||rows::parse_decimal_i128(input,scale));
            assert!(result.is_ok(),"decimal parser panicked for {input:?} at scale {scale}");
            assert!(result.unwrap().is_err(),"malformed decimal accepted: {input:?}");
        }
    }
    for (input,scale,expected) in [("12.30",2,1230),("-0.25",2,-25),(".5",1,5),
        ("0",-38,0),("100",-2,1),("-100.00",-2,-1),("+12",0,12)] {
        assert_eq!(rows::parse_decimal_i128(input,scale),Ok(expected));
    }
}

#[test]
fn sql_structured_rows_accept_typed_and_json_carriers_and_preserve_empty_schema() {
    let ty=DataType::Struct(vec![Field::new("n",DataType::Int64,true),Field::new("text",DataType::Utf8,true)].into());
    let expected=structured::decode(&serde_json::json!({"n":"7","text":"雪"}),&ty).unwrap();
    let json=crate::ir::functions::domain::json_scalar("{\"n\":\"7\",\"text\":\"雪\"}").unwrap();
    for input in [SqlValue::Domain(expected.clone()),SqlValue::Domain(json),SqlValue::Text("{\"n\":\"7\",\"text\":\"雪\"}".into())] {
        let array=rows::build_column(&Field::new("record",ty.clone(),true),&[vec![input],vec![SqlValue::Null]],0).unwrap();
        assert_eq!(ScalarValue::try_from_array(&array,0).unwrap(),expected);assert!(array.is_null(1));
    }
    let array=rows::build_column(&Field::new("record",ty.clone(),true),&[],0).unwrap();
    assert!(array.is_empty());assert_eq!(array.data_type(),&ty);
    for input in [SqlValue::Text("invalid".into()),SqlValue::Bool(true)] {
        assert!(rows::build_column(&Field::new("record",ty.clone(),true),&[vec![input]],0).is_err());
    }
}

#[test]
fn sql_decimal_rows_reject_precision_overflow_without_wrapping() {
    for (precision,scale,input) in [(2,0,SqlValue::Int(100)),(38,38,SqlValue::Int(i64::MAX)),
        (5,2,SqlValue::ExactNumber("1000.00".into())),(5,2,SqlValue::ExactNumber("1.001".into()))] {
        assert!(rows::build_column(&Field::new("amount",DataType::Decimal128(precision,scale),true),&[vec![input]],0).is_err());
    }
}

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
