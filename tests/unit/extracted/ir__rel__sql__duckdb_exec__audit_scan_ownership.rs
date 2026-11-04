
    use super::*;
    use arrow::array::Int64Array;
    fn table() -> TableData {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "v",
            arrow::datatypes::DataType::Int64,
            false,
        )]));
        TableData {
            name: "scan_input".into(),
            schema: schema.clone(),
            batches: vec![
                RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![7]))]).unwrap(),
            ],
        }
    }
    #[test]
    fn occupied_physical_name_never_enters_replacement_fallback() {
        for atomic in [false, true] {
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch(
                "CREATE TEMP TABLE occupied (v BIGINT); INSERT INTO occupied VALUES (42)",
            )
            .unwrap();
            assert!(load_scan_table(&conn, "occupied", &table(), atomic).is_err());
            assert_eq!(
                conn.query_row("SELECT v FROM occupied", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                42
            );
        }
    }
    #[test]
    fn transaction_scans_are_released_after_success_and_failure() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("BEGIN").unwrap();
        let mut executor = DuckDbExecutor::from_connection(conn);
        for query in ["SELECT v FROM scan_input", "SELECT missing FROM scan_input"] {
            let result = executor.run_with_tables(&[table()], &[], query);
            assert_eq!(result.is_ok(), !query.contains("missing"));
            let conn = executor.connection.as_ref().unwrap();
            assert_eq!(conn.query_row("SELECT count(*) FROM duckdb_tables() WHERE starts_with(table_name, '__orchid_scan_')", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
        }
        executor.rollback().unwrap();
    }
