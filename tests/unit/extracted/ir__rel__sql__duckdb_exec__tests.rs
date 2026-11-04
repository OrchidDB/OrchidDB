
    use super::*;

    #[test]
    fn interrupts_a_long_running_query() {
        let started = std::time::Instant::now();
        let mut executor = DuckDbExecutor::with_timeout(Duration::from_millis(25));
        assert_eq!(
            executor
                .run(
                    &[
                        "CREATE TABLE retained_fixture(value INTEGER)".into(),
                        "INSERT INTO retained_fixture VALUES (42)".into()
                    ],
                    "SELECT value FROM retained_fixture",
                )
                .unwrap(),
            vec![vec![SqlValue::Int(42)]]
        );
        let error = executor
            .run(
                &[],
                "SELECT SUM(a.i * b.i) FROM range(1000000) a(i), range(1000000) b(i)",
            )
            .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            executor
                .run(&[], "SELECT value FROM retained_fixture")
                .unwrap(),
            vec![vec![SqlValue::Int(42)]]
        );
    }

    #[test]
    fn interrupts_long_running_setup_and_resets_the_session() {
        let started = std::time::Instant::now();
        let mut executor = DuckDbExecutor::with_timeout(Duration::from_millis(25));
        let error = executor
            .run(
                &[
                    "CREATE TABLE oversized_setup AS SELECT i FROM range(1000000000) values_(i)"
                        .into(),
                ],
                "SELECT COUNT(*) FROM oversized_setup",
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("setup timed out"),
            "unexpected timeout phase: {error}"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            executor.run(&[], "SELECT 1").unwrap(),
            vec![vec![SqlValue::Int(1)]]
        );
    }

    #[test]
    fn returns_temporal_values_as_canonical_text() {
        let mut executor = DuckDbExecutor::new();
        assert_eq!(
            executor
                .run(
                    &[],
                    "SELECT CAST('2024-06-15 12:34:56.123456' AS TIMESTAMP), CAST('2024-06-15' AS DATE)",
                )
                .unwrap(),
            vec![vec![
                SqlValue::Text("2024-06-15 12:34:56.123456".into()),
                SqlValue::Text("2024-06-15".into()),
            ]]
        );
    }

#[test]
fn driver_temporal_units_preserve_fractional_precision_before_and_after_epoch() {
    use duckdb::types::TimeUnit;
    for (unit,value,expected) in [
        (TimeUnit::Second,-1,"1969-12-31 23:59:59"),
        (TimeUnit::Millisecond,-1,"1969-12-31 23:59:59.999"),
        (TimeUnit::Microsecond,-1,"1969-12-31 23:59:59.999999"),
        (TimeUnit::Nanosecond,-1,"1969-12-31 23:59:59.999999999"),
        (TimeUnit::Nanosecond,1,"1970-01-01 00:00:00.000000001"),
    ] {assert_eq!(convert_value(ValueRef::Timestamp(unit,value)).unwrap(),SqlValue::Text(expected.into()));}
    assert_eq!(convert_value(ValueRef::Time64(TimeUnit::Nanosecond,1)).unwrap(),SqlValue::Text("00:00:00.000000001".into()));
    for unit in [TimeUnit::Second,TimeUnit::Millisecond] {
        assert!(convert_value(ValueRef::Time64(unit,1)).unwrap_err().to_string().contains("duckdb temporal value"));
    }
    assert!(convert_value(ValueRef::Interval{months:1,days:2,nanos:3}).unwrap_err().to_string().contains("duckdb value type"));
}

#[test]
fn driver_list_offsets_preserve_null_empty_and_sliced_large_lists() {
    use arrow::array::{LargeListArray,ListArray};
    use arrow::datatypes::Int64Type;
    let values=vec![Some(vec![Some(99)]),None,Some(vec![]),Some(vec![None,Some(7)])];
    let regular=ListArray::from_iter_primitive::<Int64Type,_,_>(values.clone()).slice(1,3);
    let large=LargeListArray::from_iter_primitive::<Int64Type,_,_>(values).slice(1,3);
    for list in [ListType::Regular(&regular),ListType::Large(&large)] {
        for (row,expected) in [SqlValue::Null,SqlValue::List(vec![]),SqlValue::List(vec![SqlValue::Null,SqlValue::Int(7)])].into_iter().enumerate() {
            assert_eq!(convert_value(ValueRef::List(list,row)).unwrap(),expected);
        }
    }
}
