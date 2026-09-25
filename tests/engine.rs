#![cfg(feature = "duckdb")]

use orchiddb::engine::{GraphEngine, QueryResult, ReadMode};
use orchiddb::ir::Value;
use std::collections::BTreeMap;

fn rows(result: QueryResult) -> Vec<String> {
    let batch = result.returned.batch;
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

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "orchiddb-{name}-{}-{}.duckdb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[tokio::test]
async fn durable_create_update_delete_and_id_gaps() {
    let path = path("durable");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine
            .cypher("CREATE (a:P {name:'a'})-[:R {weight:2}]->(b:P {name:'b'})")
            .await
            .unwrap();
        engine.cypher("CREATE (:P {name:'c'})").await.unwrap();
        engine
            .cypher("MATCH (p:P {name:'a'}) SET p.name = 'alice'")
            .await
            .unwrap();
        engine
            .cypher("MATCH (p:P {name:'c'}) DELETE p")
            .await
            .unwrap();
    }
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(
                engine
                    .cypher("MATCH (a:P)-[r:R]->(b:P) RETURN a.name,r.weight,b.name")
                    .await
                    .unwrap()
            ),
            ["alice|2|b"]
        );
        engine.cypher("CREATE (:P {name:'d'})").await.unwrap();
        assert_eq!(
            rows(
                engine
                    .cypher("MATCH (p:P) RETURN p.name ORDER BY p.name")
                    .await
                    .unwrap()
            ),
            ["alice", "b", "d"]
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn rollback_and_drop_never_publish_writes() {
    let path = path("rollback");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.cypher("CREATE (:P {n:1})").await.unwrap();
        engine.begin().unwrap();
        engine.cypher("CREATE (:P {n:2})").await.unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN count(p)").await.unwrap()),
            ["2"]
        );
        engine.rollback().unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN count(p)").await.unwrap()),
            ["1"]
        );
        engine.begin().unwrap();
        engine.cypher("CREATE (:P {n:3})").await.unwrap();
    }
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN p.n").await.unwrap()),
            ["1"]
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn failed_statement_is_atomic_inside_a_transaction() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {n:1})-[:R]->(:P {n:2})")
        .await
        .unwrap();
    engine.begin().unwrap();
    // The SET runs before DELETE discovers the incident relationship.
    assert!(
        engine
            .cypher("MATCH (p:P {n:1}) SET p.n = 99 DELETE p")
            .await
            .is_err()
    );
    assert_eq!(
        rows(
            engine
                .cypher("MATCH (p:P) RETURN p.n ORDER BY p.n")
                .await
                .unwrap()
        ),
        ["1", "2"]
    );
    engine.cypher("CREATE (:P {n:3})").await.unwrap();
    engine.commit().unwrap();
    assert_eq!(
        rows(engine.cypher("MATCH (p:P) RETURN count(p)").await.unwrap()),
        ["3"]
    );
}

#[tokio::test]
async fn typed_parameters_are_data_and_missing_parameters_are_errors() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let payload = "x'}) DELETE n //";
    let params = BTreeMap::from([("name".into(), Value::String(payload.into()))]);
    engine
        .cypher_with_params("CREATE (:P {name:$name})", &params)
        .await
        .unwrap();
    assert_eq!(
        rows(
            engine
                .cypher_with_params("MATCH (p:P) WHERE p.name = $name RETURN p.name", &params)
                .await
                .unwrap()
        ),
        [payload]
    );
    assert!(
        engine
            .cypher("MATCH (p:P) WHERE p.name = $missing RETURN p")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn transactions_have_snapshot_isolation_and_refresh_after_commit() {
    let path = path("isolation");
    {
        let mut first = GraphEngine::open(&path).unwrap();
        first.cypher("CREATE (:P {n:1})").await.unwrap();
        let mut second = GraphEngine::open(&path).unwrap();
        first.begin().unwrap();
        second.cypher("CREATE (:P {n:2})").await.unwrap();
        assert_eq!(
            rows(first.cypher("MATCH (p:P) RETURN count(p)").await.unwrap()),
            ["1"]
        );
        first.commit().unwrap();
        assert_eq!(
            rows(first.cypher("MATCH (p:P) RETURN count(p)").await.unwrap()),
            ["2"]
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn sql_only_reads_report_duckdb_execution() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.set_read_mode(ReadMode::SqlOnly);
    let result = engine.cypher("RETURN 1 + 2 AS answer").await.unwrap();
    assert_eq!(result.backend, orchiddb::engine::ExecutionBackend::DuckDb);
    assert_eq!(rows(result), ["3"]);
}

#[tokio::test]
async fn property_pattern_parameters_require_maps() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {n:1}), (:P {n:2})")
        .await
        .unwrap();
    let map = BTreeMap::from([(
        "props".into(),
        Value::Map(BTreeMap::from([("n".into(), Value::Int(1))])),
    )]);
    assert_eq!(
        rows(
            engine
                .cypher_with_params("MATCH (p:P $props) RETURN p.n", &map)
                .await
                .unwrap()
        ),
        ["1"]
    );
    let invalid = BTreeMap::from([("props".into(), Value::Bool(true))]);
    assert!(
        engine
            .cypher_with_params("MATCH (p:P $props) DELETE p", &invalid)
            .await
            .is_err()
    );
    assert_eq!(
        rows(engine.cypher("MATCH (p:P) RETURN count(p)").await.unwrap()),
        ["2"]
    );
}
