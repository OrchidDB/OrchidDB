
    use super::*;

#[test]
fn decimal_transport_keeps_all_supported_scales_exact_at_precision_limit() {
    let digits="9".repeat(38);let magnitude=10_i128.pow(38)-1;
    for scale in 0..=38_i8 {
        let split=38-scale as usize;
        let text=if scale==0 {digits.clone()} else {format!("{}.{}",if split==0 {"0"}else{&digits[..split]},&digits[split..])};
        let field=Field::new("amount",DataType::Decimal128(38,scale),true);
        for (sign,expected) in [("",magnitude),("-",-magnitude),("+",magnitude)] {
            let array=rows::build_column(&field,&[vec![SqlValue::ExactNumber(format!("{sign}{text}"))]],0).unwrap();
            assert_eq!(ScalarValue::try_from_array(&array,0).unwrap(),ScalarValue::Decimal128(Some(expected),38,scale));
        }
    }
    assert_eq!(rows::parse_decimal_i128(&i128::MIN.to_string(),0),Ok(i128::MIN));
    assert_eq!(rows::parse_decimal_i128(&i128::MAX.to_string(),0),Ok(i128::MAX));
    assert!(rows::parse_decimal_i128("170141183460469231731687303715884105728",0).is_err());
}

#[test]
fn nominal_json_and_integer_row_transport_keep_null_and_precision_boundaries() {
    use crate::ir::functions::domain;
    let json_type=domain::json_type();
    let array=rows::build_column(&Field::new("json",json_type.clone(),true),
        &[vec![SqlValue::Text("null".into())],vec![SqlValue::Null],vec![SqlValue::Domain(domain::json_scalar("[1,null]").unwrap())]],0).unwrap();
    assert!(!array.is_null(0));assert!(array.is_null(1));
    assert_eq!(domain::json_text(&ScalarValue::try_from_array(&array,0).unwrap()).unwrap(),Some("null".into()));
    assert_eq!(domain::json_text(&ScalarValue::try_from_array(&array,2).unwrap()).unwrap(),Some("[1,null]".into()));
    let integer_type=domain::data_type("cypher_integer",DataType::Decimal128(38,0));
    let value=9007199254740993_i128;
    let array=rows::build_column(&Field::new("integer",integer_type.clone(),true),
        &[vec![SqlValue::ExactNumber(value.to_string())],vec![SqlValue::Null]],0).unwrap();
    assert_eq!(domain::storage(&ScalarValue::try_from_array(&array,0).unwrap()).unwrap(),ScalarValue::Decimal128(Some(value),38,0));
    assert!(array.is_null(1));
    for ty in [json_type,integer_type] {
        assert!(rows::build_column(&Field::new("value",ty.clone(),true),&[],0).unwrap().is_empty());
        assert!(rows::build_column(&Field::new("value",ty,true),&[vec![SqlValue::Bool(true)]],0).is_err());
    }
}

#[test]
#[ignore="requires PostgreSQL"]
fn postgres_negative_scale_decimal_results_reconstruct_without_changing_magnitude() {
    let url=std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL");
    let result=std::process::Command::new("psql").args([&url,"-X","-A","-t","-v","ON_ERROR_STOP=1","-c",
        "SELECT v::text FROM (VALUES (12300::numeric(10,-2)),(-12300::numeric(10,-2)),(0::numeric(10,-2))) AS t(v)"]).output().unwrap();
    assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    let values=String::from_utf8(result.stdout).unwrap().lines().map(|text|vec![SqlValue::ExactNumber(text.into())]).collect::<Vec<_>>();
    let array=rows::build_column(&Field::new("amount",DataType::Decimal128(10,-2),false),&values,0).unwrap();
    assert_eq!(array.len(),3);
    for (row,expected) in [123_i128,-123,0].into_iter().enumerate() {
        assert_eq!(ScalarValue::try_from_array(&array,row).unwrap(),ScalarValue::Decimal128(Some(expected),10,-2));
    }
}

#[test]
fn sql_integer_width_boundaries_preserve_values_and_reject_overflow() {
    for (ty,minimum,maximum) in [(DataType::Int8,-128,127),(DataType::Int16,-32768,32767),
        (DataType::UInt8,0,255),(DataType::UInt16,0,65535),(DataType::UInt32,0,4294967295)] {
        let field=Field::new("number",ty.clone(),true);
        let values=vec![vec![SqlValue::Int(minimum)],vec![SqlValue::Null],vec![SqlValue::ExactNumber(maximum.to_string())]];
        let array=rows::build_column(&field,&values,0).unwrap();
        assert_eq!(array.data_type(),&ty);assert!(array.is_null(1));
        for (row,expected) in [(0,minimum),(2,maximum)] {
            assert_eq!(arrow::util::display::array_value_to_string(array.as_ref(),row).unwrap(),expected.to_string());
        }
        for value in [minimum-1,maximum+1] {assert!(rows::build_column(&field,&[vec![SqlValue::Int(value)]],0).is_err());}
    }
}

#[test]
fn sql_binary_and_dictionary_rows_preserve_payloads_and_declared_storage() {
    for ty in [DataType::Binary,DataType::LargeBinary,DataType::BinaryView] {
        let array=rows::build_column(&Field::new("bytes",ty.clone(),true),
            &[vec![SqlValue::Binary(vec![0,255,39])],vec![SqlValue::Null],vec![SqlValue::Binary(vec![])]],0).unwrap();
        assert_eq!(array.data_type(),&ty);assert!(array.is_null(1));
        let canonical=arrow::compute::cast(array.as_ref(),&DataType::Binary).unwrap();
        assert_eq!(ScalarValue::try_from_array(&canonical,0).unwrap(),ScalarValue::Binary(Some(vec![0,255,39])));
        assert_eq!(ScalarValue::try_from_array(&canonical,2).unwrap(),ScalarValue::Binary(Some(vec![])));
    }
    for key in [DataType::Int8,DataType::UInt16,DataType::Int32] {
        let ty=DataType::Dictionary(Box::new(key),Box::new(DataType::Utf8));
        let array=rows::build_column(&Field::new("text",ty.clone(),true),
            &[vec![SqlValue::Text("雪".into())],vec![SqlValue::Null],vec![SqlValue::Text("雪".into())]],0).unwrap();
        assert_eq!(array.data_type(),&ty);
        let decoded=arrow::compute::cast(array.as_ref(),&DataType::Utf8).unwrap();
        assert!(decoded.is_null(1));assert_eq!(ScalarValue::try_from_array(&decoded,0).unwrap(),ScalarValue::Utf8(Some("雪".into())));
        assert_eq!(ScalarValue::try_from_array(&decoded,0).unwrap(),ScalarValue::try_from_array(&decoded,2).unwrap());
    }
}

#[test]
fn sql_temporal_rows_preserve_epoch_precision_and_reject_invalid_text() {
    use arrow::datatypes::TimeUnit;
    for (ty,text,expected) in [
        (DataType::Date32,"1970-01-01",ScalarValue::Date32(Some(0))),
        (DataType::Date64,"1970-01-02",ScalarValue::Date64(Some(86400000))),
        (DataType::Timestamp(TimeUnit::Microsecond,None),"1969-12-31 23:59:59.123456",ScalarValue::TimestampMicrosecond(Some(-876544),None)),
        (DataType::Time64(TimeUnit::Microsecond),"00:00:01.123456",ScalarValue::Time64Microsecond(Some(1123456))),
    ] {
        let field=Field::new("when",ty.clone(),true);
        let array=rows::build_column(&field,&[vec![SqlValue::Text(text.into())],vec![SqlValue::Null]],0).unwrap();
        assert_eq!(array.data_type(),&ty);assert_eq!(ScalarValue::try_from_array(&array,0).unwrap(),expected);assert!(array.is_null(1));
        assert!(rows::build_column(&field,&[vec![SqlValue::Text("not-a-time".into())]],0).is_err());
    }
}

#[test]
fn sql_list_storage_preserves_parent_nulls_and_checks_fixed_width() {
    let item=Arc::new(Field::new("item",DataType::Int64,true));
    let values=vec![vec![SqlValue::List(vec![SqlValue::Int(7),SqlValue::Null])],vec![SqlValue::Null]];
    for ty in [DataType::List(item.clone()),DataType::LargeList(item.clone()),DataType::FixedSizeList(item.clone(),2)] {
        let array=rows::build_column(&Field::new("items",ty.clone(),true),&values,0).unwrap();
        assert_eq!(array.data_type(),&ty);assert!(array.is_null(1));
        let canonical=arrow::compute::cast(array.as_ref(),&DataType::List(item.clone())).unwrap();
        let ScalarValue::List(list)=ScalarValue::try_from_array(&canonical,0).unwrap() else {panic!()};
        assert_eq!(list.value_length(0),2);assert_eq!(ScalarValue::try_from_array(&list.value(0),0).unwrap(),ScalarValue::Int64(Some(7)));
        assert!(list.value(0).is_null(1));
    }
    assert!(rows::build_column(&Field::new("items",DataType::FixedSizeList(item,2),true),
        &[vec![SqlValue::List(vec![SqlValue::Int(1)])]],0).is_err());
}

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

fn executor_fallback_table(name: &str) -> TableData {
    use arrow::array::{Int64Array, StringArray};
    let batch = RecordBatch::try_from_iter(vec![
        ("id", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef),
        ("text", Arc::new(StringArray::from(vec![Some("O'Reilly 雪"), None])) as ArrayRef),
    ]).unwrap();
    TableData { name: name.into(), schema: batch.schema(), batches: vec![batch] }
}

#[cfg(feature = "duckdb")]
#[test]
fn executor_fallback_materializes_tables_before_setup_and_preserves_values() {
    struct TextExecutor(DuckDbExecutor);
    impl SqlExecutor for TextExecutor {
        fn dialect(&self) -> SqlDialect { SqlDialect::DuckDb }
        fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>> {
            self.0.run(setup, query)
        }
    }
    let mut executor = TextExecutor(DuckDbExecutor::new());
    let table = executor_fallback_table("data table");
    let rows = executor.run_with_tables(&[table],
        &["UPDATE \"data table\" SET id=id+10".into()],
        "SELECT id, text FROM \"data table\" ORDER BY id").unwrap();
    assert_eq!(rows, vec![
        vec![SqlValue::Int(11), SqlValue::Text("O'Reilly 雪".into())],
        vec![SqlValue::Int(12), SqlValue::Null],
    ]);
    assert_eq!(executor.run_with_tables(&[], &[], "SELECT count(*) FROM \"data table\"").unwrap(),
        vec![vec![SqlValue::Int(2)]]);
}

#[test]
fn executor_fallback_validates_all_tables_before_dispatch_and_keeps_engine_errors() {
    struct RecordingExecutor { dialect: SqlDialect, calls: usize, setup: Vec<String> }
    impl SqlExecutor for RecordingExecutor {
        fn dialect(&self) -> SqlDialect { self.dialect }
        fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>> {
            self.calls += 1; self.setup = setup.to_vec();
            assert_eq!(query, "SELECT id FROM good");
            Err(SqlError::Execution("source connection failed".into()))
        }
    }
    for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres, SqlDialect::resolve("trino").unwrap()] {
        let mut executor = RecordingExecutor { dialect, calls: 0, setup: vec![] };
        let good = executor_fallback_table("good");
        let invalid = TableData { name: "invalid".into(), schema: Arc::new(Schema::empty()), batches: vec![] };
        let error = executor.run_with_tables(&[good.clone(), invalid], &["user setup".into()], "SELECT id FROM good").unwrap_err();
        assert!(error.to_string().contains("has no columns"), "{error}");
        assert_eq!(executor.calls, 0, "no partial materialization on generation failure");
        let error = executor.run_with_tables(&[good], &["user setup".into()], "SELECT id FROM good").unwrap_err();
        assert!(matches!(error, SqlError::Execution(ref message) if message == "source connection failed"));
        assert_eq!(executor.calls, 1);
        assert_eq!(executor.setup.len(), 3);
        assert!(executor.setup[0].starts_with("CREATE"));
        assert!(executor.setup[1].starts_with("INSERT INTO"));
        assert_eq!(executor.setup[2], "user setup");
    }
}
