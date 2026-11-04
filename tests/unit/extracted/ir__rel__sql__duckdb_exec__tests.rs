
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
