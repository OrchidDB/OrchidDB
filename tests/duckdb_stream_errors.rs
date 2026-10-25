#![cfg(feature = "duckdb")]
#[test]
fn late_query_error_is_not_end_of_stream() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("SET threads=1").unwrap();
    let mut stmt=db.prepare("SELECT CAST(CASE WHEN i < 131072 THEN CAST(i AS VARCHAR) ELSE 'not-an-int' END AS BIGINT) AS x FROM range(262144) t(i)").unwrap();
    let mut stream = stmt.stream_arrow_auto([]).unwrap();
    let mut rows = 0;
    loop {
        match stream.next_result() {
            Ok(Some(batch)) => rows += batch.num_rows(),
            Ok(None) => panic!("late conversion error was silently swallowed after {rows} rows"),
            Err(error) => {
                assert!(error.to_string().contains("not-an-int"), "{error}");
                break;
            }
        }
    }
    assert!(
        rows > 0,
        "test must exercise a failure after successful output"
    );
    assert!(stream.next_result().unwrap().is_none());
}

#[test]
fn streaming_schema_preserves_types_empty_results_and_single_execution() {
    use arrow::datatypes::DataType;
    let db=duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE SEQUENCE calls").unwrap();
    let query="SELECT nextval('calls') AS id, $1::DECIMAL(20,3) AS price, [1,NULL] AS xs, {'k':42} AS obj, TIMESTAMPTZ '2024-01-01 00:00:00+00' AS ts, 'abc'::BLOB AS bytes";
    let mut stmt=db.prepare(query).unwrap();
    let mut stream=stmt.stream_arrow_auto(["123.456"]).unwrap();
    let schema=stream.get_schema();
    assert_eq!(schema.field(1).data_type(), &DataType::Decimal128(20,3));
    assert!(matches!(schema.field(2).data_type(), DataType::List(_)));
    assert!(matches!(schema.field(3).data_type(), DataType::Struct(_)));
    assert!(matches!(schema.field(4).data_type(), DataType::Timestamp(_,Some(_))));
    let batch=stream.next_result().unwrap().unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(batch.column(0).as_ref(),0).unwrap(),"1");
    assert_eq!(arrow::util::display::array_value_to_string(batch.column(1).as_ref(),0).unwrap(),"123.456");
    assert!(stream.next_result().unwrap().is_none());
    drop(stream);drop(stmt);
    let next:i64=db.query_row("SELECT nextval('calls')",[],|r|r.get(0)).unwrap();
    assert_eq!(next,2);
    let mut stmt=db.prepare("SELECT 1::BIGINT AS n WHERE false").unwrap();
    let mut stream=stmt.stream_arrow_auto([]).unwrap();
    assert_eq!(stream.get_schema().field(0).data_type(),&DataType::Int64);
    assert!(stream.next_result().unwrap().is_none());
}
