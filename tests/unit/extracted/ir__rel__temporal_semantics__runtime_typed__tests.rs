
    use super::*;
    #[test]
    fn wide_integer_domain_decodes_to_language_integer_and_preserves_null() {
        let integer = 9223372036854775807999999999_i128;
        let value = domain::scalar(
            "cypher_integer",
            ScalarValue::Decimal128(Some(integer), 38, 0),
        )
        .unwrap();
        let batch = RecordBatch::try_new(
            Arc::new(arrow::datatypes::Schema::new(vec![
                arrow::datatypes::Field::new("ns", value.data_type(), true),
            ])),
            vec![
                ScalarValue::iter_to_array(vec![
                    value.clone(),
                    ScalarValue::try_from(&value.data_type()).unwrap(),
                ])
                .unwrap(),
            ],
        )
        .unwrap();
        let (_, rows) = crate::ir::exec::batch_to_bindings(&crate::ir::runtime::ReturnedBatches {
            fields: vec!["ns".into()],
            result_form: crate::ir::policy::ResultForm::RowSet,
            batch,
        })
        .unwrap();
        assert_eq!(rows[0][0], Value::BigInt(integer.into()));
        assert_eq!(rows[1][0], Value::Null);
        let Value::BigInt(integer) = &rows[0][0] else {
            panic!()
        };
        assert_eq!(integer.to_string(), "9223372036854775807999999999");
    }

    fn quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }
    fn execute(sql: &str, left: &str, right: &str) -> Result<String, String> {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        let sql = sql
            .replace("__arg0", &quote(left))
            .replace("__arg1", &quote(right));
        conn.query_row(&format!("SELECT CAST(({sql}) AS VARCHAR)"), [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("{e}\n{sql}"))
    }
    #[test]
    fn typed_components_match_calendar_clock_zone_and_duration_reference() {
        for (kind, text) in [
            ("date", "2000-01-01"),
            ("date", "-1000000-12-31"),
            ("date", "+1000000-01-01"),
            ("localtime", "12:31:14.645876123"),
            ("time", "00:00:00.000000001-00:01:02"),
            ("localdatetime", "1984-11-11T12:31:14.645876123"),
            ("localdatetime", "-1000000-12-31T23:59:59.999999999"),
            ("datetime", "1969-12-31T23:59:59.999999999Z"),
            (
                "datetime",
                "1984-11-11T12:31:14.645876123+01:00[Europe/Stockholm]",
            ),
            ("duration", "P12Y5M14DT16H13M10.000000001S"),
            ("duration", "P-1M-8DT-1.000000001S"),
        ] {
            let value = crate::ir::temporal::parse(kind, text).unwrap();
            for key in [
                "year",
                "quarter",
                "month",
                "week",
                "weekYear",
                "day",
                "ordinalDay",
                "weekDay",
                "dayOfQuarter",
                "hour",
                "minute",
                "second",
                "millisecond",
                "microsecond",
                "nanosecond",
                "timezone",
                "offset",
                "offsetMinutes",
                "offsetSeconds",
                "epochSeconds",
                "epochMillis",
                "years",
                "quarters",
                "months",
                "weeks",
                "days",
                "hours",
                "minutes",
                "seconds",
                "milliseconds",
                "microseconds",
                "nanoseconds",
                "quartersOfYear",
                "monthsOfQuarter",
                "monthsOfYear",
                "daysOfWeek",
                "minutesOfHour",
                "secondsOfMinute",
                "millisecondsOfSecond",
                "microsecondsOfSecond",
                "nanosecondsOfSecond",
            ] {
                let expected = value.component(key);
                let sql = component_sql(kind, false, key).unwrap();
                if matches!(expected, Value::Null) {
                    let query = sql.replace("__arg0", &quote(&value.encode()));
                    let actual: bool = duckdb::Connection::open_in_memory().unwrap()
                        .query_row(&format!("SELECT ({query}) IS NULL"), [], |row| row.get(0)).unwrap();
                    assert!(actual, "{kind} {text} {key} must remain null");
                    continue;
                }
                let actual = execute(&sql, &value.encode(), "")
                    .unwrap_or_else(|e| panic!("{kind} {key}: {e}"));
                let expected = match expected {
                    Value::String(s) => s,
                    Value::Int(n) => n.to_string(),
                    Value::BigInt(n) => n.to_string(),
                    other => panic!("{other:?}"),
                };
                assert_eq!(actual, expected, "{kind} {text} {key}");
            }
        }
    }
    #[test]
    fn runtime_calendar_arithmetic_matches_reference_for_month_clamping_and_nanoseconds() {
        for (k, text) in [
            ("date", "2000-01-31"),
            ("date", "0001-01-01"),
            ("date", "-1000000-02-29"),
            ("date", "+1000000-12-31"),
            ("localdatetime", "2000-01-31T23:59:59.999999999"),
            ("localdatetime", "-1000000-02-29T00:00:00.000000001"),
            ("datetime", "1984-10-11T12:31:14.000000001+01:00"),
        ] {
            let value = crate::ir::temporal::parse(k, text).unwrap();
            for duration in [
                "P1M",
                "P-1M",
                "P1DT0.000000002S",
                "P-1DT-0.000000002S",
                "P12Y5M14DT16H13M10.000000002S",
                "PT-0.000000001S",
            ] {
                let duration = crate::ir::temporal::parse("duration", duration).unwrap();
                for (op, sign) in [(BinaryOp::Add, 1), (BinaryOp::Sub, -1)] {
                    let Value::Temporal(expected) = crate::ir::temporal::arithmetic(
                        op,
                        &Value::Temporal(value.clone()),
                        &Value::Temporal(duration.clone()),
                    )
                    .unwrap() else {
                        panic!()
                    };
                    let actual = execute(
                        &calendar_sql(k, false, sign),
                        &value.encode(),
                        &duration.encode(),
                    )
                    .unwrap_or_else(|e| panic!("{k} {text} {op:?}: {e}"));
                    assert_eq!(actual, expected.encode(), "{k} {text} {op:?} {duration}");
                }
            }
        }
    }

#[tokio::test]
async fn native_timestamp_bounds_preserve_precision_negative_epoch_and_nulls() {
    use arrow::{array::BooleanArray,datatypes::{Field,Schema,TimeUnit}};
    use datafusion::prelude::col;
    use datafusion::logical_expr::execution_props::ExecutionProps;
    let operators=[BinaryOp::Eq,BinaryOp::Neq,BinaryOp::Lt,BinaryOp::Lte,BinaryOp::Gt,BinaryOp::Gte];
    for (unit,tick) in [(TimeUnit::Second,1_000_000_000_i128),(TimeUnit::Millisecond,1_000_000),(TimeUnit::Microsecond,1_000),(TimeUnit::Nanosecond,1)] {
        for zoned in [false,true] {
            let zone=if zoned {Some(Arc::<str>::from("UTC"))} else {None};
            let ty=DataType::Timestamp(unit.clone(),zone.clone());
            let times=[Some(-2_i64),Some(-1),Some(0),Some(1),Some(2),None];
            let scalars=times.iter().map(|v|match unit {
                TimeUnit::Second=>ScalarValue::TimestampSecond(*v,zone.clone()),
                TimeUnit::Millisecond=>ScalarValue::TimestampMillisecond(*v,zone.clone()),
                TimeUnit::Microsecond=>ScalarValue::TimestampMicrosecond(*v,zone.clone()),
                TimeUnit::Nanosecond=>ScalarValue::TimestampNanosecond(*v,zone.clone()),
            }).collect::<Vec<_>>();
            let batch=RecordBatch::try_new(Arc::new(Schema::new(vec![Field::new("ts",ty,true)])),vec![ScalarValue::iter_to_array(scalars).unwrap()]).unwrap();
            let schema=Arc::new(datafusion::common::DFSchema::try_from(batch.schema().as_ref().clone()).unwrap());
            let plan=LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {produce_one_row:false,schema});
            for (text,ns) in [("1969-12-31T23:59:59.999999999",-1_i128),("1970-01-01T00:00:00",0),("1970-01-01T00:00:00.000000001",1),("1970-01-01T00:00:01.000000001",1_000_000_001)] {
                let text=if zoned {format!("{text}Z")} else {text.to_string()};
                let value=crate::ir::temporal::parse(if zoned {"datetime"}else{"localdatetime"},&text).unwrap();
                let constant=lit(transport::scalar(&value).unwrap());
                for op in operators {
                    for swapped in [false,true] {
                        let (left,right)=if swapped {(constant.clone(),col("ts"))}else{(col("ts"),constant.clone())};
                        let comparison=native_timestamp_comparison_bound(&plan,op,left,right).expect("timestamp comparison must lower");
                        let output=datafusion_physical_expr::create_physical_expr(&comparison,plan.schema(),&ExecutionProps::new()).unwrap()
                            .evaluate(&batch).unwrap().into_array(batch.num_rows()).unwrap();
                        let actual=output.as_any().downcast_ref::<BooleanArray>().unwrap();
                        let expected=times.iter().map(|value|value.map(|n| {
                            let (a,b)=if swapped {(ns,i128::from(n)*tick)}else{(i128::from(n)*tick,ns)};
                            match op {BinaryOp::Eq=>a==b,BinaryOp::Neq=>a!=b,BinaryOp::Lt=>a<b,BinaryOp::Lte=>a<=b,BinaryOp::Gt=>a>b,BinaryOp::Gte=>a>=b,_=>unreachable!()}
                        })).collect::<Vec<_>>();
                        assert_eq!(actual.iter().collect::<Vec<_>>(),expected,"{unit:?} zoned={zoned} {text} {op:?} swapped={swapped}");
                    }
                }
            }
        }
    }
}

#[test]
fn timestamp_bound_rewrite_preserves_domain_and_range_guards() {
    use arrow::datatypes::{Field,Schema,TimeUnit};
    use datafusion::prelude::col;
    let schema=Arc::new(datafusion::common::DFSchema::try_from(Schema::new(vec![
        Field::new("ts",DataType::Timestamp(TimeUnit::Nanosecond,None),true),
        Field::new("zoned",DataType::Timestamp(TimeUnit::Nanosecond,Some("UTC".into())),true),
        Field::new("number",DataType::Int64,true),
    ])).unwrap());
    let plan=LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {produce_one_row:false,schema});
    let temporal=|kind,text|transport::scalar(&crate::ir::temporal::parse(kind,text).unwrap()).unwrap();
    let local=temporal("localdatetime","1970-01-01T00:00:00");
    let date=temporal("date","1970-01-01");
    let zoned=temporal("datetime","1970-01-01T00:00:00Z");
    for (column,value) in [("ts",date),("ts",zoned),("zoned",local.clone()),("ts",ScalarValue::Int64(Some(0))),
        ("ts",temporal("localdatetime","+1000000-01-01T00:00:00"))] {
        assert!(native_timestamp_comparison_bound(&plan,BinaryOp::Eq,col(column),lit(value)).is_none());
    }
    assert!(native_timestamp_comparison_bound(&plan,BinaryOp::Eq,col("number"),lit(local.clone())).is_none());
    assert!(native_timestamp_comparison_bound(&plan,BinaryOp::Eq,col("ts"),col("number")).is_none());
    assert!(native_timestamp_comparison_bound(&plan,BinaryOp::Eq,col("ts"),lit(local.clone()).alias("value")).is_none());
    let null=ScalarValue::try_from(&local.data_type()).unwrap();
    assert_eq!(native_timestamp_comparison_bound(&plan,BinaryOp::Eq,col("ts"),lit(null)),Some(lit(ScalarValue::Boolean(None))));
    let projected=LogicalPlanBuilder::from(plan).project(vec![col("ts"),lit(local).alias("constant")]).unwrap().build().unwrap();
    let comparison=native_timestamp_comparison_bound(&projected,BinaryOp::Eq,col("ts"),col("constant")).unwrap();
    assert_eq!(comparison,col("ts").eq(lit(ScalarValue::TimestampNanosecond(Some(0),None))));
}

#[test]
fn native_date_components_and_calendar_arithmetic_match_temporal_reference() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    for text in ["1900-02-28", "2000-02-29", "1999-12-31", "2000-01-01", "2024-03-31", "2024-12-30"] {
        let value = crate::ir::temporal::parse("date", text).unwrap();
        for key in ["year", "quarter", "month", "week", "weekYear", "day", "ordinalDay", "weekDay", "dayOfQuarter"] {
            let sql = component_sql("date", true, key).unwrap().replace("__arg0", &format!("DATE {}", quote(text)));
            let actual: i64 = conn.query_row(&format!("SELECT {sql}"), [], |row| row.get(0)).unwrap();
            assert_eq!(Value::Int(actual), value.component(key), "{text} {key}");
        }
        for duration in ["P1M", "P-1M", "P1Y", "P-1DT-0.000000001S", "P1DT0.000000001S"] {
            let duration = crate::ir::temporal::parse("duration", duration).unwrap();
            for (op, sign) in [(BinaryOp::Add, 1), (BinaryOp::Sub, -1)] {
                let Value::Temporal(expected) = crate::ir::temporal::arithmetic(op, &Value::Temporal(value.clone()), &Value::Temporal(duration.clone())).unwrap() else { panic!() };
                let sql = calendar_sql("date", true, sign)
                    .replace("__arg0", &format!("DATE {}", quote(text)))
                    .replace("__arg1", &quote(&duration.encode()));
                let actual: String = conn.query_row(&format!("SELECT {sql}"), [], |row| row.get(0)).unwrap();
                assert_eq!(actual, expected.to_string(), "{text} {op:?} {duration}");
            }
        }
    }
}

#[test]
fn duration_scaling_ratio_accepts_exact_bounded_constants_only() {
    for (value, expected) in [
        (ScalarValue::Int64(Some(0)), Some((0, 1))),
        (ScalarValue::Int64(Some(-1_000_000_000)), Some((-1_000_000_000, 1))),
        (ScalarValue::Int64(Some(1_000_000_001)), None),
        (ScalarValue::Int64(Some(i64::MIN)), None),
        (ScalarValue::Float64(Some(-0.125)), Some((-125, 1000))),
        (ScalarValue::Float64(Some(0.000000001)), Some((1, 1_000_000_000))),
        (ScalarValue::Float64(Some(0.0000000001)), None),
        (ScalarValue::Float64(Some(1e30)), None),
        (ScalarValue::Float64(Some(f64::INFINITY)), None),
        (ScalarValue::Float64(Some(f64::NEG_INFINITY)), None),
        (ScalarValue::Float64(Some(f64::NAN)), None),
        (ScalarValue::Float64(None), None),
        (ScalarValue::Int64(None), None),
        (ScalarValue::Utf8(Some("1.5".into())), None),
    ] {
        assert_eq!(numeric_ratio(&lit(value.clone())), expected, "{value:?}");
    }
    assert_eq!(numeric_ratio(&datafusion::prelude::col("scale")), None);
}
