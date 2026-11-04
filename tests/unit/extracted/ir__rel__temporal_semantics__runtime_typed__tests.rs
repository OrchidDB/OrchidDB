
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
