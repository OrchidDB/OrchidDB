#![cfg(feature = "duckdb")]

//! End-to-end tests for `MappedGraphEngine::cypher_update`, the mapped-table
//! write path that turns a single Cypher `MATCH ... SET p.<prop> = <expr>`
//! (no `RETURN`) on one statically labeled, mapped node into a native SQL
//! `UPDATE` against the user's own table.
//!
//! The user-owned `users` table is mapped onto the `Person` label and stood up
//! in an in-memory DuckDB database through `execute_sql`, exactly like
//! `tests/mapped_engine.rs`. Writes flow through the same executor, so the
//! tests also exercise transaction participation (autocommit outside an
//! explicit `begin`, rollback inside one) and the up-front rejection of every
//! unsupported graph mutation before any source row is touched.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow::datatypes::{DataType, Field, Schema};

use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping};
use orchiddb::ir::rel::sql::DuckDbExecutor;
use orchiddb::ir::value::Value;
use orchiddb::mapped_engine::MappedGraphEngine;

/// A plain user table with no default id generation.
const DDL: &str = r#"
CREATE TABLE users (id BIGINT, name VARCHAR, age BIGINT);
INSERT INTO users VALUES (1, 'alice', 30), (2, 'bob', 28), (3, 'carol', 41);
"#;

fn schema(fields: Vec<Field>) -> Arc<Schema> {
    Arc::new(Schema::new(fields))
}

/// Map the `Person` label onto the user `users` table. Only the schema is
/// registered; the data lives in DuckDB.
fn mapping() -> Arc<GraphMapping> {
    let mut mapping = GraphMapping::new();
    mapping
        .register_table_schema(
            "users",
            schema(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("name", DataType::Utf8, true),
                Field::new("age", DataType::Int64, true),
            ]),
        )
        .map_node(
            NodeMapping::table("Person", "users", "id")
                .property("name", "name")
                .property("identity", "id")
                .property("age", "age"),
        );
    Arc::new(mapping)
}

fn engine() -> MappedGraphEngine {
    MappedGraphEngine::new(DuckDbExecutor::new(), mapping())
}

async fn setup_engine() -> MappedGraphEngine {
    let mut engine = engine();
    engine.execute_sql(DDL).expect("apply schema DDL");
    engine
}

fn lines(batch: &RecordBatch) -> Vec<String> {
    (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|col| arrow::util::display::array_value_to_string(col, row).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

/// Read a single BIGINT cell straight from the engine's DuckDB connection.
fn cell_i64(engine: &mut MappedGraphEngine, sql: &str) -> i64 {
    let executor = engine.executor_mut();
    let conn = executor.connection().unwrap();
    conn.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap()
}

/// Read a single nullable VARCHAR cell straight from the DuckDB connection.
fn cell_opt_string(engine: &mut MappedGraphEngine, sql: &str) -> Option<String> {
    let executor = engine.executor_mut();
    let conn = executor.connection().unwrap();
    conn.query_row(sql, [], |row| row.get::<_, Option<String>>(0))
        .unwrap()
}

#[tokio::test]
async fn arithmetic_update_reports_affected_count_and_is_visible_via_cypher() {
    let mut engine = setup_engine().await;
    let updated = engine
        .cypher_update("MATCH (p:Person) WHERE p.age > 20 SET p.age = p.age + 1")
        .await
        .expect("arithmetic update");
    assert_eq!(updated, 3);

    let result = engine
        .cypher("MATCH (p:Person) RETURN p.age ORDER BY p.age")
        .await
        .expect("read after update");
    assert_eq!(lines(&result.batch), vec!["29", "31", "42"]);
}

#[tokio::test]
async fn predicate_limits_affected_count_and_is_visible_via_raw_connection() {
    let mut engine = setup_engine().await;

    let updated = engine
        .cypher_update("MATCH (p:Person) WHERE p.name = 'alice' SET p.age = 99")
        .await
        .expect("predicate update");
    assert_eq!(updated, 1);
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 1"),
        99
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 2"),
        28
    );

    let none = engine
        .cypher_update("MATCH (p:Person) WHERE p.name = 'nobody' SET p.age = -1")
        .await
        .expect("no-op update");
    assert_eq!(none, 0);
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 1"),
        99
    );
}

#[tokio::test]
async fn typed_numeric_parameter_binding_updates_matching_rows() {
    let mut engine = setup_engine().await;
    let params = BTreeMap::from([
        ("min".to_string(), Value::Int(30)),
        ("new_age".to_string(), Value::Int(7)),
    ]);
    let updated = engine
        .cypher_update_with_params(
            "MATCH (p:Person) WHERE p.age >= $min SET p.age = $new_age",
            &params,
        )
        .await
        .expect("numeric parameter update");
    assert_eq!(updated, 2);
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 1"),
        7
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 3"),
        7
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 2"),
        28
    );
}

#[tokio::test]
async fn parameter_injection_is_bound_as_data_not_spliced_sql() {
    let mut engine = setup_engine().await;
    let payload = "alice'; DROP TABLE users;--";
    let params = BTreeMap::from([
        ("who".to_string(), Value::String("bob".to_string())),
        ("target".to_string(), Value::String(payload.to_string())),
    ]);
    let updated = engine
        .cypher_update_with_params(
            "MATCH (p:Person) WHERE p.name = $who SET p.name = $target",
            &params,
        )
        .await
        .expect("string parameter update");
    assert_eq!(updated, 1);
    assert_eq!(
        cell_opt_string(&mut engine, "SELECT name FROM users WHERE id = 2"),
        Some(payload.to_string())
    );
    // The injected payload must have been stored verbatim, not executed: the
    // table still exists with all three rows intact.
    assert_eq!(cell_i64(&mut engine, "SELECT count(*) FROM users"), 3);
}

#[tokio::test]
async fn null_removes_mapped_property_via_sql_null() {
    let mut engine = setup_engine().await;

    let updated = engine
        .cypher_update("MATCH (p:Person) WHERE p.name = 'alice' SET p.name = null")
        .await
        .expect("literal null update");
    assert_eq!(updated, 1);
    assert_eq!(
        cell_opt_string(&mut engine, "SELECT name FROM users WHERE id = 1"),
        None
    );

    let params = BTreeMap::from([("empty".to_string(), Value::Null)]);
    let updated = engine
        .cypher_update_with_params(
            "MATCH (p:Person) WHERE p.name = 'bob' SET p.name = $empty",
            &params,
        )
        .await
        .expect("parameter null update");
    assert_eq!(updated, 1);
    assert_eq!(
        cell_opt_string(&mut engine, "SELECT name FROM users WHERE id = 2"),
        None
    );
}

#[tokio::test]
async fn update_participates_in_explicit_transaction_and_rolls_back() {
    let mut engine = setup_engine().await;

    engine.executor_mut().begin().unwrap();
    let updated = engine
        .cypher_update("MATCH (p:Person) SET p.age = 0")
        .await
        .expect("update inside transaction");
    assert_eq!(updated, 3);
    // Visible inside the uncommitted transaction...
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 1"),
        0
    );

    engine.executor_mut().rollback().unwrap();
    // ...but undone by the rollback.
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 1"),
        30
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE id = 3"),
        41
    );
}

#[tokio::test]
async fn duplicate_matching_ids_are_rejected_without_modifications() {
    let mut engine = engine();
    engine
        .execute_sql(
            "CREATE TABLE users (id BIGINT, name VARCHAR, age BIGINT); \
             INSERT INTO users VALUES (1, 'alice', 30), (1, 'bob', 28), (2, 'carol', 41);",
        )
        .unwrap();

    let err = engine
        .cypher_update("MATCH (p:Person) WHERE p.age > 20 SET p.age = p.age + 1")
        .await
        .expect_err("duplicate matching ids must be rejected");
    let message = err.to_lowercase();
    assert!(
        message.contains("duplicate") || message.contains("unique"),
        "expected a duplicate-id rejection, got: {err}"
    );

    let result = engine
        .cypher("MATCH (p:Person) RETURN p.age ORDER BY p.age")
        .await
        .expect("read after rejected update");
    assert_eq!(lines(&result.batch), vec!["28", "30", "41"]);
}

#[tokio::test]
async fn updating_the_id_property_is_rejected() {
    let mut engine = setup_engine().await;
    let err = engine
        .cypher_update("MATCH (p:Person) SET p.identity = 99")
        .await
        .expect_err("id property update must be rejected");
    assert!(
        err.to_lowercase().contains("id"),
        "expected an id-property rejection, got: {err}"
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT id FROM users WHERE id = 1"),
        1
    );
}

#[tokio::test]
async fn unsupported_graph_mutations_are_rejected_before_any_write() {
    let mut engine = setup_engine().await;
    for query in [
        "CREATE (:Person {name: 'dave'})",
        "MERGE (p:Person {name: 'dave'})",
        "MATCH (p:Person) DELETE p",
        "MATCH (p:Person) SET p = {age: 99}",
        "MATCH (p:Person) SET p.age = 1, p.name = 'x'",
        "MATCH (p:Person) SET p.age = 1 RETURN p",
    ] {
        let err = engine.cypher_update(query).await.expect_err(query);
        assert!(
            !err.trim().is_empty(),
            "expected a rejection for `{query}`, got an empty error"
        );
    }
    let result = engine
        .cypher("MATCH (p:Person) RETURN count(p) AS n")
        .await
        .expect("count after rejections");
    assert_eq!(lines(&result.batch), vec!["3"]);
}

#[tokio::test]
async fn quoted_identifiers_in_table_and_columns_are_handled() {
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), quoted_mapping());
    engine
        .execute_sql(
            "CREATE TABLE \"order\" (id BIGINT, name VARCHAR, \"group\" BIGINT); \
             INSERT INTO \"order\" VALUES (1, 'alice', 30), (2, 'bob', 28), (3, 'carol', 41);",
        )
        .unwrap();

    let updated = engine
        .cypher_update("MATCH (p:Person) WHERE p.age > 20 SET p.age = p.age + 1")
        .await
        .expect("update over quoted identifiers");
    assert_eq!(updated, 3);
    assert_eq!(
        cell_i64(&mut engine, "SELECT \"group\" FROM \"order\" WHERE id = 1"),
        31
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT \"group\" FROM \"order\" WHERE id = 2"),
        29
    );
}

/// A mapping whose table and property column names are reserved SQL words and
/// therefore require identifier quoting in the generated `UPDATE`.
fn quoted_mapping() -> Arc<GraphMapping> {
    let mut mapping = GraphMapping::new();
    mapping
        .register_table_schema(
            "order",
            schema(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("name", DataType::Utf8, true),
                Field::new("group", DataType::Int64, true),
            ]),
        )
        .map_node(
            NodeMapping::table("Person", "order", "id")
                .property("name", "name")
                .property("age", "group"),
        );
    Arc::new(mapping)
}

#[tokio::test]
async fn cypher_mutations_update_the_same_mapped_rows() {
    let mut engine = setup_engine().await;
    engine.cypher("MATCH (p:Person) SET p.age = 99").await.unwrap();
    let result = engine.cypher("MATCH (p:Person) WHERE p.age=99 RETURN count(p) AS n").await.unwrap();
    assert_eq!(lines(&result.batch), vec!["3"]);
}

#[tokio::test]
async fn duplicate_source_identity_outside_match_is_rejected() {
    let mut engine = setup_engine().await;
    engine
        .execute_sql("INSERT INTO users VALUES (1, 'other', 99)")
        .unwrap();
    assert!(
        engine
            .cypher_update("MATCH (p:Person) WHERE p.name = 'alice' SET p.age = 5")
            .await
            .is_err()
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE name='alice'"),
        30
    );
    assert_eq!(
        cell_i64(&mut engine, "SELECT age FROM users WHERE name='other'"),
        99
    );
}

#[tokio::test]
async fn failed_sql_assignment_is_atomic_and_engine_remains_usable() {
    let mut engine = setup_engine().await;
    assert!(
        engine
            .cypher_update("MATCH (p:Person) SET p.age = 'not an integer'")
            .await
            .is_err()
    );
    assert_eq!(cell_i64(&mut engine, "SELECT sum(age) FROM users"), 99);
    assert_eq!(
        engine
            .cypher_update("MATCH (p:Person) SET p.age = p.age + 1")
            .await
            .unwrap(),
        3
    );
}
