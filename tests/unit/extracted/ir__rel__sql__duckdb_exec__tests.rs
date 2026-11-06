
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

#[test]
fn file_backed_executors_share_committed_data_and_survive_reopening() {
    let path=std::env::temp_dir().join(format!("orchid-coverage-file-session-{}-{}.duckdb",std::process::id(),NEXT_SCAN.fetch_add(1,Ordering::Relaxed)));
    {
        let mut first=DuckDbExecutor::open(&path).unwrap();
        first.run(&["CREATE TABLE persistent_values(v BIGINT)".into(),"INSERT INTO persistent_values VALUES (7), (NULL)".into()],"SELECT 1").unwrap();
        let mut second=DuckDbExecutor::open(&path).unwrap();
        assert_eq!(second.run(&[],"SELECT v FROM persistent_values ORDER BY v NULLS LAST").unwrap(),vec![vec![SqlValue::Int(7)],vec![SqlValue::Null]]);
        first.begin().unwrap();
        first.run(&[],"INSERT INTO persistent_values VALUES (9)").unwrap();
        first.rollback().unwrap();
    }
    {
        let mut reopened=DuckDbExecutor::open(&path).unwrap();
        assert_eq!(reopened.run(&[],"SELECT count(*) FROM persistent_values").unwrap(),vec![vec![SqlValue::Int(2)]]);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn lost_session_never_silently_opens_an_empty_replacement_database() {
    let mut executor=DuckDbExecutor::new();
    executor.lost_connection=true;
    assert!(executor.connection.is_none());
    for _ in 0..2 {
        let error=executor.run(&[],"SELECT 1").unwrap_err().to_string();
        assert!(error.contains("session was lost after interruption"),"{error}");
        assert!(executor.connection.is_none());
    }
}

#[test]
fn arrow_query_errors_identify_prepare_and_execution_without_poisoning_connection() {
    let connection=Connection::open_in_memory().unwrap();
    let error=execute_arrow(&connection,"SELECT FROM").unwrap_err().to_string();
    assert!(error.contains("duckdb prepare:"),"{error}");
    connection.execute_batch("CREATE TABLE invalid_values(value VARCHAR); INSERT INTO invalid_values VALUES ('not-an-integer')").unwrap();
    let error=execute_arrow(&connection,"SELECT CAST(value AS BIGINT) FROM invalid_values").unwrap_err().to_string();
    assert!(error.contains("duckdb query:"),"{error}");
    let empty=execute_arrow(&connection,"SELECT CAST(NULL AS BIGINT) AS value WHERE FALSE").unwrap();
    assert_eq!(empty.num_rows(),0);assert_eq!(empty.num_columns(),1);
    assert_eq!(empty.schema().field(0).name(),"value");
    assert_eq!(empty.schema().field(0).data_type(),&arrow::datatypes::DataType::Int64);
    let recovered=execute_arrow(&connection,"SELECT 7::BIGINT AS value").unwrap();
    assert_eq!(recovered.num_rows(),1);
    assert_eq!(recovered.column(0).as_any().downcast_ref::<arrow::array::Int64Array>().unwrap().value(0),7);
}

#[test]
fn failed_setup_blocks_roll_back_only_their_owned_transaction() {
    for atomic in [false,true] {
        let connection=Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE persistent_values(v BIGINT); INSERT INTO persistent_values VALUES (11)").unwrap();
        if !atomic {connection.execute_batch("BEGIN TRANSACTION").unwrap();}
        let error=execute_setup_block(&connection,&[
            "INSERT INTO persistent_values VALUES (22)".into(),
            "INSERT INTO missing_setup_table VALUES (33)".into(),
        ],atomic).unwrap_err().to_string();
        assert!(error.contains("duckdb setup:")&&error.contains("statement: INSERT INTO missing_setup_table"),"{error}");
        if !atomic {connection.execute_batch("ROLLBACK").unwrap();}
        let values=connection.query_row("SELECT list(v ORDER BY v)::VARCHAR FROM persistent_values",[],|row|row.get::<_,String>(0)).unwrap();
        assert_eq!(values,"[11]");
        execute_setup_block(&connection,&["INSERT INTO persistent_values VALUES (44)".into()],true).unwrap();
        assert_eq!(connection.query_row("SELECT sum(v)::BIGINT FROM persistent_values",[],|row|row.get::<_,i64>(0)).unwrap(),55);
    }
}

#[test]
fn setup_transaction_boundary_errors_leave_connection_recoverable() {
    let connection=Connection::open_in_memory().unwrap();
    connection.execute_batch("BEGIN TRANSACTION").unwrap();
    let error=execute_setup_block(&connection,&[],true).unwrap_err().to_string();
    assert!(error.contains("duckdb setup begin:"),"{error}");
    connection.execute_batch("ROLLBACK").unwrap();
    let error=execute_setup_block(&connection,&["COMMIT".into()],true).unwrap_err().to_string();
    assert!(error.contains("duckdb setup commit:"),"{error}");
    execute_setup_block(&connection,&["CREATE TABLE recovered(v BIGINT)".into(),"INSERT INTO recovered VALUES (5)".into()],true).unwrap();
    assert_eq!(connection.query_row("SELECT v FROM recovered",[],|row|row.get::<_,i64>(0)).unwrap(),5);
}
