#![cfg(feature = "duckdb")]

//! Session-level behavior of [`DuckDbExecutor`]: file-backed persistence,
//! transactions, setup recovery, and raw-write cache invalidation.

use std::path::PathBuf;

use orchiddb::ir::rel::sql::{DuckDbExecutor, SqlError, SqlExecutor, SqlValue};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "crabgraph-duckdb-session-{name}-{}-{}.duckdb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn file_backed_executor_persists_across_reopen() {
    let path = temp_path("persist");
    {
        let mut executor = DuckDbExecutor::open(&path).unwrap();
        executor
            .execute_batch("CREATE TABLE persisted(v INTEGER)")
            .unwrap();
        executor
            .execute_batch("INSERT INTO persisted VALUES (42)")
            .unwrap();
    }
    {
        let mut executor = DuckDbExecutor::open(&path).unwrap();
        assert_eq!(
            executor.run(&[], "SELECT v FROM persisted").unwrap(),
            vec![vec![SqlValue::Int(42)]]
        );
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rollback_undoes_uncommitted_writes() {
    let mut executor = DuckDbExecutor::new();
    executor.execute_batch("CREATE TABLE t(v INTEGER)").unwrap();
    executor.execute_batch("INSERT INTO t VALUES (1)").unwrap();

    executor.begin().unwrap();
    assert!(executor.in_transaction());
    executor.execute_batch("INSERT INTO t VALUES (2)").unwrap();
    assert_eq!(
        executor.run(&[], "SELECT COUNT(*) FROM t").unwrap(),
        vec![vec![SqlValue::Int(2)]]
    );

    executor.rollback().unwrap();
    assert!(!executor.in_transaction());
    assert_eq!(
        executor.run(&[], "SELECT COUNT(*) FROM t").unwrap(),
        vec![vec![SqlValue::Int(1)]]
    );
}

#[test]
fn failed_setup_block_recovers_without_partial_tables() {
    let mut executor = DuckDbExecutor::new();
    let error = executor
        .run(
            &[
                "CREATE TABLE t(v INTEGER)".to_string(),
                "INSERT INTO t VALUES (1, 2)".to_string(),
            ],
            "SELECT * FROM t",
        )
        .unwrap_err();
    assert!(matches!(&error, SqlError::Setup(_)), "{error}");

    // The failed INSERT must have rolled back the CREATE from the same block,
    // leaving no half-materialized table behind.
    assert!(executor.run(&[], "SELECT * FROM t").is_err());
    // The session stays usable afterwards.
    assert_eq!(
        executor.run(&[], "SELECT 1").unwrap(),
        vec![vec![SqlValue::Int(1)]]
    );
}

#[test]
fn repeated_execute_batch_mutations_accumulate_and_invalidate_setup() {
    let mut executor = DuckDbExecutor::new();
    executor
        .execute_batch("CREATE TABLE counters(n INTEGER)")
        .unwrap();
    executor
        .execute_batch("INSERT INTO counters VALUES (1)")
        .unwrap();
    executor
        .execute_batch("INSERT INTO counters VALUES (2)")
        .unwrap();
    executor
        .execute_batch("INSERT INTO counters VALUES (3)")
        .unwrap();
    executor
        .execute_batch("UPDATE counters SET n = n * 10")
        .unwrap();
    executor
        .execute_batch("DELETE FROM counters WHERE n = 30")
        .unwrap();
    assert_eq!(
        executor
            .run(&[], "SELECT n FROM counters ORDER BY n")
            .unwrap(),
        vec![vec![SqlValue::Int(10)], vec![SqlValue::Int(20)]]
    );

    // Raw writes invalidate the applied-setup cache: after dropping the table,
    // the same setup must be re-applied rather than skipped on the next run.
    let setup = [
        "CREATE TABLE t(v INTEGER)".to_string(),
        "INSERT INTO t VALUES (1)".to_string(),
    ];
    executor.run(&setup, "SELECT COUNT(*) FROM t").unwrap();
    executor.execute_batch("DROP TABLE t").unwrap();
    assert_eq!(
        executor.run(&setup, "SELECT COUNT(*) FROM t").unwrap(),
        vec![vec![SqlValue::Int(1)]]
    );
}

#[test]
fn raw_begin_transaction_is_detected() {
    let mut executor = DuckDbExecutor::new();
    assert!(!executor.in_transaction());

    executor.execute_batch("BEGIN TRANSACTION").unwrap();
    assert!(executor.in_transaction());
    executor.execute_batch("ROLLBACK").unwrap();
    assert!(!executor.in_transaction());

    executor.begin().unwrap();
    assert!(executor.in_transaction());
    executor.commit().unwrap();
    assert!(!executor.in_transaction());
}

#[test]
fn setup_timeout_preserves_the_file_backed_session() {
    use std::time::Duration;
    let path = temp_path("timeout");
    {
        let mut executor = DuckDbExecutor::open(&path).unwrap();
        executor
            .execute_batch("CREATE TABLE keeper AS SELECT 42 AS n")
            .unwrap();
        executor.set_timeouts(Duration::from_secs(2), Duration::from_millis(1));
        let error = executor
            .run(
                &["CREATE TABLE enormous AS SELECT i FROM range(1000000000) t(i)".into()],
                "SELECT count(*) FROM enormous",
            )
            .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        assert_eq!(
            executor.run(&[], "SELECT n FROM keeper").unwrap(),
            vec![vec![SqlValue::Int(42)]]
        );
        assert!(executor.run(&[], "SELECT * FROM enormous").is_err());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn file_backed_sessions_observe_other_commits_and_detect_conflicts() {
    let path = temp_path("concurrent");
    {
        let mut a = DuckDbExecutor::open(&path).unwrap();
        a.execute_batch("CREATE TABLE shared(id BIGINT PRIMARY KEY, value BIGINT); INSERT INTO shared VALUES(1,10)").unwrap();
        let mut b = DuckDbExecutor::open(&path).unwrap();
        a.execute_batch("INSERT INTO shared VALUES(2,20)").unwrap();
        assert_eq!(
            b.run(&[], "SELECT count(*) FROM shared").unwrap(),
            vec![vec![SqlValue::Int(2)]]
        );
        b.begin().unwrap();
        b.run(&[], "SELECT * FROM shared").unwrap();
        a.execute_batch("UPDATE shared SET value=11 WHERE id=1")
            .unwrap();
        assert!(
            b.execute_batch("UPDATE shared SET value=12 WHERE id=1")
                .is_err()
        );
        b.rollback().unwrap();
        assert_eq!(
            b.run(&[], "SELECT value FROM shared WHERE id=1").unwrap(),
            vec![vec![SqlValue::Int(11)]]
        );
    }
    let _ = std::fs::remove_file(path);
}
