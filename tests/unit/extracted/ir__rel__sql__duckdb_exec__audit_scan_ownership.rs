
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

#[test]
fn prepared_arrow_results_preserve_values_and_release_aliases_after_errors() {
    for transaction in [false,true] {
        let mut executor=DuckDbExecutor::from_connection(Connection::open_in_memory().unwrap());
        if transaction {executor.begin().unwrap();}
        for (query,valid) in [("SELECT v FROM scan_input",true),("SELECT missing FROM scan_input",false),
            ("SELECT v FROM scan_input WHERE false",true)] {
            let input=table();
            let mut prepared=super::super::PreparedSql::query_only(query.into(),input.schema.clone());
            prepared.tables=if transaction {vec![input]} else {vec![input.clone(),input]};
            // Repeated logical scans reuse the same content and alias.
            let result=executor.execute_prepared_arrow(&prepared);
            if valid {
                let output=result.unwrap();
                assert_eq!(output.num_rows(),if query.contains("false") {0} else {1});
                assert_eq!(output.schema().field(0).data_type(),&arrow::datatypes::DataType::Int64);
                if output.num_rows()>0 {assert_eq!(output.column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0),7);}
            } else {assert!(result.unwrap_err().to_string().contains("missing"));}
            assert!(executor.scan_views.is_empty());
            assert!(executor.transient_scans.is_empty());
        }
        if transaction {executor.rollback().unwrap();}
    }
}

#[test]
fn prepared_arrow_alias_conflict_cleans_up_and_allows_subsequent_execution() {
    let mut executor=DuckDbExecutor::from_connection(Connection::open_in_memory().unwrap());
    let first=table();
    let mut second=table();
    second.batches=vec![RecordBatch::try_new(second.schema.clone(),vec![Arc::new(Int64Array::from(vec![9]))]).unwrap()];
    let mut prepared=super::super::PreparedSql::query_only("SELECT v FROM scan_input".into(),first.schema.clone());
    prepared.tables=vec![first.clone(),second];
    assert!(executor.execute_prepared_arrow(&prepared).unwrap_err().to_string().contains("conflicting scan alias"));
    assert!(executor.scan_views.is_empty());
    prepared.tables=vec![first];
    let output=executor.execute_prepared_arrow(&prepared).unwrap();
    assert_eq!(output.column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0),7);
}

#[test]
fn stream_setup_applies_ddl_and_releases_scan_aliases() {
    let mut executor=DuckDbExecutor::from_connection(Connection::open_in_memory().unwrap());
    let input=table();
    let mut prepared=super::super::PreparedSql::query_only("SELECT v FROM scan_input".into(),input.schema.clone());
    prepared.tables=vec![input];
    prepared.setup=vec!["CREATE TEMP TABLE setup_marker AS SELECT 42 AS v".into()];
    executor.prepare_stream_setup(&prepared).unwrap();
    let conn=executor.connection.as_ref().unwrap();
    assert_eq!(conn.query_row("SELECT v FROM setup_marker",[],|r|r.get::<_,i64>(0)).unwrap(),42);
    assert_eq!(conn.query_row(&prepared.query,[],|r|r.get::<_,i64>(0)).unwrap(),7);
    executor.finish_stream_setup().unwrap();
    assert!(executor.scan_views.is_empty());
}

#[test]
fn scan_cache_eviction_drops_old_unused_tables_and_preserves_current_inputs() {
    let mut executor=DuckDbExecutor::from_connection(Connection::open_in_memory().unwrap());
    let mut entries=vec![];
    for value in [1,2,3] {
        let mut input=table();
        input.name=format!("input_{value}");
        input.batches=vec![RecordBatch::try_new(input.schema.clone(),vec![Arc::new(Int64Array::from(vec![value]))]).unwrap()];
        let key=executor.ensure_scan_table(&input,true).unwrap();
        let entry=executor.scan_tables.get_mut(&key).unwrap();
        // Exercise the cache policy independently of allocating gigabytes.
        entry.bytes=DEFAULT_SCAN_CACHE_BYTES;
        entries.push((key,entry.name.clone()));
    }
    executor.clear_query_scans().unwrap();
    let current=BTreeSet::from([entries[2].0.clone()]);
    executor.evict_scan_tables(&current);
    assert_eq!(executor.scan_tables.keys().cloned().collect::<BTreeSet<_>>(),current);
    let connection=executor.connection.as_ref().unwrap();
    for (index,(_,name)) in entries.iter().enumerate() {
        assert_eq!(scan_object_oid(connection,name,false).unwrap().is_some(),index==2);
    }
    executor.evict_scan_tables(&BTreeSet::new());
    assert_eq!(executor.scan_tables.len(),1,"within-budget entries must remain cached");
}
