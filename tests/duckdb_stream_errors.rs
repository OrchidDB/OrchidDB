#![cfg(feature = "duckdb")]
use arrow::datatypes::{DataType, Field, Schema};
use std::sync::Arc;
#[test]
fn late_query_error_is_not_end_of_stream() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("SET threads=1").unwrap();
    let mut stmt=db.prepare("SELECT CAST(CASE WHEN i < 131072 THEN CAST(i AS VARCHAR) ELSE 'not-an-int' END AS BIGINT) AS x FROM range(262144) t(i)").unwrap();
    let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, true)]));
    let mut stream = stmt.stream_arrow([], schema).unwrap();
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
