//! Adversarial integration tests for the durable `GraphEngine`.
//!
//! These tests probe the seams the happy-path suite leaves open: concurrent
//! writers, invalid transaction state, recovery from a commit-time conflict,
//! statement atomicity in autocommit mode, and agreement between the strict
//! SQL read path and the interpreter before and after a mutation. They are
//! written to detect *incorrect answers* (a silent lost update, a partially
//! applied statement, a strict-SQL result that disagrees with the graph)
//! rather than to assert that incomplete features have shipped.

#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};
use new_graph::engine::{GraphEngine, QueryResult, ReadMode};
use new_graph::ir::{PropertyGraph, edges_from_columns, nodes_from_columns};

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
        "crabgraph-adversarial-{name}-{}-{}.duckdb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn cleanup(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
}

/// Two connections that both begin from the same snapshot and both write must
/// not both commit. A silent double-commit is a lost update: the loser's
/// write would overwrite the winner's without any error.
#[tokio::test]
async fn conflicting_concurrent_writes_do_not_lose_an_update() {
    let path = path("conflict");
    {
        let mut a = GraphEngine::open(&path).unwrap();
        let mut b = GraphEngine::open(&path).unwrap();

        a.begin().unwrap();
        b.begin().unwrap();

        // The first writer writes and commits while the second only holds a
        // read snapshot (so no writer blocks a writer here).
        a.cypher("CREATE (:P {who:'a'}), (:P {who:'a2'})")
            .await
            .unwrap();
        let a_ok = a.commit().is_ok();

        // The second writer writes from the now-stale snapshot.
        let write = b
            .cypher("CREATE (:P {who:'b'}), (:P {who:'b2'}), (:P {who:'b3'})")
            .await;
        let b_ok = write.is_ok() && b.commit().is_ok();

        assert!(a_ok, "first writer failed to commit unexpectedly");
        assert!(
            !b_ok,
            "conflicting writer committed silently: a lost update was accepted"
        );
    }
    {
        // The surviving write must be visible in full — never a partial mix.
        let mut engine = GraphEngine::open(&path).unwrap();
        let names = rows(
            engine
                .cypher("MATCH (p:P) RETURN p.who ORDER BY p.who")
                .await
                .unwrap(),
        );
        assert_eq!(names, ["a", "a2"], "winner's write was not fully applied");
    }
    cleanup(&path);
}

#[tokio::test]
async fn invalid_transaction_state_is_rejected() {
    let mut engine = GraphEngine::in_memory().unwrap();

    // Commit/rollback without an active transaction.
    assert!(engine.commit().is_err(), "commit without begin must fail");
    assert!(
        engine.rollback().is_err(),
        "rollback without begin must fail"
    );
    assert!(!engine.in_transaction());

    engine.begin().unwrap();
    assert!(engine.in_transaction());

    // Nested begin.
    assert!(
        engine.begin().is_err(),
        "begin inside a transaction must fail"
    );

    engine.commit().unwrap();
    assert!(
        !engine.in_transaction(),
        "commit must clear the transaction flag"
    );

    // After commit there is again no active transaction.
    assert!(engine.commit().is_err());
    assert!(engine.rollback().is_err());
}

/// After a commit-time persistence conflict the losing engine must be able to
/// roll back, observe the committed state, and keep writing. A failed commit
/// that leaves the engine wedged (unable to roll back or to see the winner's
/// data) is a real defect.
#[tokio::test]
async fn rollback_recovers_after_persistence_conflict() {
    let path = path("rollback-conflict");
    {
        let mut a = GraphEngine::open(&path).unwrap();
        a.cypher("CREATE (:P {n:1})").await.unwrap();

        let mut b = GraphEngine::open(&path).unwrap();
        b.begin().unwrap();

        // a writes and commits while b holds a read snapshot.
        a.cypher("CREATE (:P {n:2})").await.unwrap();

        // DuckDB may detect the conflict at UPDATE or at COMMIT.
        let write = b.cypher("CREATE (:P {n:3})").await;
        assert!(
            write.is_err() || b.commit().is_err(),
            "stale writer must conflict"
        );

        // Recovery: rollback must clear the failed transaction.
        b.rollback().unwrap();
        assert!(
            !b.in_transaction(),
            "rollback must clear the transaction flag"
        );

        // b must now see the committed state (n:1, n:2) and not its own
        // rolled-back write (n:3).
        assert_eq!(
            rows(
                b.cypher("MATCH (p:P) RETURN p.n ORDER BY p.n")
                    .await
                    .unwrap()
            ),
            ["1", "2"]
        );

        // b remains usable.
        b.cypher("CREATE (:P {n:4})").await.unwrap();
        assert_eq!(
            rows(
                b.cypher("MATCH (p:P) RETURN p.n ORDER BY p.n")
                    .await
                    .unwrap()
            ),
            ["1", "2", "4"]
        );
    }
    cleanup(&path);
}

/// A failed autocommit statement must not leave partially applied writes. The
/// SET applies before DELETE discovers the incident relationship, so the
/// statement fails midway.
#[tokio::test]
async fn failed_autocommit_statement_leaves_no_partial_mutations() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {n:1})-[:R]->(:P {n:2})")
        .await
        .unwrap();

    assert!(
        engine
            .cypher("MATCH (p:P {n:1}) SET p.n = 99 DELETE p")
            .await
            .is_err(),
        "statement must fail"
    );

    // Neither the SET nor any part of the DELETE may survive.
    assert_eq!(
        rows(
            engine
                .cypher("MATCH (p:P) RETURN p.n ORDER BY p.n")
                .await
                .unwrap()
        ),
        ["1", "2"]
    );
    assert_eq!(
        rows(
            engine
                .cypher("MATCH (p:P {n:99}) RETURN count(p)")
                .await
                .unwrap()
        ),
        ["0"]
    );
}

/// In strict (`SqlOnly`) mode, every read must agree with the interpreter
/// path, both before and after a mutation. When the SQL backend cannot lower a
/// plan it must return an explicit error — it may never return a silently
/// wrong answer. The `RETURN 1 + 2` case is guaranteed to lower and therefore
/// anchors the strict path as genuinely exercised.
#[tokio::test]
async fn strict_sql_agrees_with_interpreter_before_and_after_mutation() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {name:'a', n:1})-[:R {w:10}]->(:P {name:'b', n:2})")
        .await
        .unwrap();

    let queries = [
        "RETURN 1 + 2 AS answer",
        "MATCH (p:P) RETURN p.n ORDER BY p.n",
        "MATCH (p:P) RETURN p.name ORDER BY p.name",
        "MATCH (p:P) RETURN count(p)",
    ];

    // Before mutation: hybrid and strict must agree (or strict must error).
    assert_strict_agrees(&mut engine, &queries).await;

    // A mutation, then again.
    engine.cypher("CREATE (:P {name:'c', n:3})").await.unwrap();
    engine
        .cypher("MATCH (p:P {name:'a'}) SET p.n = 100")
        .await
        .unwrap();

    assert_strict_agrees(&mut engine, &queries).await;
}

async fn assert_strict_agrees(engine: &mut GraphEngine, queries: &[&str]) {
    for query in queries {
        engine.set_read_mode(ReadMode::Hybrid);
        let hybrid = rows(engine.cypher(query).await.unwrap());

        engine.set_read_mode(ReadMode::SqlOnly);
        match engine.cypher(query).await {
            Ok(result) => {
                assert_eq!(
                    rows(result),
                    hybrid,
                    "strict SQL disagreed with interpreter for `{query}`"
                );
            }
            // An explicit error is acceptable: decline, never fabricate.
            Err(_) => {}
        }
    }

    // The strict path must have actually produced a DuckDB answer for the
    // constant expression; otherwise the whole comparison is vacuous.
    engine.set_read_mode(ReadMode::SqlOnly);
    let result = engine.cypher("RETURN 1 + 2 AS answer").await.unwrap();
    assert_eq!(rows(result), ["3"]);
}

/// Importing an Arrow-backed `PropertyGraph` via `replace_graph` must persist
/// the full graph (nodes and edges) across a reopen, and must not collide with
/// subsequent writes.
#[tokio::test]
async fn imported_arrow_graph_persists_across_reopen() {
    let path = path("import");
    {
        let mut engine = GraphEngine::open(&path).unwrap();

        let mut graph = PropertyGraph::new();
        graph.add_nodes(nodes_from_columns(
            "Person",
            vec![(
                "name",
                Arc::new(StringArray::from(vec!["alice", "bob"])) as ArrayRef,
            )],
        ));
        graph
            .add_edges(edges_from_columns(
                "KNOWS",
                "Person",
                "Person",
                vec![0],
                vec![1],
                vec![("since", Arc::new(Int64Array::from(vec![2020])) as ArrayRef)],
            ))
            .unwrap();

        engine.replace_graph(graph).unwrap();
    }
    {
        let mut engine = GraphEngine::open(&path).unwrap();

        assert_eq!(
            rows(
                engine
                    .cypher("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name, r.since, b.name")
                    .await
                    .unwrap()
            ),
            ["alice|2020|bob"]
        );
        assert_eq!(
            rows(
                engine
                    .cypher("MATCH (p:Person) RETURN p.name ORDER BY p.name")
                    .await
                    .unwrap()
            ),
            ["alice", "bob"]
        );

        // The imported graph must accept new writes without losing old data.
        engine
            .cypher("CREATE (:Person {name:'carol'})")
            .await
            .unwrap();
        assert_eq!(
            rows(
                engine
                    .cypher("MATCH (p:Person) RETURN p.name ORDER BY p.name")
                    .await
                    .unwrap()
            ),
            ["alice", "bob", "carol"]
        );
    }
    cleanup(&path);
}
