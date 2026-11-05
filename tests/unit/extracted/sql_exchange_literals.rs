use super::super::*;
use serde_json::{Value, json};

fn scalar_cases() -> Vec<(ScalarValue, Value)> {
    vec![
        (ScalarValue::Boolean(Some(false)), json!(false)),
        (ScalarValue::Boolean(None), Value::Null),
        (ScalarValue::Int8(Some(i8::MIN)), json!(i8::MIN)),
        (ScalarValue::Int16(Some(i16::MAX)), json!(i16::MAX)),
        (ScalarValue::Int32(Some(i32::MIN)), json!(i32::MIN)),
        (ScalarValue::Int64(Some(i64::MAX)), json!(i64::MAX)),
        (ScalarValue::UInt8(Some(u8::MAX)), json!(u8::MAX)),
        (ScalarValue::UInt16(Some(u16::MAX)), json!(u16::MAX)),
        (ScalarValue::UInt32(Some(u32::MAX)), json!(u32::MAX)),
        (ScalarValue::UInt64(Some(u64::MAX)), json!(u64::MAX)),
        (ScalarValue::Float32(Some(1.25)), json!(1.25)),
        (ScalarValue::Float64(Some(-2.5)), json!(-2.5)),
        (
            ScalarValue::Decimal128(Some(99999999999999999999999999999999999999), 38, 0),
            json!("99999999999999999999999999999999999999"),
        ),
        (
            ScalarValue::Decimal128(Some(-12345), 20, 4),
            json!("-1.2345"),
        ),
        (ScalarValue::Decimal128(None, 20, 4), Value::Null),
    ]
}
fn text_cases() -> Vec<(ScalarValue, Value)> {
    [None, Some(""), Some("雪'\\\n\t\"é")]
        .into_iter()
        .flat_map(|value| {
            [
                ScalarValue::Utf8(value.map(str::to_owned)),
                ScalarValue::LargeUtf8(value.map(str::to_owned)),
                ScalarValue::Utf8View(value.map(str::to_owned)),
            ]
            .into_iter()
            .map(move |scalar| (scalar, json!(value)))
        })
        .collect()
}
fn binary_cases() -> Vec<(ScalarValue, Value)> {
    [None, Some(vec![0, 39, 92, 255])]
        .into_iter()
        .flat_map(|bytes| {
            let expected = if bytes.is_some() {
                json!("00275cff")
            } else {
                Value::Null
            };
            [
                ScalarValue::Binary(bytes.clone()),
                ScalarValue::LargeBinary(bytes.clone()),
                ScalarValue::BinaryView(bytes.clone()),
                ScalarValue::FixedSizeBinary(4, bytes),
            ]
            .into_iter()
            .map(move |value| (value, expected.clone()))
        })
        .collect()
}
fn list_cases() -> Vec<(ScalarValue, Value)> {
    use arrow::{
        array::{FixedSizeListArray, LargeListArray},
        datatypes::Int64Type,
    };
    let items = vec![
        ScalarValue::Int64(Some(7)),
        ScalarValue::Int64(None),
        ScalarValue::Int64(Some(7)),
    ];
    let standard = ScalarValue::List(ScalarValue::new_list(&items, &DataType::Int64, true));
    let large = ScalarValue::LargeList(Arc::new(LargeListArray::from_iter_primitive::<
        Int64Type,
        _,
        _,
    >([Some(vec![Some(7), None, Some(7)])])));
    let fixed = ScalarValue::FixedSizeList(Arc::new(FixedSizeListArray::from_iter_primitive::<
        Int64Type,
        _,
        _,
    >([Some(vec![Some(7), None, Some(7)])], 3)));
    let mut cases = vec![];
    for value in [standard, large, fixed] {
        cases.push((
            ScalarValue::try_from(&value.data_type()).unwrap(),
            Value::Null,
        ));
        cases.push((value, json!([7, null, 7])));
    }
    cases.push((
        ScalarValue::List(ScalarValue::new_list(&[], &DataType::Int64, true)),
        json!([]),
    ));
    cases
}
fn nested_cases() -> Vec<(ScalarValue, Value)> {
    let empty = ScalarValue::List(ScalarValue::new_list(&[], &DataType::Int64, true));
    let values = ScalarValue::List(ScalarValue::new_list(
        &[ScalarValue::Int64(Some(7)), ScalarValue::Int64(None)],
        &DataType::Int64,
        true,
    ));
    let kind = empty.data_type();
    let null = ScalarValue::try_from(&kind).unwrap();
    let value = ScalarValue::List(ScalarValue::new_list(&[null, empty, values], &kind, true));
    vec![(value, json!([null, [], [7, null]]))]
}
fn json_query(value: &ScalarValue, dialect: SqlDialect) -> String {
    let mut expression = exchange_literal(value.clone(), value.data_type(), dialect)
        .unwrap_or_else(|error| panic!("{dialect:?} {value:?}: {error}"));
    match value.data_type() {
        DataType::Binary
        | DataType::LargeBinary
        | DataType::BinaryView
        | DataType::FixedSizeBinary(_) => {
            expression = match dialect {
                SqlDialect::DuckDb => format!("lower(hex({expression}))"),
                SqlDialect::Postgres => format!("encode({expression}, 'hex')"),
                SqlDialect::Custom(_) => format!("lower(to_hex({expression}))"),
            };
        }
        DataType::Decimal128(_, _) => expression = format!("CAST({expression} AS VARCHAR)"),
        _ => {}
    }
    let json = match dialect {
        SqlDialect::DuckDb => format!("CAST(to_json({expression}) AS VARCHAR)"),
        SqlDialect::Postgres => format!("CAST(to_json({expression}) AS TEXT)"),
        SqlDialect::Custom(_) => format!("json_format(CAST({expression} AS JSON))"),
    };
    format!("SELECT coalesce({json}, 'null') AS payload")
}
fn check_duckdb(cases: Vec<(ScalarValue, Value)>) {
    let mut executor = DuckDbExecutor::default();
    for (value, expected) in cases {
        let query = json_query(&value, SqlDialect::DuckDb);
        let rows = executor
            .run(&[], &query)
            .unwrap_or_else(|error| panic!("{value:?}: {query}: {error}"));
        let [row] = rows.as_slice() else {
            panic!("{rows:?}")
        };
        let [SqlValue::Text(text)] = row.as_slice() else {
            panic!("{row:?}")
        };
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            expected,
            "{value:?}: {query}"
        );
    }
}
#[test]
fn exchange_scalar_literals_preserve_integer_limits_and_decimal_precision() {
    check_duckdb(scalar_cases());
}
#[test]
fn exchange_text_literals_preserve_quotes_unicode_control_characters_and_nulls() {
    check_duckdb(text_cases());
}
#[test]
fn exchange_binary_literals_preserve_every_byte_and_null_storage() {
    check_duckdb(binary_cases());
}
#[test]
fn exchange_list_literals_preserve_offsets_null_parents_empty_lists_and_duplicates() {
    check_duckdb(list_cases());
}
#[test]
fn exchange_nested_list_literals_keep_null_children_distinct_from_empty_lists() {
    check_duckdb(nested_cases());
}

#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires PostgreSQL and Trino"]
async fn exchange_literals_execute_consistently_on_postgres_duckdb_and_trino() {
    use crate::execution::trino::{TrinoOptions, TrinoSession};
    use futures::StreamExt;
    let pg = std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL");
    let endpoint = std::env::var("ORCHIDDB_TEST_TRINO_URL")
        .or_else(|_| std::env::var("ORCHIDDB_TEST_OPA_URL"))
        .expect("Trino URL");
    let trino = TrinoSession::source(TrinoOptions::new(endpoint, "orchid_test")).unwrap();
    let cases = scalar_cases()
        .into_iter()
        .chain(text_cases())
        .chain(binary_cases())
        .chain(list_cases())
        .chain(nested_cases())
        .collect::<Vec<_>>();
    check_duckdb(cases.clone());
    for (value, expected) in cases {
        let query = json_query(&value, SqlDialect::Postgres);
        let output = std::process::Command::new("psql")
            .args([&pg, "-X", "-A", "-t", "-v", "ON_ERROR_STOP=1", "-c", &query])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{value:?}: {query}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            expected,
            "PostgreSQL {value:?}: {query}"
        );
        let query = json_query(&value, SqlDialect::Custom(&trino::TRINO));
        let mut stream = trino.execute(&query, vec![]).await.unwrap();
        let mut rows = vec![];
        while let Some(batch) = stream.next().await {
            let batch = batch.unwrap();
            for row in 0..batch.num_rows() {
                let text =
                    arrow::util::display::array_value_to_string(batch.column(0).as_ref(), row)
                        .unwrap();
                rows.push(serde_json::from_str::<Value>(&text).unwrap());
            }
        }
        assert_eq!(rows, vec![expected], "Trino {value:?}: {query}");
    }
}

#[cfg(feature = "trino")]
async fn execute_text_case(
    query: &str,
    dialect: SqlDialect,
    pg: &str,
    trino_session: &crate::execution::trino::TrinoSession,
) -> String {
    use futures::StreamExt;
    match dialect {
        SqlDialect::DuckDb => {
            let rows = DuckDbExecutor::default().run(&[], query).unwrap();
            let [row] = rows.as_slice() else {
                panic!("{rows:?}")
            };
            let [SqlValue::Text(value)] = row.as_slice() else {
                panic!("{row:?}")
            };
            value.clone()
        }
        SqlDialect::Postgres => {
            let result = std::process::Command::new("psql")
                .args([pg, "-X", "-A", "-t", "-v", "ON_ERROR_STOP=1", "-c", query])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{query}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            String::from_utf8(result.stdout).unwrap().trim().to_owned()
        }
        SqlDialect::Custom(_) => {
            let mut stream = trino_session.execute(query, vec![]).await.unwrap();
            let mut values = vec![];
            while let Some(batch) = stream.next().await {
                let batch = batch.unwrap();
                for row in 0..batch.num_rows() {
                    values.push(
                        arrow::util::display::array_value_to_string(batch.column(0).as_ref(), row)
                            .unwrap(),
                    );
                }
            }
            assert_eq!(values.len(), 1, "{query}");
            values.remove(0)
        }
    }
}

#[cfg(feature = "trino")]
fn literal_test_engines() -> (String, crate::execution::trino::TrinoSession) {
    use crate::execution::trino::{TrinoOptions, TrinoSession};
    let pg = std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL");
    let endpoint = std::env::var("ORCHIDDB_TEST_TRINO_URL")
        .or_else(|_| std::env::var("ORCHIDDB_TEST_OPA_URL"))
        .expect("Trino URL");
    (
        pg,
        TrinoSession::source(TrinoOptions::new(endpoint, "orchid_test")).unwrap(),
    )
}

#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires PostgreSQL and Trino"]
async fn exchange_special_floats_preserve_nan_infinity_and_zero_sign_on_every_engine() {
    let (pg, trino_session) = literal_test_engines();
    for (number, expected) in [
        (Some(f64::NAN), "nan"),
        (Some(f64::INFINITY), "+inf"),
        (Some(f64::NEG_INFINITY), "-inf"),
        (Some(-0.0), "-0"),
        (Some(0.0), "+0"),
        (None, "null"),
    ] {
        for value in [
            ScalarValue::Float64(number),
            ScalarValue::Float32(number.map(|number| number as f32)),
        ] {
            for dialect in [
                SqlDialect::DuckDb,
                SqlDialect::Postgres,
                SqlDialect::Custom(&trino::TRINO),
            ] {
                let literal = exchange_literal(value.clone(), value.data_type(), dialect).unwrap();
                let (nan, infinite, negative) = match dialect {
                    SqlDialect::DuckDb => ("isnan(v)", "isinf(v)", "signbit(v)"),
                    SqlDialect::Postgres => (
                        "v = CAST('NaN' AS DOUBLE PRECISION)",
                        "abs(v) = CAST('Infinity' AS DOUBLE PRECISION)",
                        "encode(float8send(CAST(v AS DOUBLE PRECISION)), 'hex') = '8000000000000000'",
                    ),
                    SqlDialect::Custom(_) => {
                        ("is_nan(v)", "is_infinite(v)", "CAST(1 AS DOUBLE) / v < 0")
                    }
                };
                let query = format!(
                    "SELECT CASE WHEN v IS NULL THEN 'null' WHEN {nan} THEN 'nan' WHEN {infinite} AND v > 0 THEN '+inf' WHEN {infinite} AND v < 0 THEN '-inf' WHEN v = 0 THEN CASE WHEN {negative} THEN '-0' ELSE '+0' END ELSE 'finite' END FROM (SELECT {literal} AS v) t"
                );
                assert_eq!(
                    execute_text_case(&query, dialect, &pg, &trino_session).await,
                    expected,
                    "{dialect:?} {value:?}: {query}"
                );
            }
        }
    }
}

#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires PostgreSQL and Trino"]
async fn exchange_zoned_timestamp_literals_preserve_instants_before_and_after_epoch() {
    let (pg, trino_session) = literal_test_engines();
    for zone in [
        None,
        Some("UTC"),
        Some("Asia/Kolkata"),
        Some("America/New_York"),
    ] {
        for number in [None, Some(-1_234_567_i64), Some(0), Some(1_234_567)] {
            let value = ScalarValue::TimestampMicrosecond(number, zone.map(Into::into));
            for dialect in [
                SqlDialect::DuckDb,
                SqlDialect::Postgres,
                SqlDialect::Custom(&trino::TRINO),
            ] {
                let direct = exchange_literal(value.clone(), value.data_type(), dialect).unwrap();
                let encoded = unparse::encode_expression_literals(
                    datafusion::logical_expr::lit(value.clone()),
                    dialect,
                )
                .unwrap()
                .data;
                let unparser_dialect = dialect.unparser_dialect();
                let mut ast = datafusion::sql::unparser::Unparser::new(unparser_dialect.as_ref())
                    .expr_to_sql(&encoded)
                    .unwrap();
                functions::prepare_ast(&mut ast, dialect).unwrap();
                for literal in [direct, ast.to_string()] {
                    assert_eq!(
                        literal.to_ascii_uppercase().matches("CAST(").count(),
                        1,
                        "timestamp transport needs one declared-type cast: {literal}"
                    );
                    let epoch = match dialect {
                        SqlDialect::DuckDb => "epoch(v)",
                        SqlDialect::Postgres => "extract(epoch from v)",
                        SqlDialect::Custom(_) => "to_unixtime(v AT TIME ZONE 'UTC')",
                    };
                    let query = format!(
                        "SELECT coalesce(CAST({epoch} AS VARCHAR), 'null') FROM (SELECT {literal} AS v) t"
                    );
                    let actual = execute_text_case(&query, dialect, &pg, &trino_session).await;
                    match number {
                        None => assert_eq!(actual, "null", "{dialect:?} {zone:?}"),
                        Some(micros) => {
                            let actual: f64 = actual.parse().unwrap();
                            let expected = micros as f64 / 1_000_000.0;
                            assert!(
                                (actual - expected).abs() <= 1e-9,
                                "{dialect:?} {zone:?} {micros}: {actual} != {expected}: {query}"
                            );
                        }
                    }
                }
            }
        }
    }
}
