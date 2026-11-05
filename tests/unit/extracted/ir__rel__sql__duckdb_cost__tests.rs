
    use super::*;
    #[test]
    fn counts_each_operator_once_and_takes_peaks() {
        let leaf = ProfilingInfo {
            metrics: [
                ("OPERATOR_ROWS_SCANNED".into(), "10".into()),
                ("OPERATOR_CARDINALITY".into(), "3".into()),
            ]
            .into(),
            children: vec![],
        };
        let root = ProfilingInfo {
            metrics: [
                ("CPU_TIME".into(), "0.25".into()),
                ("CUMULATIVE_CARDINALITY".into(), "999".into()),
                ("SYSTEM_PEAK_BUFFER_MEMORY".into(), "4096".into()),
            ]
            .into(),
            children: vec![leaf],
        };
        let mut cost = read(&root);
        assert_eq!(cost.work_units(), 13);
        assert_eq!(cost.cpu_nanos, 250_000_000);
        cost.merge(&read(&root));
        assert_eq!(cost.work_units(), 26);
        assert_eq!(cost.peak_memory_bytes, 4096);
    }
    #[test]
    fn profiles_once_and_restores_disabled_setting() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE SEQUENCE calls").unwrap();
        let cost = Arc::new(Mutex::new(EngineCost::default()));
        {
            let profile = Profile::start(&conn, true, cost.clone(), "SELECT nextval('calls')")
                .unwrap()
                .unwrap();
            let value: i64 = conn
                .query_row("SELECT nextval('calls')", [], |r| r.get(0))
                .unwrap();
            assert_eq!(value, 1);
            profile.finish();
        }
        assert_eq!(cost.lock().unwrap().profiled_statements, 1);
        let next: i64 = conn
            .query_row("SELECT nextval('calls')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(next, 2);
        let setting: Option<String> = conn
            .query_row("SELECT current_setting('enable_profiling')", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(setting.is_none_or(|s| s.is_empty()));
    }

#[test]
fn metric_boundaries_reject_invalid_numbers_and_saturate_accumulated_work() {
    let info = |name: &str, value: &str| ProfilingInfo {
        metrics: [(name.to_owned(), value.to_owned())].into(), children: vec![]
    };
    for value in ["", "bad", "-1", "18446744073709551616", "1.5"] {
        assert_eq!(metric(&info("n", value), "n"), 0);
    }
    assert_eq!(metric(&info("n", &u64::MAX.to_string()), "n"), u64::MAX);
    for value in ["", "bad", "-0.1", "NaN", "inf", "-inf"] {
        assert_eq!(nanos(&info("n", value), "n"), 0);
    }
    for (value, expected) in [("0", 0), ("0.0000000004", 0), ("0.0000000006", 1), ("1.25", 1_250_000_000), ("1e100", u64::MAX)] {
        assert_eq!(nanos(&info("n", value), "n"), expected);
    }
    let leaf = ProfilingInfo { metrics: [("OPERATOR_CARDINALITY".into(), u64::MAX.to_string()),
        ("OPERATOR_ROWS_SCANNED".into(), u64::MAX.to_string())].into(), children: vec![] };
    let root = ProfilingInfo { metrics: [("CPU_TIME".into(), "0.5".into()), ("LATENCY".into(), "1".into()),
        ("SYSTEM_PEAK_TEMP_DIR_SIZE".into(), "4096".into())].into(),
        children: vec![ProfilingInfo { metrics: leaf.metrics.clone(), children: vec![leaf] }] };
    let cost = read(&root);
    assert_eq!(cost.profiled_statements, 1);
    assert_eq!(cost.rows_scanned, u64::MAX); assert_eq!(cost.operator_rows, u64::MAX);
    assert_eq!(cost.operators, 2); assert_eq!(cost.max_plan_depth, 2); assert_eq!(cost.peak_spill_bytes, 4096);
    for name in ["CPU_TIME", "LATENCY"] {
        let mut partial = root.clone(); partial.metrics.remove(name);
        assert_eq!(read(&partial).profiled_statements, 0);
    }
    for name in ["OPERATOR_CARDINALITY", "OPERATOR_ROWS_SCANNED"] {
        let mut partial = root.clone(); partial.children[0].children[0].metrics.remove(name);
        assert_eq!(read(&partial).profiled_statements, 0);
    }
}

#[test]
fn disabled_and_caller_owned_profiling_preserve_configuration_and_statement_counts() {
    let conn = Connection::open_in_memory().unwrap();
    let cost = Arc::new(Mutex::new(EngineCost::default()));
    assert!(Profile::start(&conn, false, cost.clone(), "SELECT 1").unwrap().is_none());
    assert_eq!(cost.lock().unwrap().statements, 1);
    assert_eq!(cost.lock().unwrap().profiled_statements, 0);
    conn.execute_batch("PRAGMA enable_profiling='no_output'").unwrap();
    {
        let sql = "SELECT '雪'";
        let profile = Profile::start(&conn, true, cost.clone(), sql).unwrap().unwrap();
        let _: String = conn.query_row(sql, [], |row| row.get(0)).unwrap();
        profile.finish();
        assert_eq!(cost.lock().unwrap().sql_bytes, sql.len() as u64);
    }
    let setting: String = conn.query_row("SELECT current_setting('enable_profiling')", [], |row| row.get(0)).unwrap();
    assert_eq!(setting, "no_output");
    assert_eq!(cost.lock().unwrap().statements, 2);
    assert_eq!(cost.lock().unwrap().profiled_statements, 1);
}

#[test]
fn profiling_handles_absent_snapshots_and_poisoned_cost_targets() {
    let conn = Connection::open_in_memory().unwrap();
    let cost = Arc::new(Mutex::new(EngineCost::default()));
    let profile = Profile::start(&conn, true, cost.clone(), "SELECT 1").unwrap().unwrap();
    conn.execute_batch("PRAGMA disable_profiling").unwrap();
    profile.finish();
    assert_eq!(cost.lock().unwrap().profiled_statements, 0);
    drop(profile);
    let profile = Profile::start(&conn, true, cost.clone(), "SELECT 1").unwrap().unwrap();
    let _: i64 = conn.query_row("SELECT 1", [], |row| row.get(0)).unwrap();
    let target = cost.clone();
    assert!(std::thread::spawn(move || { let _guard = target.lock().unwrap(); panic!("intentional cost target poison"); }).join().is_err());
    profile.finish();
    drop(profile);
    assert!(Profile::start(&conn, false, cost, "SELECT 1").is_err());
}
