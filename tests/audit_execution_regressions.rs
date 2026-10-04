#![cfg(feature = "duckdb")]

use arrow::{
    array::{Int64Array, RecordBatch},
    datatypes::{DataType, Field, Schema},
};
use orchiddb::engine::GraphEngine;
use orchiddb::ir::rel::sql::{DuckDbExecutor, SqlExecutor, SqlValue, TableData};
use orchiddb::ir::{
    PropertyGraph,
    plan::{GraphPlan, LabelExpr, Node},
    policy::GraphPlanPolicy,
};
use std::sync::Arc;

fn table() -> TableData {
    let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Int64, false)]));
    TableData {
        name: "audit_scan".into(),
        schema: schema.clone(),
        batches: vec![
            RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![7]))]).unwrap(),
        ],
    }
}

#[test]
fn cache_invalidation_reclaims_owned_scan_tables() {
    let mut executor = DuckDbExecutor::new();
    for _ in 0..5 {
        executor.execute_batch("SELECT 1").unwrap();
        executor
            .run_with_tables(&[table()], &[], "SELECT * FROM audit_scan")
            .unwrap();
    }
    let count = executor
        .run(
            &[],
            "SELECT count(*) FROM duckdb_tables() WHERE starts_with(table_name, '__orchid_scan_')",
        )
        .unwrap();
    assert_eq!(count, vec![vec![SqlValue::Int(1)]]);
}

#[test]
fn scan_preserves_caller_owned_temp_view() {
    let mut executor = DuckDbExecutor::new();
    executor
        .execute_batch("CREATE TEMP VIEW audit_scan AS SELECT 42 AS v")
        .unwrap();
    assert!(
        executor
            .run_with_tables(&[table()], &[], "SELECT * FROM audit_scan")
            .is_err()
    );
    assert_eq!(
        executor.run(&[], "SELECT * FROM audit_scan").unwrap(),
        vec![vec![SqlValue::Int(42)]]
    );
}

#[tokio::test]
async fn cancellation_cleans_up_automatic_transaction() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let mut engine = GraphEngine::in_memory().unwrap();
    let mut future = Box::pin(engine.cypher("UNWIND range(1,10000) AS i CREATE (:P {i:i})"));
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(future.as_mut().poll(&mut context), Poll::Pending),
        "probe must cancel a pending query"
    );
    drop(future);
    assert!(
        !engine.in_transaction(),
        "cancelled autocommit query left an active transaction"
    );
}

#[tokio::test]
async fn successful_write_after_cancellation_survives_reopen() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let path = std::env::temp_dir().join(format!(
        "orchid-round2-cancel-{}.duckdb",
        std::process::id()
    ));
    assert!(!path.exists());
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        let mut future =
            Box::pin(engine.cypher("UNWIND range(1,10000) AS i CREATE (:Cancelled {i:i})"));
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        drop(future);
        engine
            .cypher("CREATE (:Acknowledged {value:42})")
            .await
            .expect("next write reports success");
    }
    let result = {
        let mut engine = GraphEngine::open(&path).unwrap();
        strings(
            engine
                .cypher("MATCH (n:Acknowledged) RETURN count(n)")
                .await
                .unwrap(),
        )
    };
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        result,
        vec![vec!["1"]],
        "successful autocommit write was lost after cancelling the previous query"
    );
}

#[tokio::test]
async fn negated_conjunction_retains_nonmatching_nodes() {
    let graph = PropertyGraph::new();
    graph.insert_node("A", Default::default());
    graph.insert_node("B", Default::default());
    let c = graph.insert_node("C", Default::default());
    graph
        .set_node_labels(&c, vec!["C".into(), "A".into()])
        .unwrap();
    let not = |expr| LabelExpr::Not(Box::new(expr));
    for (labels, expected) in [
        (not(LabelExpr::AllOf(vec!["A".into()])), vec!["B"]),
        (
            not(LabelExpr::AllOf(vec!["A".into(), "C".into()])),
            vec!["A", "B"],
        ),
        (not(not(LabelExpr::AllOf(vec!["A".into()]))), vec!["A", "C"]),
    ] {
        let plan = GraphPlan::new(
            GraphPlanPolicy::cypher(),
            Node::GraphNodeScan {
                graph: "default".into(),
                binding: "n".into(),
                labels,
            },
        );
        let (rows, _) =
            orchiddb::ir::rel::runtime::execute_rows_with_jvm(&plan, &graph, Default::default())
                .await
                .unwrap();
        let mut actual = rows
            .iter()
            .map(|row| match &row.bindings["n"] {
                orchiddb::ir::Value::Node { label, .. } => label.as_str(),
                other => panic!("expected node, got {other:?}"),
            })
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn graphson_equivalent_integer_encodings_resolve_endpoint() {
    let path = std::env::temp_dir().join(format!("orchid-round2-{}.json", std::process::id()));
    std::fs::write(&path, r#"
{"id":{"@type":"gx:BigInteger","@value":"1"},"label":"P","outE":{"E":[{"id":{"@type":"g:Int32","@value":3},"inV":{"@type":"gx:BigInteger","@value":2}}]}}
{"id":{"@type":"gx:BigInteger","@value":"2"},"label":"P"}
"#).unwrap();
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine
        .gremlin(&format!(
            "g.io({}).read()",
            serde_json::to_string(path.to_str().unwrap()).unwrap()
        ))
        .await;
    std::fs::remove_file(&path).unwrap();
    assert!(
        result.is_ok(),
        "equivalent endpoint encoding rejected: {:?}",
        result.err()
    );
    assert_eq!(
        strings(engine.gremlin("g.E().count()").await.unwrap()),
        vec![vec!["1"]]
    );
}

#[tokio::test]
async fn execution_preserves_unbounded_query_semantics() {
    use orchiddb::ir::rel::{RelBackend, sql};
    use orchiddb::ir::{edges_from_columns, nodes_from_columns};
    use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("i", Arc::new(Int64Array::from_iter_values(0..9)))],
    ));
    graph
        .add_edges(edges_from_columns(
            "NEXT",
            "P",
            "P",
            (0..8).collect(),
            (1..9).collect(),
            vec![],
        ))
        .unwrap();
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH (p:P)-[:NEXT*1..]->(f) WHERE p.i=0 RETURN count(*)").unwrap())
        .unwrap();
    let backend = RelBackend::new();
    let lowered = backend.lower(&plan, &graph).unwrap();
    let result = sql::execute_lowered_sql(&mut DuckDbExecutor::new(), &lowered)
        .await
        .unwrap();
    let count = arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap();
    assert_eq!(count, "8");
}

#[tokio::test]
async fn cancellation_preserves_explicit_transaction_and_prior_writes() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.begin().unwrap();
    engine.cypher("CREATE (:P {i:0})").await.unwrap();
    let mut future = Box::pin(engine.cypher("UNWIND range(1,10000) AS i CREATE (:P {i:i})"));
    assert!(matches!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    drop(future);
    assert!(engine.in_transaction());
    assert_eq!(
        strings(engine.cypher("MATCH (n:P) RETURN count(n)").await.unwrap()),
        vec![vec!["1"]]
    );
    engine.cypher("CREATE (:P {i:42})").await.unwrap();
    engine.commit().unwrap();
    assert_eq!(
        strings(engine.cypher("MATCH (n:P) RETURN count(n)").await.unwrap()),
        vec![vec!["2"]]
    );
}

#[test]
fn scan_cleanup_handles_transaction_endings_and_caller_replacements() {
    let mut executor = DuckDbExecutor::new();
    for ending in ["COMMIT", "ROLLBACK"] {
        executor
            .run_with_tables(&[table()], &[], "SELECT * FROM audit_scan")
            .unwrap();
        // Raw BEGIN also covers cached tables that predate the transaction.
        executor.run(&[], "BEGIN").unwrap();
        executor.connection().unwrap(); // invalidates cached data inside the transaction
        executor
            .run_with_tables(&[table()], &[], "SELECT * FROM audit_scan")
            .unwrap();
        if ending == "COMMIT" {
            executor.commit().unwrap();
        } else {
            executor.rollback().unwrap();
        }
        assert_eq!(executor.run(&[], "SELECT count(*) FROM duckdb_tables() WHERE starts_with(table_name, '__orchid_scan_')").unwrap(), vec![vec![SqlValue::Int(0)]]);
    }
    executor
        .run_with_tables(&[table()], &[], "SELECT * FROM audit_scan")
        .unwrap();
    let names = executor.run(&[], "SELECT table_name FROM duckdb_tables() WHERE starts_with(table_name, '__orchid_scan_')").unwrap();
    let SqlValue::Text(name) = &names[0][0] else {
        panic!("missing table name")
    };
    executor.run(&[], "BEGIN").unwrap();
    executor
        .connection()
        .unwrap()
        .execute_batch(&format!(
            "CREATE OR REPLACE TEMP TABLE \"{name}\" AS SELECT 42 AS v"
        ))
        .unwrap();
    executor.commit().unwrap();
    assert_eq!(
        executor
            .run(&[], &format!("SELECT v FROM \"{name}\""))
            .unwrap(),
        vec![vec![SqlValue::Int(42)]]
    );
}

fn strings(result: orchiddb::engine::QueryResult) -> Vec<Vec<String>> {
    let batch = result.returned.batch;
    (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn production_path_and_gremlin_controls() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (a:Person {name:'alice'}), (b:Person {name:'bob'}), (c:Person {name:'carol'}), (a)-[:KNOWS]->(b), (a)-[:KNOWS]->(c), (b)-[:KNOWS]->(c), (city:City {name:'paris'}), (b)-[:LIVES_IN]->(city)").await.unwrap();
    assert_eq!(strings(engine.cypher("MATCH (p:Person)-[e:KNOWS*1..]->(f) WHERE p.name='alice' RETURN count(DISTINCT e)").await.unwrap()), vec![vec!["3"]]);
    assert_eq!(strings(engine.cypher("MATCH (p:Person)-[e:KNOWS*2..2]-(f) WHERE p.name='alice' RETURN f.name, size(e) ORDER BY f.name").await.unwrap()), vec![vec!["bob", "2"], vec!["carol", "2"]]);
    assert_eq!(strings(engine.cypher("MATCH p=(a:Person)-[:KNOWS*1..1]->(:Person)-[:LIVES_IN]->(:City) WHERE a.name='alice' RETURN length(p)").await.unwrap()), vec![vec!["2"]]);
    assert_eq!(
        strings(
            engine
                .gremlin("g.inject(\"feature\",\"test\",null).length(Scope.local)")
                .await
                .unwrap()
        ),
        vec![vec!["7"], vec!["4"], vec![""]]
    );
}
