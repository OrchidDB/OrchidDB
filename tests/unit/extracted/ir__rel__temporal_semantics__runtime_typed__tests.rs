
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

#[test]
fn typed_column_arithmetic_matches_native_and_sql_with_null_and_empty_batches() {
    use datafusion::logical_expr::execution_props::ExecutionProps;
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None, graph: &graph, options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(), language: Language::Cypher,
        scan_counter: 0, correlate_plan: None, rdf_typed_terms_used: false,
        scan_logical_labels: false, gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(), unordered_distincts: Default::default(),
    };
    let conn = duckdb::Connection::open_in_memory().unwrap();
    for (kind, text) in [
        ("duration", "P1M2DT3.000000001S"), ("duration", "P-2M-3DT-0.000000001S"),
        ("localtime", "23:59:59.999999999"), ("time", "00:00:00.000000001-00:01:02"),
        ("date", "2000-01-31"), ("localdatetime", "2000-02-29T23:59:59.999999999"),
        ("datetime", "2000-02-29T23:59:59.999999999+01:00"),
    ] {
        let value = crate::ir::temporal::parse(kind, text).unwrap();
        let domain_kind = if kind == "datetime" { "datetime_fixed" } else { kind };
        let scalar = domain::scalar(&format!("cypher_temporal_{domain_kind}"), ScalarValue::Utf8(Some(value.encode()))).unwrap();
        // Evaluate component lowering on actual nullable columns, rather than
        // only checking the standalone SQL component templates.
        let component_schema = Arc::new(arrow::datatypes::Schema::new(vec![Field::new("a", scalar.data_type(), true)]));
        let component_plan = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
            produce_one_row: false, schema: Arc::new(datafusion::common::DFSchema::try_from(component_schema.as_ref().clone()).unwrap()),
        });
        let component_batch = RecordBatch::try_new(component_schema.clone(), vec![ScalarValue::iter_to_array(vec![scalar.clone(), ScalarValue::try_from(&scalar.data_type()).unwrap()]).unwrap()]).unwrap();
        for key in ["year", "month", "day", "hour", "nanosecond", "timezone", "offset", "offsetSeconds", "epochSeconds", "months", "seconds", "milliseconds", "microseconds", "nanoseconds", "unknown"] {
            let expr = lower(&context, &component_plan, &IrExpr::Call {
                name: "cypher_subscript".into(), args: vec![IrExpr::binding("a"), IrExpr::Lit(Lit::String(key.into()))],
            }).unwrap();
            let physical = datafusion_physical_expr::create_physical_expr(&expr, component_plan.schema(), &ExecutionProps::new()).unwrap();
            let output = physical.evaluate(&component_batch).unwrap().into_array(2).unwrap();
            assert!(ScalarValue::try_from_array(&output, 1).unwrap().is_null());
            assert_eq!(physical.evaluate(&RecordBatch::new_empty(component_schema.clone())).unwrap().into_array(0).unwrap().len(), 0);
            let actual = ScalarValue::try_from_array(&output, 0).unwrap();
            let actual = if domain::descriptor(&actual.data_type()).is_some() { domain::storage(&actual).unwrap() } else { actual };
            match value.component(key) {
                Value::Null => assert!(actual.is_null(), "{kind} {key}"),
                Value::String(text) => assert_eq!(actual, ScalarValue::Utf8(Some(text))),
                Value::Int(n) | Value::Long(n) => match actual {
                    ScalarValue::Int64(Some(actual)) => assert_eq!(actual, n),
                    ScalarValue::Decimal128(Some(actual), 38, 0) => assert_eq!(actual, i128::from(n)),
                    other => panic!("{kind} {key}: {other:?}"),
                },
                Value::BigInt(n) => assert_eq!(actual, ScalarValue::Decimal128(Some(n.to_string().parse().unwrap()), 38, 0)),
                other => panic!("unexpected component {other:?}"),
            }
        }
        for (rhs, operations) in [
            (Value::Temporal(crate::ir::temporal::parse("duration", "P1M-2DT0.000000002S").unwrap()), vec![BinaryOp::Add, BinaryOp::Sub]),
            (Value::Float(-0.5), vec![BinaryOp::Mul, BinaryOp::Div]),
            (Value::Int(2), vec![BinaryOp::Mul, BinaryOp::Div]),
        ] {
            if kind != "duration" && !matches!(rhs, Value::Temporal(_)) { continue; }
            let right_scalar = match &rhs {
                Value::Temporal(v) => domain::scalar("cypher_temporal_duration", ScalarValue::Utf8(Some(v.encode()))).unwrap(),
                Value::Float(n) => ScalarValue::Float64(Some(*n)),
                Value::Int(n) => ScalarValue::Int64(Some(*n)), _ => unreachable!(),
            };
            let schema = Arc::new(arrow::datatypes::Schema::new(vec![Field::new("a", scalar.data_type(), true), Field::new("b", right_scalar.data_type(), true)]));
            let plan = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
                produce_one_row: false, schema: Arc::new(datafusion::common::DFSchema::try_from(schema.as_ref().clone()).unwrap()),
            });
            let batch = RecordBatch::try_new(schema.clone(), vec![
                ScalarValue::iter_to_array(vec![scalar.clone(), ScalarValue::try_from(&scalar.data_type()).unwrap(), scalar.clone()]).unwrap(),
                ScalarValue::iter_to_array(vec![right_scalar.clone(), right_scalar.clone(), ScalarValue::try_from(&right_scalar.data_type()).unwrap()]).unwrap(),
            ]).unwrap();
            for op in operations {
                for swapped in [false, true] {
                    if swapped && !matches!(op, BinaryOp::Add | BinaryOp::Mul) { continue; }
                    let left = IrExpr::binding("a");
                    let right = if matches!(rhs, Value::Temporal(_)) { IrExpr::binding("b") } else { IrExpr::Lit(Lit::Scalar(right_scalar.clone())) };
                    let (left, right) = if swapped { (right, left) } else { (left, right) };
                    let expr = arithmetic(&context, &plan, op, &left, &right).unwrap_or_else(|| panic!("{kind} {op:?} swapped={swapped} should lower"));
                    let physical = datafusion_physical_expr::create_physical_expr(&expr, plan.schema(), &ExecutionProps::new()).unwrap();
                    let result = physical.evaluate(&batch).unwrap().into_array(3).unwrap();
                    let (a, b) = if swapped { (rhs.clone(), Value::Temporal(value.clone())) } else { (Value::Temporal(value.clone()), rhs.clone()) };
                    let Value::Temporal(expected) = crate::ir::temporal::arithmetic(op, &a, &b).unwrap() else { panic!() };
                    assert_eq!(transport::decode(&ScalarValue::try_from_array(&result, 0).unwrap()).unwrap(), expected);
                    assert!(ScalarValue::try_from_array(&result, 1).unwrap().is_null());
                    let last = ScalarValue::try_from_array(&result, 2).unwrap();
                    if matches!(rhs, Value::Temporal(_)) { assert!(last.is_null()); } else { assert_eq!(transport::decode(&last).unwrap(), expected); }
                    assert_eq!(physical.evaluate(&RecordBatch::new_empty(schema.clone())).unwrap().into_array(0).unwrap().len(), 0);
                    let Expr::ScalarFunction(call) = &expr else { panic!("expected typed arithmetic function") };
                    let function = call.func.inner().as_any().downcast_ref::<LogicalFunction>().unwrap();
                    let template = &function.sql_mapping("duckdb", 2).unwrap().value;
                    let argument_sql = |arg: &Expr| match arg {
                        Expr::Column(c) if c.name == "a" => quote(&value.encode()),
                        Expr::Column(c) if c.name == "b" => match &rhs { Value::Temporal(v) => quote(&v.encode()), _ => unreachable!() },
                        Expr::Literal(ScalarValue::Float64(Some(n)), _) => n.to_string(),
                        Expr::Literal(ScalarValue::Int64(Some(n)), _) => n.to_string(),
                        other => panic!("unexpected normalized operand {other:?}"),
                    };
                    let sql = template.replace("__arg0", &argument_sql(&call.args[0])).replace("__arg1", &argument_sql(&call.args[1]));
                    let actual: String = conn.query_row(&format!("SELECT CAST(({sql}) AS VARCHAR)"), [], |row| row.get(0))
                        .unwrap_or_else(|e| panic!("{kind} {op:?} swapped={swapped}: {e}\n{sql}"));
                    assert_eq!(actual, expected.encode(), "{kind} {op:?} swapped={swapped}");
                }
            }
        }
    }
}

#[test]
fn temporal_order_keys_match_golden_components_in_native_and_sql_execution() {
    use datafusion::logical_expr::execution_props::ExecutionProps;
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None, graph: &graph, options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(), language: Language::Cypher,
        scan_counter: 0, correlate_plan: None, rdf_typed_terms_used: false,
        scan_logical_labels: false, gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(), unordered_distincts: Default::default(),
    };
    let conn = duckdb::Connection::open_in_memory().unwrap();
    for (kind, text, components, zone) in [
        ("date", "2000-02-29", vec![2000,2,29], None),
        ("date", "-1000000-12-31", vec![-1000000,12,31], None),
        ("date", "+1000000-01-01", vec![1000000,1,1], None),
        ("localdatetime", "2000-02-29T23:59:59.999999999", vec![2000,2,29,86399,999999999], None),
        ("localdatetime", "-1000000-01-01T00:00:00.000000001", vec![-1000000,1,1,0,1], None),
        ("localtime", "00:00:00.000000001", vec![0,1], None),
        ("localtime", "23:59:59.999999999", vec![86399,999999999], None),
        ("time", "00:00:00.000000001-00:01:02", vec![62,1,-62], None),
        ("time", "00:00:00+01:00", vec![-3600,0,3600], None),
        ("datetime", "1969-12-31T23:59:59.999999999Z", vec![-1,999999999,0], Some("")),
        ("datetime", "1970-01-01T01:00:00+01:00", vec![0,0,3600], Some("")),
        ("datetime", "1970-01-01T01:00:00+01:00[Europe/Stockholm]", vec![0,0,3600], Some("Europe/Stockholm")),
    ] {
        let value = crate::ir::temporal::parse(kind, text).unwrap();
        let scalar = domain::scalar(&format!("cypher_temporal_{kind}"), ScalarValue::Utf8(Some(value.encode()))).unwrap();
        let schema = Arc::new(arrow::datatypes::Schema::new(vec![Field::new("value", scalar.data_type(), true)]));
        let plan = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
            produce_one_row: false, schema: Arc::new(datafusion::common::DFSchema::try_from(schema.as_ref().clone()).unwrap()),
        });
        let null = ScalarValue::try_from(&scalar.data_type()).unwrap();
        let batch = RecordBatch::try_new(schema.clone(), vec![ScalarValue::iter_to_array(vec![scalar.clone(), null.clone()]).unwrap()]).unwrap();
        for wrapped in [false, true] {
            let input = IrExpr::binding("value");
            let input = if wrapped { IrExpr::Call {name: "cypher_order_key".into(), args: vec![input]} } else { input };
            let keys = order_keys(&context, &plan, &input).unwrap();
            assert_eq!(keys.len(), if zone.is_some() {2} else {1});
            for (index, key) in keys.iter().enumerate() {
                let expected = if index == 0 {
                    ScalarValue::List(ScalarValue::new_list_nullable(&components.iter().map(|v| ScalarValue::Int64(Some(*v))).collect::<Vec<_>>(), &DataType::Int64))
                } else { ScalarValue::Utf8(Some(zone.unwrap().into())) };
                let physical = datafusion_physical_expr::create_physical_expr(key, plan.schema(), &ExecutionProps::new()).unwrap();
                let output = physical.evaluate(&batch).unwrap().into_array(2).unwrap();
                assert_eq!(ScalarValue::try_from_array(&output, 0).unwrap(), expected, "{kind} {text}");
                assert!(ScalarValue::try_from_array(&output, 1).unwrap().is_null());
                assert_eq!(physical.evaluate(&RecordBatch::new_empty(schema.clone())).unwrap().into_array(0).unwrap().len(), 0);
                let Expr::ScalarFunction(call) = key else { panic!() };
                // Constant inputs exercise the scalar UDF path as well as batches.
                for constant in [scalar.clone(), null.clone()] {
                    let expr = call.func.call(vec![lit(constant.clone())]);
                    let actual = datafusion_physical_expr::create_physical_expr(&expr, plan.schema(), &ExecutionProps::new()).unwrap()
                        .evaluate(&batch).unwrap().into_array(2).unwrap();
                    let actual = ScalarValue::try_from_array(&actual, 0).unwrap();
                    if constant.is_null() { assert!(actual.is_null()); } else { assert_eq!(actual, expected); }
                }
                let function = call.func.inner().as_any().downcast_ref::<LogicalFunction>().unwrap();
                let template = &function.sql_mapping("duckdb", 1).unwrap().value;
                let sql = template.replace("__arg0", &quote(&value.encode()));
                let actual: String = conn.query_row(&format!("SELECT CAST(to_json(({sql})) AS VARCHAR)"), [], |row| row.get(0)).unwrap();
                let expected_json = if index == 0 { serde_json::json!(components) } else { serde_json::json!(zone.unwrap()) };
                assert_eq!(serde_json::from_str::<serde_json::Value>(&actual).unwrap(), expected_json, "{kind} {text}");
                let null_sql = template.replace("__arg0", "CAST(NULL AS VARCHAR)");
                let is_null: bool = conn.query_row(&format!("SELECT ({null_sql}) IS NULL"), [], |row| row.get(0)).unwrap();
                assert!(is_null);
            }
        }
    }
    for ty in [DataType::Date32, DataType::Int64, domain::data_type("cypher_temporal_duration", DataType::Utf8)] {
        let schema = arrow::datatypes::Schema::new(vec![Field::new("value", ty.clone(), true)]);
        let plan = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation { produce_one_row: false, schema: Arc::new(datafusion::common::DFSchema::try_from(schema).unwrap()) });
        let result = order_keys(&context, &plan, &IrExpr::binding("value"));
        if ty == DataType::Date32 { assert_eq!(result, Some(vec![datafusion::prelude::col("value")])); }
        else { assert!(result.is_none()); }
        assert!(order_keys(&context, &plan, &IrExpr::binding("missing")).is_none());
    }
}

#[test]
fn temporal_component_constructors_preserve_field_order_and_decline_unsupported_maps() {
    use datafusion::logical_expr::execution_props::ExecutionProps;
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None, graph: &graph, options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(), language: Language::Cypher,
        scan_counter: 0, correlate_plan: None, rdf_typed_terms_used: false,
        scan_logical_labels: false, gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(), unordered_distincts: Default::default(),
    };
    let plan = LogicalPlanBuilder::empty(true).build().unwrap();
    let batch = RecordBatch::new_empty(Arc::new(arrow::datatypes::Schema::empty()));
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let map = |fields: Vec<(&str, Lit)>| IrExpr::Call { name: "map".into(), args: fields.into_iter().flat_map(|(k,v)| [IrExpr::Lit(Lit::String(k.into())), IrExpr::Lit(v)]).collect() };
    for (kind, fields, expected_text) in [
        ("date", vec![("day",Lit::Int(29)),("year",Lit::Int(2000)),("month",Lit::Int(2))], "2000-02-29"),
        ("localtime", vec![("nanosecond",Lit::Int(4)),("second",Lit::Int(3)),("minute",Lit::Int(2)),("hour",Lit::Int(1))], "01:02:03.000000004"),
        ("localtime", vec![("hour",Lit::Int(1))], "01:00"),
        ("localtime", vec![("microsecond",Lit::Int(123)),("millisecond",Lit::Int(456))], "00:00:00.456123"),
        ("time", vec![("timezone",Lit::String("+01:00".into())),("hour",Lit::Int(1))], "01:00+01:00"),
        ("time", vec![("hour",Lit::Int(1)),("timezone",Lit::String("Z".into()))], "01:00Z"),
        ("localdatetime", vec![("year",Lit::Int(2000)),("month",Lit::Int(2)),("day",Lit::Int(29)),("nanosecond",Lit::Int(1))], "2000-02-29T00:00:00.000000001"),
        ("datetime", vec![("timezone",Lit::String("-00:01:02".into())),("year",Lit::Int(2000))], "2000-01-01T00:00-00:01:02"),
        ("duration", vec![("nanoseconds",Lit::Int(-1)),("days",Lit::Int(-2)),("months",Lit::Int(-3))], "P-3M-2DT-0.000000001S"),
        ("duration", vec![("years",Lit::Int(1)),("months",Lit::Int(2)),("weeks",Lit::Int(3)),("days",Lit::Int(4)),("hours",Lit::Int(5)),("minutes",Lit::Int(6)),("seconds",Lit::Int(7)),("milliseconds",Lit::Int(8)),("microseconds",Lit::Int(9)),("nanoseconds",Lit::Int(10))], "P1Y2M25DT5H6M7.008009010S"),
    ] {
        let expected = crate::ir::temporal::parse(kind, expected_text).unwrap();
        for reverse in [false,true] {
            let mut fields = fields.clone();
            if reverse { fields.reverse(); }
            let expr = constructor(&context, &plan, &format!("cypher_temporal.{kind}"), &[map(fields)])
                .unwrap_or_else(|| panic!("{kind} {expected_text} should lower"));
            let output = datafusion_physical_expr::create_physical_expr(&expr, plan.schema(), &ExecutionProps::new()).unwrap()
                .evaluate(&batch).unwrap().into_array(1).unwrap();
            assert_eq!(transport::decode(&ScalarValue::try_from_array(&output, 0).unwrap()).unwrap(), expected);
            let Expr::ScalarFunction(call) = expr else { panic!() };
            let function = call.func.inner().as_any().downcast_ref::<LogicalFunction>().unwrap();
            let mut sql = function.sql_mapping("duckdb", call.args.len()).unwrap().value.clone();
            for (i, arg) in call.args.iter().enumerate().rev() {
                let replacement = match arg {
                    Expr::Literal(ScalarValue::Int64(Some(n)), _) => n.to_string(),
                    Expr::Literal(ScalarValue::Utf8(Some(s)), _) => quote(s),
                    other => panic!("unexpected constructor argument {other:?}"),
                };
                sql = sql.replace(&format!("__arg{i}"), &replacement);
            }
            let actual: String = conn.query_row(&format!("SELECT CAST(({sql}) AS VARCHAR)"), [], |row| row.get(0)).unwrap();
            assert_eq!(actual, if kind == "date" {expected.to_string()} else {expected.encode()}, "{kind} reverse={reverse}");
        }
    }
    for (kind, input) in [
        ("unknown", map(vec![])),
        ("date", IrExpr::Lit(Lit::String("2000-01-01".into()))),
        ("date", IrExpr::Call {name:"list".into(),args:vec![]}),
        ("date", IrExpr::Call {name:"map".into(),args:vec![IrExpr::Lit(Lit::String("year".into()))]}),
        ("date", IrExpr::Call {name:"map".into(),args:vec![IrExpr::Lit(Lit::Int(1)),IrExpr::Lit(Lit::Int(2000))]}),
        ("date", map(vec![("year",Lit::Scalar(ScalarValue::Float64(Some(2000.5))))])),
        ("date", map(vec![("year",Lit::Int(2000)),("month",Lit::Int(13))])),
        ("date", map(vec![("year",Lit::Int(1000000))])),
        ("date", map(vec![("year",Lit::Int(2000)),("ordinalDay",Lit::Int(60))])),
        ("time", map(vec![("hour",Lit::Int(1))])),
        ("time", map(vec![("hour",Lit::Int(1)),("timezone",Lit::String("+0100".into()))])),
        ("datetime", map(vec![("year",Lit::Int(2000)),("timezone",Lit::String("Europe/Stockholm".into()))])),
    ] {
        assert!(constructor(&context, &plan, &format!("cypher_temporal.{kind}"), &[input.clone()]).is_none(), "{kind} {input:?}");
    }
    assert!(constructor(&context, &plan, "cypher_temporal.date", &[]).is_none());
    assert!(constructor(&context, &plan, "date", &[]).is_none());
}
