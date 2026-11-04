
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
