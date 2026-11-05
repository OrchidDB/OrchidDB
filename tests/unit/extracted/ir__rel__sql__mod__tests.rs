
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

#[test]
fn iceberg_provider_keeps_predicates_above_the_scan() {
    use datafusion::{datasource::provider_as_source, logical_expr::{TableProviderFilterPushDown, LogicalPlanBuilder, col, lit}};
    let metadata = crate::ir::rel::source_metadata::SourceMetadata {
        table: "lake".into(), format: Some("duckdb_iceberg".into()),
        options: [("path".into(), serde_json::json!("/lake/owner's data"))].into(), indexes: vec![],
    };
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
    let provider = duckdb_iceberg::provider(&metadata, schema.clone()).unwrap().unwrap();
    let predicate = col("id").gt(lit(5_i64));
    let null_predicate = col("id").is_not_null();
    for filters in [vec![], vec![&predicate], vec![&predicate, &null_predicate]] {
        assert_eq!(provider.supports_filters_pushdown(&filters).unwrap(), vec![TableProviderFilterPushDown::Unsupported; filters.len()]);
    }
    // A violated planner contract must fail closed instead of silently dropping WHERE.
    let mut plan = LogicalPlanBuilder::scan("lake", provider_as_source(provider), None).unwrap().build().unwrap();
    let sql = duckdb_iceberg::lower(&plan).unwrap().unwrap().to_string();
    assert!(sql.contains("owner''s data"), "{sql}");
    if let LogicalPlan::TableScan(scan) = &mut plan { scan.filters.push(predicate); } else { panic!("expected scan") }
    assert!(duckdb_iceberg::lower(&plan).unwrap_err().to_string().contains("unexpected pushed filters"));
}

#[test]
fn duckdb_region_session_collects_typed_batches_and_preserves_empty_schema() {
    use super::region::RegionSession;
    use arrow::array::{Int64Array, Array};
    let mut session = DuckDbSession::default();
    let schema = Arc::new(Schema::new(vec![Field::new("value", DataType::Int64, true)]));
    let batch = RegionSession::query(&mut session,
        "SELECT CASE WHEN i = 1 THEN NULL ELSE i END AS value FROM range(4097) AS t(i) ORDER BY i", schema.clone()).unwrap();
    assert_eq!(batch.schema(), schema);
    assert_eq!(batch.num_rows(), 4097);
    let values = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
    assert_eq!(values.value(0), 0);
    assert!(values.is_null(1));
    assert_eq!(values.value(4096), 4096);
    let empty = RegionSession::query(&mut session, "SELECT 1::BIGINT AS value WHERE false", schema.clone()).unwrap();
    assert_eq!(empty.schema(), schema);
    assert_eq!(empty.num_rows(), 0);
    assert!(RegionSession::query(&mut session, "SELECT missing_column", schema).is_err());
}

#[test]
fn duckdb_region_session_rejects_unverified_helpers_and_poisoned_sessions() {
    use super::region::RegionSession;
    let capability = crate::ir::rel::language_functions::FloatHelperCapability::verified("foreign-session".into());
    let session = DuckDbSession::default();
    assert!(RegionSession::validate_float_helper(&session, &capability).unwrap_err().to_string().contains("reprepare"));
    let executor = session.executor.clone();
    let poison = std::thread::spawn(move || {
        let _guard = executor.lock().unwrap();
        panic!("intentional session owner failure");
    });
    assert!(poison.join().is_err());
    assert!(matches!(RegionSession::validate_float_helper(&session, &capability),
        Err(SqlError::Execution(message)) if message == "DuckDB session poisoned"));
}

#[path = "sql_exchange_literals.rs"]
mod exchange_literals;

#[test]
fn engine_scalar_transport_preserves_primitive_values_and_typed_nulls() {
    let cases = vec![
        (ScalarValue::Boolean(Some(false)), SqlValue::Bool(false)),
        (ScalarValue::Int8(Some(i8::MIN)), SqlValue::Int(i8::MIN.into())),
        (ScalarValue::Int16(Some(i16::MIN)), SqlValue::Int(i16::MIN.into())),
        (ScalarValue::Int32(Some(i32::MIN)), SqlValue::Int(i32::MIN.into())),
        (ScalarValue::Int64(Some(i64::MIN)), SqlValue::Int(i64::MIN)),
        (ScalarValue::UInt8(Some(u8::MAX)), SqlValue::Int(u8::MAX.into())),
        (ScalarValue::UInt16(Some(u16::MAX)), SqlValue::Int(u16::MAX.into())),
        (ScalarValue::UInt32(Some(u32::MAX)), SqlValue::Int(u32::MAX.into())),
        (ScalarValue::UInt64(Some(i64::MAX as u64)), SqlValue::Int(i64::MAX)),
        (ScalarValue::Float32(Some(1.25)), SqlValue::Float(1.25)),
        (ScalarValue::Float64(Some(-2.5)), SqlValue::Float(-2.5)),
        (ScalarValue::Utf8(Some("雪🙂'".into())), SqlValue::Text("雪🙂'".into())),
        (ScalarValue::LargeUtf8(Some("雪🙂'".into())), SqlValue::Text("雪🙂'".into())),
        (ScalarValue::Utf8View(Some("雪🙂'".into())), SqlValue::Text("雪🙂'".into())),
        (ScalarValue::Binary(Some(vec![0,255,39])), SqlValue::Binary(vec![0,255,39])),
        (ScalarValue::LargeBinary(Some(vec![0,255,39])), SqlValue::Binary(vec![0,255,39])),
        (ScalarValue::BinaryView(Some(vec![0,255,39])), SqlValue::Binary(vec![0,255,39])),
        (ScalarValue::Date32(Some(0)), SqlValue::Text("1970-01-01".into())),
    ];
    for (value, expected) in cases {
        let null=ScalarValue::try_from(&value.data_type()).unwrap();
        let array=ScalarValue::iter_to_array([null.clone(),value.clone()]).unwrap();
        assert_eq!(scalar_to_sql_value(&value).unwrap(),expected,"{value:?}");
        assert_eq!(sql_value_from_array(array.as_ref(),1).unwrap(),expected);
        assert_eq!(scalar_to_sql_value(&null).unwrap(),SqlValue::Null);
        assert_eq!(sql_value_from_array(array.as_ref(),0).unwrap(),SqlValue::Null);
    }
    assert_eq!(scalar_to_sql_value(&ScalarValue::Null).unwrap(),SqlValue::Null);
    for value in [i64::MAX as u64+1,u64::MAX] {
        assert!(matches!(scalar_to_sql_value(&ScalarValue::UInt64(Some(value))),Err(SqlError::Conversion(message)) if message.contains("overflows i64")));
    }
    assert!(matches!(scalar_to_sql_value(&ScalarValue::TimestampNanosecond(Some(0),None)),Err(SqlError::Unsupported(message)) if message.contains("engine value of type")));
}

#[test]
fn engine_decimal_transport_is_exact_for_every_arrow_scale() {
    // Arrow permits negative scales down to i8::MIN. Never negate that value
    // in i8 or pass exact fixed-point values through floating point.
    for scale in i8::MIN..=38 {
        for value in [0_i128,1,-1,12345,-12345,10_i128.pow(38)-1] {
            let expected=if scale<=0 {
                format!("{value}{}","0".repeat(-(scale as i16) as usize))
            } else {
                let magnitude=value.unsigned_abs();
                let factor=10_u128.pow(scale as u32);
                format!("{}{}.{:0width$}",if value<0 {"-"} else {""},magnitude/factor,magnitude%factor,width=scale as usize)
            };
            assert_eq!(rows::parse_decimal_i128(&expected,scale),Ok(value));
            let scalar=ScalarValue::Decimal128(Some(value),38,scale);
            assert_eq!(scalar_to_sql_value(&scalar).unwrap(),SqlValue::ExactNumber(expected.clone()),"{value} scale {scale}");
            let array=scalar.to_array().unwrap();
            assert_eq!(sql_value_from_array(array.as_ref(),0).unwrap(),SqlValue::ExactNumber(expected));
            assert_eq!(scalar_to_sql_value(&ScalarValue::Decimal128(None,38,scale)).unwrap(),SqlValue::Null);
        }
    }
}

#[test]
fn engine_nested_transport_distinguishes_null_empty_and_populated_lists() {
    use arrow::array::{ArrayRef,FixedSizeListArray,Int64Array,LargeListArray,ListArray};
    use arrow::buffer::{NullBuffer,OffsetBuffer};
    let field=Arc::new(Field::new("item",DataType::Int64,true));
    let values=Arc::new(Int64Array::from(vec![Some(7),None,Some(-3)])) as ArrayRef;
    let arrays:Vec<ArrayRef>=vec![
        Arc::new(ListArray::new(field.clone(),OffsetBuffer::new(vec![0_i32,0,0,3].into()),values.clone(),Some(NullBuffer::from(vec![false,true,true])))),
        Arc::new(LargeListArray::new(field.clone(),OffsetBuffer::new(vec![0_i64,0,0,3].into()),values.clone(),Some(NullBuffer::from(vec![false,true,true])))),
    ];
    for array in arrays {
        assert_eq!(sql_value_from_array(array.as_ref(),0).unwrap(),SqlValue::Null);
        assert_eq!(sql_value_from_array(array.as_ref(),1).unwrap(),SqlValue::List(vec![]));
        assert_eq!(sql_value_from_array(array.as_ref(),2).unwrap(),SqlValue::List(vec![SqlValue::Int(7),SqlValue::Null,SqlValue::Int(-3)]));
        let outer=ListArray::new(Arc::new(Field::new("item",array.data_type().clone(),true)),OffsetBuffer::new(vec![0,3].into()),array,None);
        assert_eq!(sql_value_from_array(&outer,0).unwrap(),SqlValue::List(vec![SqlValue::Null,SqlValue::List(vec![]),SqlValue::List(vec![SqlValue::Int(7),SqlValue::Null,SqlValue::Int(-3)])]));
    }
    let fixed=FixedSizeListArray::new(field,3,values,None);
    assert_eq!(sql_value_from_array(&fixed,0).unwrap(),SqlValue::List(vec![SqlValue::Int(7),SqlValue::Null,SqlValue::Int(-3)]));
    let null=ScalarValue::try_from(fixed.data_type()).unwrap();
    assert_eq!(scalar_to_sql_value(&null).unwrap(),SqlValue::Null);
}

#[test]
fn engine_nominal_and_structured_transport_retains_logical_identity() {
    use crate::ir::functions::domain;
    let record_type=DataType::Struct(vec![Field::new("name",DataType::Utf8,true),Field::new("amount",DataType::Int64,true)].into());
    let record=structured::decode(&serde_json::json!({"name":"雪","amount":7}),&record_type).unwrap();
    for value in [
        domain::json_scalar("null").unwrap(),
        domain::json_scalar("{\"a\":[1,null]}").unwrap(),
        domain::scalar("cypher_integer",ScalarValue::Decimal128(Some(10_i128.pow(37)),38,0)).unwrap(),
        domain::scalar("custom_integer",ScalarValue::Int64(None)).unwrap(),
        record,
    ] {
        assert_eq!(scalar_to_sql_value(&value).unwrap(),SqlValue::Domain(value.clone()));
        let array=value.to_array().unwrap();
        assert_eq!(sql_value_from_array(array.as_ref(),0).unwrap(),SqlValue::Domain(value.clone()));
        let field=Field::new("v",value.data_type(),true);
        let decoded=rows::build_column(&field,&[vec![SqlValue::Domain(value.clone())],vec![SqlValue::Null]],0).unwrap();
        assert_eq!(ScalarValue::try_from_array(decoded.as_ref(),0).unwrap(),value);
        assert!(decoded.is_null(1));
        let null=ScalarValue::try_from(&value.data_type()).unwrap();
        if domain::descriptor(&value.data_type()).is_some() {
            assert_eq!(scalar_to_sql_value(&null).unwrap(),SqlValue::Null);
        }
    }
}

struct VectorTransportSnapshot;
impl duckdb::vscalar::VScalar for VectorTransportSnapshot {
    type State=();
    fn signatures()->Vec<duckdb::vscalar::ScalarFunctionSignature> {
        vec![duckdb::vscalar::ScalarFunctionSignature::exact(vec![duckdb::core::LogicalTypeId::Any.into()],duckdb::core::LogicalTypeId::Varchar.into())]
    }
    unsafe fn invoke(_: &(),input:&mut duckdb::core::DataChunkHandle,output:&mut dyn duckdb::vtab::arrow::WritableVector)->Result<(),Box<dyn std::error::Error>> {
        let arrays=super::language_vectors::arrays(input)?;
        let values=(0..input.len()).map(|row|{
            let scalar=ScalarValue::try_from_array(&arrays[0],row)?;
            Ok(serde_json::to_string(&crate::ir::expr::Lit::Scalar(scalar))?)
        }).collect::<Result<Vec<String>,Box<dyn std::error::Error>>>()?;
        let array=Arc::new(arrow::array::StringArray::from(values)) as arrow::array::ArrayRef;
        duckdb::vtab::arrow::write_arrow_array_to_vector(&array,output)
    }
}
fn vector_snapshot(connection:&duckdb::Connection,sql:&str)->Vec<ScalarValue> {
    connection.prepare(sql).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap()
        .map(|row|{let crate::ir::expr::Lit::Scalar(value)=serde_json::from_str(&row.unwrap()).unwrap() else {panic!()};value}).collect()
}
fn vector_record(value:ScalarValue)->ScalarValue {
    ScalarValue::Struct(Arc::new(arrow::array::StructArray::new(
        vec![Field::new("v",value.data_type(),true)].into(),vec![value.to_array().unwrap()],None)))
}

#[test]
fn duckdb_vector_callback_preserves_nested_scalar_widths_and_temporal_precision() {
    use arrow::datatypes::i256;
    let connection=duckdb::Connection::open_in_memory().unwrap();
    connection.register_scalar_function::<VectorTransportSnapshot>("coverage_vector_snapshot").unwrap();
    let cases=vec![
        ("BOOLEAN","true",ScalarValue::Boolean(Some(true))),
        ("TINYINT","-128",ScalarValue::Int8(Some(i8::MIN))),
        ("SMALLINT","-32768",ScalarValue::Int16(Some(i16::MIN))),
        ("INTEGER","-2147483648",ScalarValue::Int32(Some(i32::MIN))),
        ("BIGINT","-9223372036854775808",ScalarValue::Int64(Some(i64::MIN))),
        ("UTINYINT","255",ScalarValue::UInt8(Some(u8::MAX))),
        ("USMALLINT","65535",ScalarValue::UInt16(Some(u16::MAX))),
        ("UINTEGER","4294967295",ScalarValue::UInt32(Some(u32::MAX))),
        ("UBIGINT","18446744073709551615",ScalarValue::UInt64(Some(u64::MAX))),
        ("HUGEINT","'-170141183460469231731687303715884105728'",ScalarValue::Decimal256(Some(i256::from_i128(i128::MIN)),39,0)),
        ("FLOAT","1.25",ScalarValue::Float32(Some(1.25))),
        ("DOUBLE","-2.5",ScalarValue::Float64(Some(-2.5))),
        ("DECIMAL(4,2)","-12.34",ScalarValue::Decimal128(Some(-1234),4,2)),
        ("DECIMAL(9,3)","-123456.789",ScalarValue::Decimal128(Some(-123456789),9,3)),
        ("DECIMAL(18,6)","-123456789012.345678",ScalarValue::Decimal128(Some(-123456789012345678),18,6)),
        ("DECIMAL(38,9)","'-12345678901234567890.123456789'",ScalarValue::Decimal128(Some(-12345678901234567890123456789),38,9)),
        ("DATE","'1969-12-31'",ScalarValue::Date32(Some(-1))),
        ("TIMESTAMP_S","'1969-12-31 23:59:59'",ScalarValue::TimestampSecond(Some(-1),None)),
        ("TIMESTAMP_MS","'1969-12-31 23:59:59.123'",ScalarValue::TimestampMillisecond(Some(-877),None)),
        ("TIMESTAMP","'1969-12-31 23:59:59.123456'",ScalarValue::TimestampMicrosecond(Some(-876544),None)),
        ("TIMESTAMP_NS","'1969-12-31 23:59:59.123456789'",ScalarValue::TimestampNanosecond(Some(-876543211),None)),
        ("TIMESTAMPTZ","'1969-12-31 23:59:59.123456+00'",ScalarValue::TimestampMicrosecond(Some(-876544),Some("UTC".into()))),
        ("VARCHAR","'short'",ScalarValue::Utf8(Some("short".into()))),
        ("VARCHAR","'O''Reilly 雪🙂 longer than inline'",ScalarValue::Utf8(Some("O'Reilly 雪🙂 longer than inline".into()))),
        ("BLOB","from_hex('00ff27')",ScalarValue::Binary(Some(vec![0,255,39]))),
    ];
    for (kind,literal,expected) in cases {
        let sql=format!("SELECT coverage_vector_snapshot(struct_pack(v := value)) FROM (VALUES (0,CAST(NULL AS {kind})),(1,CAST({literal} AS {kind})),(2,CAST(NULL AS {kind}))) t(i,value) ORDER BY i");
        let null=ScalarValue::try_from(&expected.data_type()).unwrap();
        assert_eq!(vector_snapshot(&connection,&sql),vec![vector_record(null.clone()),vector_record(expected),vector_record(null)],"{kind}");
    }
}

#[test]
fn duckdb_vector_callback_materializes_nested_dictionary_and_constant_children() {
    let connection=duckdb::Connection::open_in_memory().unwrap();
    connection.register_scalar_function::<VectorTransportSnapshot>("coverage_vector_snapshot").unwrap();
    let sql="SELECT coverage_vector_snapshot(struct_pack(v := CASE WHEN i % 3 = 0 THEN NULL::BIGINT[] WHEN i % 3 = 1 THEN []::BIGINT[] ELSE [i,NULL,i] END)) FROM range(4097) t(i) WHERE i % 2 = 0 ORDER BY i";
    let actual=vector_snapshot(&connection,sql);
    let expected=(0..4097_i64).filter(|i|i%2==0).map(|i|{
        let ty=DataType::List(Arc::new(Field::new("item",DataType::Int64,true)));
        let value=match i%3 {
            0=>ScalarValue::try_from(&ty).unwrap(),
            1=>ScalarValue::List(ScalarValue::new_list_nullable(&[],&DataType::Int64)),
            _=>ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(i)),ScalarValue::Int64(None),ScalarValue::Int64(Some(i))],&DataType::Int64)),
        };vector_record(value)
    }).collect::<Vec<_>>();
    assert_eq!(actual,expected);
    let value=ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int32(None),ScalarValue::Int32(None)],&DataType::Int32));
    assert_eq!(vector_snapshot(&connection,"SELECT coverage_vector_snapshot(struct_pack(v := [NULL,NULL]::INTEGER[])) FROM range(3)"),vec![vector_record(value);3]);
}

#[test]
fn duckdb_empty_vector_transport_retains_schema_and_rejects_unsupported_types() {
    use duckdb::core::{DataChunkHandle,LogicalTypeHandle as T,LogicalTypeId as L};
    let types=vec![L::Bigint.into(),L::Blob.into(),T::decimal(38,9),T::list(&L::Integer.into()),T::struct_type(&[("v",L::Integer.into())])];
    let input=DataChunkHandle::new(&types);input.set_len(0);
    let arrays=super::language_vectors::arrays(&input).unwrap();
    assert_eq!(arrays.len(),5);assert!(arrays.iter().all(|a|a.is_empty()));
    assert_eq!(arrays[0].data_type(),&DataType::Int64);assert_eq!(arrays[1].data_type(),&DataType::Binary);assert_eq!(arrays[2].data_type(),&DataType::Decimal128(38,9));
    assert_eq!(arrays[3].data_type(),&DataType::List(Arc::new(Field::new("item",DataType::Int32,true))));
    assert_eq!(arrays[4].data_type(),&DataType::Struct(vec![Field::new("v",DataType::Int32,true)].into()));
    for kind in [L::Interval.into(),T::array(&L::Integer.into(),2)] {
        let input=DataChunkHandle::new(&[kind]);input.set_len(0);
        assert!(super::language_vectors::arrays(&input).unwrap_err().to_string().contains("unsupported language vector type"));
    }
}

#[test]
fn duckdb_vector_callback_preserves_lists_of_nullable_records_across_chunks() {
    let connection=duckdb::Connection::open_in_memory().unwrap();
    connection.register_scalar_function::<VectorTransportSnapshot>("coverage_vector_snapshot").unwrap();
    let fields:arrow::datatypes::Fields=vec![Field::new("label",DataType::Utf8,true),Field::new("count",DataType::Int64,true)].into();
    let record_type=DataType::Struct(fields.clone());
    let list_type=DataType::List(Arc::new(Field::new("item",record_type.clone(),true)));
    let sql="SELECT coverage_vector_snapshot(struct_pack(v := CASE WHEN i % 5 = 0 THEN NULL::STRUCT(label VARCHAR, count BIGINT)[] ELSE [NULL::STRUCT(label VARCHAR, count BIGINT), struct_pack(label := CASE WHEN i % 4 = 0 THEN NULL::VARCHAR ELSE '雪' || chr(0) || CAST(i AS VARCHAR) END, count := i)] END)) FROM range(5003) t(i) WHERE i % 2 = 0 ORDER BY i";
    let expected=(0..5003_i64).filter(|i|i%2==0).map(|i| {
        let value=if i%5==0 {ScalarValue::try_from(&list_type).unwrap()} else {
            let label=ScalarValue::Utf8(if i%4==0 {None}else{Some(format!("雪\0{i}"))});
            let record=ScalarValue::Struct(Arc::new(arrow::array::StructArray::new(fields.clone(),
                vec![label.to_array().unwrap(),ScalarValue::Int64(Some(i)).to_array().unwrap()],None)));
            ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::try_from(&record_type).unwrap(),record],&record_type))
        };vector_record(value)
    }).collect::<Vec<_>>();
    assert_eq!(vector_snapshot(&connection,sql),expected);
}
