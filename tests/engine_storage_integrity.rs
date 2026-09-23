#![cfg(feature = "duckdb")]

use new_graph::engine::GraphEngine;

struct Database(std::path::PathBuf);
impl Database {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "crabgraph-integrity-{}-{}-{}.duckdb",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )))
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn names(engine: &mut GraphEngine) -> Vec<String> {
    let result = engine
        .cypher("MATCH (n:P) RETURN n.name ORDER BY n.name")
        .await
        .unwrap();
    (0..result.returned.batch.num_rows())
        .map(|i| {
            arrow::util::display::array_value_to_string(result.returned.batch.column(0), i).unwrap()
        })
        .collect()
}

#[tokio::test]
async fn rolled_back_revision_cannot_mask_another_writers_commit() {
    let database = Database::new();
    let mut a = GraphEngine::open(&database.0).unwrap();
    let mut b = GraphEngine::open(&database.0).unwrap();
    a.cypher("CREATE (:P {name:'base'})").await.unwrap();
    a.begin().unwrap();
    a.cypher("CREATE (:P {name:'aborted'})").await.unwrap();
    a.rollback().unwrap();
    b.cypher("CREATE (:P {name:'committed'})").await.unwrap();
    // Both writes used revision 2; the rolled-back cache must not win.
    assert_eq!(names(&mut a).await, ["base", "committed"]);
}

#[tokio::test]
async fn checkpoint_conflicts_with_stale_writer_and_refreshes_readers() {
    let database = Database::new();
    let mut a = GraphEngine::open(&database.0).unwrap();
    a.cypher("CREATE (:P {name:'base'})").await.unwrap();
    let mut b = GraphEngine::open(&database.0).unwrap();
    b.begin().unwrap();
    a.checkpoint().unwrap();
    assert!(b.cypher("CREATE (:P {name:'stale'})").await.is_err());
    b.rollback().unwrap();
    assert_eq!(names(&mut b).await, ["base"]);
    a.cypher("CREATE (:P {name:'fresh'})").await.unwrap();
    assert_eq!(names(&mut b).await, ["base", "fresh"]);
}

#[tokio::test]
async fn corrupt_incremental_record_fails_open_instead_of_losing_data() {
    let database = Database::new();
    {
        let mut engine = GraphEngine::open(&database.0).unwrap();
        engine.cypher("CREATE (:P {name:'base'})").await.unwrap();
    }
    {
        let connection = duckdb::Connection::open(&database.0).unwrap();
        connection
            .execute_batch("UPDATE __crabgraph_records SET payload = 'bad'::BLOB WHERE kind = 1")
            .unwrap();
    }
    assert!(GraphEngine::open(&database.0).is_err());
}

#[tokio::test]
async fn future_storage_version_is_rejected_without_changing_it() {
    let database = Database::new();
    drop(GraphEngine::open(&database.0).unwrap());
    {
        let connection = duckdb::Connection::open(&database.0).unwrap();
        connection
            .execute_batch("UPDATE __crabgraph_state SET format_version = 999")
            .unwrap();
    }
    assert!(GraphEngine::open(&database.0).is_err());
    let connection = duckdb::Connection::open(&database.0).unwrap();
    let version: i32 = connection
        .query_row("SELECT format_version FROM __crabgraph_state", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(version, 999);
}

#[tokio::test]
async fn invalid_legacy_checkpoint_does_not_commit_a_schema_migration() {
    let database = Database::new();
    {
        let connection = duckdb::Connection::open(&database.0).unwrap();
        connection.execute_batch("CREATE TABLE __crabgraph_state(singleton INTEGER PRIMARY KEY, format_version INTEGER, payload BLOB); INSERT INTO __crabgraph_state VALUES(1,1,'invalid'::BLOB)").unwrap();
    }
    assert!(GraphEngine::open(&database.0).is_err());
    let connection = duckdb::Connection::open(&database.0).unwrap();
    let version: i32 = connection
        .query_row("SELECT format_version FROM __crabgraph_state", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, 1);
    let columns: i64 = connection.query_row("SELECT count(*) FROM information_schema.columns WHERE table_name = '__crabgraph_state' AND column_name = 'revision'", [], |row| row.get(0)).unwrap();
    assert_eq!(columns, 0);
}

#[tokio::test]
async fn managed_and_sql_sessions_share_one_database_instance() {
    let database = Database::new();
    let mut engine = GraphEngine::open(&database.0).unwrap();
    engine.cypher("CREATE (:P {name:'first'})").await.unwrap();
    let mut sql = new_graph::ir::rel::sql::DuckDbExecutor::open(&database.0).unwrap();
    engine.cypher("CREATE (:P {name:'second'})").await.unwrap();
    let count: i64 = sql
        .connection()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM __crabgraph_records WHERE kind = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    sql.execute_batch("CREATE TABLE user_data(value BIGINT); INSERT INTO user_data VALUES(42)")
        .unwrap();
    assert_eq!(names(&mut engine).await, ["first", "second"]);
}
