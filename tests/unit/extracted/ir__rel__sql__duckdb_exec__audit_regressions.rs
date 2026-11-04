
    use super::*;
    use arrow::array::Int64Array;
    #[test]
    fn audit_scan_collision_preserves_caller_table() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            arrow::datatypes::DataType::Int64,
            false,
        )]));
        let table = TableData {
            name: "audit_scan".into(),
            schema: schema.clone(),
            batches: vec![
                RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![7]))]).unwrap(),
            ],
        };
        for temporary in [false, true] {
            let conn = Connection::open_in_memory().unwrap();
            let key = SqlDialect::DuckDb.quote_ident(&table.content_key());
            conn.execute_batch(&format!(
                "CREATE {} TABLE {key} (value BIGINT); INSERT INTO {key} VALUES (42)",
                if temporary { "TEMP" } else { "" }
            ))
            .unwrap();
            let mut executor = DuckDbExecutor::from_connection(conn);
            assert_eq!(
                executor
                    .run_with_tables(
                        std::slice::from_ref(&table),
                        &[],
                        "SELECT value FROM audit_scan"
                    )
                    .unwrap(),
                vec![vec![SqlValue::Int(7)]]
            );
            assert_eq!(
                executor
                    .run(&[], &format!("SELECT value FROM {key}"))
                    .unwrap(),
                vec![vec![SqlValue::Int(42)]],
                "temporary={temporary}"
            );
        }
    }
