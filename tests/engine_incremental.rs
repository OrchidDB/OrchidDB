#![cfg(feature = "duckdb")]

//! Integration tests for the incremental (format version 2) storage layer of
//! [`GraphEngine`].
//!
//! Format version 2 keeps the immutable checkpoint payload in
//! `__crabgraph_state` (now alongside a monotonic `revision BIGINT`) and
//! layers ordinary CREATE/SET/DELETE writes into `__crabgraph_records`
//! `(kind, name, id, payload)`. Only `checkpoint()` and `replace_graph()`
//! rewrite the checkpoint payload. `kind` is 1 for nodes, 2 for edges,
//! 3 for node metadata, and 4 for edge metadata.
//!
//! Every assertion below inspects the committed on-disk state through a raw
//! `duckdb::Connection`, opened only *after* the engines are dropped.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use duckdb::{Connection, params};

use orchiddb::engine::{GraphEngine, QueryResult};
use orchiddb::ir::{PropertyGraph, Value};
use orchiddb::storage::{decode_graph, encode_graph};

/// Format every column of every returned row into `col1|col2|...` strings.
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

fn path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "crabgraph-incremental-{name}-{}-{}.duckdb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ))
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}.wal", path.display()));
}

#[tokio::test]
async fn batched_delta_upserts_preserve_updates_rollback_and_reopen() {
    let file = path("batched-upserts");
    {
        let mut engine = GraphEngine::open(&file).unwrap();
        engine.cypher("UNWIND range(0,599) AS i CREATE (:Batch {i:i, value:0})").await.unwrap();
        engine.cypher("MATCH (n:Batch) SET n.value = n.i + 1").await.unwrap();
        engine.begin().unwrap();
        engine.cypher("MATCH (n:Batch) DELETE n").await.unwrap();
        engine.rollback().unwrap();
        assert_eq!(rows(engine.cypher("MATCH (n:Batch) RETURN count(n), sum(n.value)").await.unwrap()), vec!["600|180300"]);
    }
    {
        let mut engine = GraphEngine::open(&file).unwrap();
        assert_eq!(rows(engine.cypher("MATCH (n:Batch) RETURN count(n), sum(n.value)").await.unwrap()), vec!["600|180300"]);
        engine.cypher("MATCH (n:Batch) WHERE n.i < 100 DELETE n").await.unwrap();
    }
    {
        let mut engine = GraphEngine::open(&file).unwrap();
        assert_eq!(rows(engine.cypher("MATCH (n:Batch) RETURN count(n)").await.unwrap()), vec!["500"]);
    }
    cleanup(&file);
}

/// Read the checkpoint row: `(format_version, revision, payload)`.
fn read_state(conn: &Connection) -> (i32, Option<i64>, Vec<u8>) {
    let state: (i32, Option<i64>, Vec<u8>) = conn
        .query_row(
            "SELECT format_version, revision, payload FROM __crabgraph_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    state
}

/// Read every overlay record, ordered by its primary key.
fn read_records(conn: &Connection) -> Vec<(i32, String, i64, Vec<u8>)> {
    let mut stmt = conn
        .prepare("SELECT kind, name, id, payload FROM __crabgraph_records ORDER BY kind, name, id")
        .unwrap();
    let mapped = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap();
    mapped
        .collect::<Result<Vec<(i32, String, i64, Vec<u8>)>, _>>()
        .unwrap()
}

/// Build a small in-memory graph with a single node, for `replace_graph`.
fn graph_with_node(label: &str, key: &str, value: Value) -> PropertyGraph {
    let graph = PropertyGraph::new();
    graph.insert_node(label, BTreeMap::from([(key.to_string(), value)]));
    graph
}

#[tokio::test]
async fn checkpoint_blob_is_unchanged_by_ordinary_writes() {
    let path = path("checkpoint_blob");
    let (initial_version, initial_revision, initial_payload) = {
        drop(GraphEngine::open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        let state = read_state(&conn);
        drop(conn);
        state
    };
    assert_eq!(initial_version, 2);
    assert_eq!(initial_revision, Some(0));

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.cypher("CREATE (:P {n:1})").await.unwrap();
        engine.cypher("MATCH (p:P) SET p.n = 2").await.unwrap();
        drop(engine);
    }

    let (version, revision, payload) = {
        let conn = Connection::open(&path).unwrap();
        let state = read_state(&conn);
        drop(conn);
        state
    };
    assert_eq!(version, 2);
    assert_eq!(
        payload, initial_payload,
        "checkpoint payload must stay immutable"
    );
    assert!(
        revision.unwrap() > 0,
        "revision must advance on ordinary writes"
    );

    {
        let conn = Connection::open(&path).unwrap();
        let records = read_records(&conn);
        let nodes: Vec<_> = records.iter().filter(|(k, _, _, _)| *k == 1).collect();
        assert_eq!(nodes.len(), 1, "the write must land in the records overlay");
        assert_eq!(nodes[0].1.as_str(), "P");
        assert_eq!(nodes[0].2, 0);
        drop(conn);
    }

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN p.n").await.unwrap()),
            ["2"]
        );
        drop(engine);
    }

    cleanup(&path);
}

#[tokio::test]
async fn single_node_update_touches_only_its_record() {
    let path = path("single_update");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine
            .cypher("CREATE (:P {n:1}), (:P {n:2}), (:P {n:3}), (:P {n:4}), (:P {n:5})")
            .await
            .unwrap();
        drop(engine);
    }

    let before = {
        let conn = Connection::open(&path).unwrap();
        let records = read_records(&conn);
        drop(conn);
        records
    };

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine
            .cypher("MATCH (p:P {n:3}) SET p.n = 30")
            .await
            .unwrap();
        drop(engine);
    }

    let after = {
        let conn = Connection::open(&path).unwrap();
        let records = read_records(&conn);
        drop(conn);
        records
    };

    assert_eq!(
        before.len(),
        after.len(),
        "no records may be added or removed"
    );
    let diffs: Vec<_> = before
        .iter()
        .zip(after.iter())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(diffs.len(), 1, "exactly one record must change");
    let ((bk, bn, bi, _), (ak, an, ai, _)) = diffs[0];
    assert_eq!((*bk, bn.as_str(), *bi), (*ak, an.as_str(), *ai));
    assert_eq!(*bk, 1, "the changed record must be a node");
    assert_eq!(bn.as_str(), "P");
    assert_eq!(*bi, 2, "the changed record must be the touched node (n:3)");

    cleanup(&path);
}

#[tokio::test]
async fn rollback_clears_uncommitted_records() {
    let path = path("rollback_records");
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
        drop(engine);
    }

    {
        let conn = Connection::open(&path).unwrap();
        let records = read_records(&conn);
        let nodes: Vec<_> = records.iter().filter(|(k, _, _, _)| *k == 1).collect();
        assert_eq!(
            nodes.len(),
            1,
            "uncommitted node records must be rolled back"
        );
        assert_eq!(nodes[0].1.as_str(), "P");
        assert_eq!(nodes[0].2, 0);
        drop(conn);
    }

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN p.n").await.unwrap()),
            ["1"]
        );
        drop(engine);
    }

    cleanup(&path);
}

#[tokio::test]
async fn detach_delete_persists_node_and_edge_removal() {
    let path = path("detach_delete");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine
            .cypher("CREATE (a:P {n:1})-[:R {w:2}]->(b:P {n:2})")
            .await
            .unwrap();
        engine
            .cypher("MATCH (a:P {n:1}) DETACH DELETE a")
            .await
            .unwrap();
        drop(engine);
    }

    {
        let conn = Connection::open(&path).unwrap();
        let records = read_records(&conn);
        let keys: Vec<_> = records
            .iter()
            .map(|(k, n, i, _)| (*k, n.clone(), *i))
            .collect();
        assert!(
            keys.contains(&(1, "P".to_string(), 0)),
            "deleted node tombstone missing: {keys:?}"
        );
        assert!(
            keys.contains(&(2, "R".to_string(), 0)),
            "deleted edge tombstone missing: {keys:?}"
        );
        drop(conn);
    }

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(
                engine
                    .cypher("MATCH (p:P) RETURN p.n ORDER BY p.n")
                    .await
                    .unwrap()
            ),
            ["2"]
        );
        assert_eq!(
            rows(
                engine
                    .cypher("MATCH ()-[r:R]->() RETURN count(r)")
                    .await
                    .unwrap()
            ),
            ["0"]
        );
        drop(engine);
    }

    cleanup(&path);
}

#[tokio::test]
async fn legacy_format1_database_is_migrated_preserving_payload() {
    let path = path("legacy_migration");

    let original = graph_with_node("P", "n", Value::Int(42));
    let payload = encode_graph(&original).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE __crabgraph_state (\
             singleton INTEGER PRIMARY KEY CHECK(singleton = 1), \
             format_version INTEGER NOT NULL, payload BLOB NOT NULL)",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO __crabgraph_state VALUES (1, 1, ?)",
            params![payload],
        )
        .unwrap();
        drop(conn);
    }

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN p.n").await.unwrap()),
            ["42"]
        );
        drop(engine);
    }

    {
        let conn = Connection::open(&path).unwrap();
        let (version, revision, migrated_payload) = read_state(&conn);
        assert_eq!(version, 2, "format_version must be upgraded to 2");
        assert_eq!(revision, Some(0), "migration must add a revision column");
        assert_eq!(
            migrated_payload, payload,
            "migration must preserve the checkpoint payload"
        );
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM __crabgraph_records", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "the overlay table must exist and start empty");
        drop(conn);
    }

    cleanup(&path);
}

#[tokio::test]
async fn replace_graph_is_transactional_and_clears_records() {
    let path = path("replace_graph");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.cypher("CREATE (:P {n:1})").await.unwrap();

        engine.begin().unwrap();
        engine
            .replace_graph(graph_with_node("Q", "n", Value::Int(2)))
            .unwrap();
        engine.rollback().unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN p.n").await.unwrap()),
            ["1"]
        );

        engine
            .replace_graph(graph_with_node("Q", "n", Value::Int(2)))
            .unwrap();
        drop(engine);
    }

    {
        let conn = Connection::open(&path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM __crabgraph_records", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "replace_graph must clear the records table");

        let (_, _, payload) = read_state(&conn);
        let decoded = decode_graph(&payload).unwrap();
        assert!(
            decoded.labels().contains(&"Q".to_string()),
            "checkpoint must hold the new graph"
        );
        assert!(
            !decoded.labels().contains(&"P".to_string()),
            "checkpoint must drop the old graph"
        );
        drop(conn);
    }

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (q:Q) RETURN q.n").await.unwrap()),
            ["2"]
        );
        drop(engine);
    }

    cleanup(&path);
}

#[tokio::test]
async fn checkpoint_preserves_id_allocation_and_deleted_gaps() {
    let path = path("checkpoint_gaps");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine
            .cypher("CREATE (:P {n:1}), (:P {n:2}), (:P {n:3})")
            .await
            .unwrap();
        engine.cypher("MATCH (p:P {n:2}) DELETE p").await.unwrap();
        engine.checkpoint().unwrap();
        drop(engine);
    }

    {
        let conn = Connection::open(&path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM __crabgraph_records", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "checkpoint must compact the records away");
        drop(conn);
    }

    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.begin().unwrap();
        engine.cypher("CREATE (:P {n:4})").await.unwrap();
        let mut ids = rows(engine.cypher("MATCH (p:P) RETURN id(p)").await.unwrap());
        ids.sort();
        assert_eq!(
            ids,
            ["0", "2", "3"],
            "reopened allocation must skip the deleted id"
        );
        engine.rollback().unwrap();

        engine.cypher("CREATE (:P {n:5})").await.unwrap();
        let mut nodes = rows(
            engine
                .cypher("MATCH (p:P) RETURN id(p), p.n")
                .await
                .unwrap(),
        );
        nodes.sort();
        assert_eq!(
            nodes,
            ["0|1", "2|3", "3|5"],
            "rollback must restore the allocation counter"
        );
        drop(engine);
    }

    cleanup(&path);
}

#[tokio::test]
async fn rolling_back_checkpoint_restores_records_and_checkpoint_bytes() {
    let path = path("checkpoint_rollback");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.cypher("CREATE (:P {n:1})").await.unwrap();
    }
    let (before_state, before_records) = {
        let connection = Connection::open(&path).unwrap();
        (read_state(&connection), read_records(&connection))
    };
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.begin().unwrap();
        engine.cypher("CREATE (:P {n:2})").await.unwrap();
        engine.checkpoint().unwrap();
        engine.rollback().unwrap();
        assert_eq!(
            rows(engine.cypher("MATCH (p:P) RETURN p.n").await.unwrap()),
            ["1"]
        );
    }
    {
        let connection = Connection::open(&path).unwrap();
        assert_eq!(read_state(&connection), before_state);
        assert_eq!(read_records(&connection), before_records);
    }
    cleanup(&path);
}

#[tokio::test]
async fn cypher_temporal_properties_survive_incremental_and_checkpoint_reopen() {
    let path = path("temporal");
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.cypher("CREATE (:Event {at: datetime('2024-03-31T12:00+02:00[Europe/Stockholm]'), dates: [date('2024-02-29')], span: duration({seconds: 9007199254740993, nanoseconds: 1})})").await.unwrap();
    }
    for checkpoint in [false, true] {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(rows(engine.cypher("MATCH (e:Event) RETURN e.at.timezone, e.dates[0].day, e.span.seconds, e.span.nanosecondsOfSecond").await.unwrap()),
            ["Europe/Stockholm|29|9007199254740993|1"]);
        if !checkpoint {engine.checkpoint().unwrap();}
    }
    cleanup(&path);
}
