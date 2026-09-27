#![cfg(feature = "duckdb")]
use arrow::array::{ArrayRef, StringArray};
use arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;
use orchiddb::engine::{GraphEngine, ManagedGraphCheckpoint, QueryResult};
use orchiddb::ir::{
    ElementId, PropertyGraph, Value,
    catalog::{EdgeTable, NodeTable},
};
use std::sync::Arc;

fn key(value: &str) -> ElementId {
    ElementId::new(ScalarValue::Utf8(Some(value.into()))).unwrap()
}
fn fixture() -> PropertyGraph {
    let mut graph = PropertyGraph::new();
    graph
        .add_keyed_nodes(
            NodeTable {
                label: "N".into(),
                batch: RecordBatch::try_from_iter(vec![(
                    "name",
                    Arc::new(StringArray::from(vec!["alpha", "beta"])) as ArrayRef,
                )])
                .unwrap(),
            },
            vec![key("node-a"), key("node-b")],
        )
        .unwrap();
    graph
        .add_keyed_edges(
            EdgeTable {
                rel_type: "E".into(),
                src_label: "N".into(),
                dst_label: "N".into(),
                batch: RecordBatch::try_from_iter(vec![
                    (
                        "__src_id",
                        Arc::new(StringArray::from(vec!["node-a"])) as ArrayRef,
                    ),
                    (
                        "__dst_id",
                        Arc::new(StringArray::from(vec!["node-b"])) as ArrayRef,
                    ),
                ])
                .unwrap(),
            },
            vec![key("edge-a")],
        )
        .unwrap();
    graph
}
fn rows(result: QueryResult) -> Vec<String> {
    let batch = result.returned.batch;
    (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[tokio::test]
async fn reusable_checkpoint_is_isolated_transactional_and_persisted_for_other_engines() {
    let path = std::env::temp_dir().join(format!(
        "orchiddb-reusable-checkpoint-{}-{}.duckdb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let original = fixture();
    let expected_payload = orchiddb::storage::encode_graph(&original).unwrap();
    let checkpoint = ManagedGraphCheckpoint::new(original.clone()).unwrap();
    // Mutating a graph that shared the original overlay cannot alter the handle.
    original
        .set_property(
            &Value::Node {
                label: "N".into(),
                id: key("node-a"),
            },
            "name",
            Value::String("outside".into()),
        )
        .unwrap();
    {
        let mut first = GraphEngine::open(&path).unwrap();
        first.restore_graph_checkpoint(&checkpoint).unwrap();
        assert_eq!(
            rows(first.cypher("MATCH (n) RETURN count(n)").await.unwrap()),
            ["2"]
        );
        first
            .cypher("MATCH (n:N) SET n.name='changed'")
            .await
            .unwrap();
        first.begin().unwrap();
        first.restore_graph_checkpoint(&checkpoint).unwrap();
        assert_eq!(
            rows(
                first
                    .cypher("MATCH (n:N) RETURN n.name ORDER BY n.name")
                    .await
                    .unwrap()
            ),
            ["alpha", "beta"]
        );
        first.rollback().unwrap();
        assert_eq!(
            rows(
                first
                    .cypher("MATCH (n:N) RETURN n.name ORDER BY n.name")
                    .await
                    .unwrap()
            ),
            ["changed", "changed"]
        );
        let mut second = GraphEngine::open(&path).unwrap();
        second.restore_graph_checkpoint(&checkpoint).unwrap();
        // Revision advancement must invalidate a different engine's loaded graph.
        assert_eq!(
            rows(
                first
                    .cypher("MATCH (n:N) RETURN n.name ORDER BY n.name")
                    .await
                    .unwrap()
            ),
            ["alpha", "beta"]
        );
        first
            .cypher("MATCH (n:N) SET n.name='again'")
            .await
            .unwrap();
        first.restore_graph_checkpoint(&checkpoint.clone()).unwrap();
    }
    {
        let sql = duckdb::Connection::open(&path).unwrap();
        let (revision, payload): (i64, Vec<u8>) = sql
            .query_row(
                "SELECT revision,payload FROM __orchiddb_state WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(revision >= 5);
        assert_eq!(
            payload, expected_payload,
            "restore must persist the pristine preencoded bytes"
        );
        let records: i64 = sql
            .query_row("SELECT count(*) FROM __orchiddb_records", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            records, 0,
            "replacement must clear prior incremental mutations"
        );
    }
    {
        let mut reopened = GraphEngine::open(&path).unwrap();
        assert_eq!(
            rows(
                reopened
                    .cypher("MATCH (a:N)-[:E]->(b:N) RETURN a.name,b.name")
                    .await
                    .unwrap()
            ),
            ["alpha|beta"]
        );
    }
    std::fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn checkpoint_restore_rejects_mapped_engines_without_changing_their_tables() {
    use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping};
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE items(id VARCHAR PRIMARY KEY); INSERT INTO items VALUES ('original')",
        )
        .unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("Item", "items", "id"));
    let mut engine = GraphEngine::mapped(connection, Arc::new(mapping)).unwrap();
    let checkpoint = ManagedGraphCheckpoint::new(fixture()).unwrap();
    assert!(
        engine
            .restore_graph_checkpoint(&checkpoint)
            .unwrap_err()
            .contains("mapped schema")
    );
    assert!(!engine.in_transaction());
    assert_eq!(
        rows(
            engine
                .cypher("MATCH (n:Item) RETURN count(n)")
                .await
                .unwrap()
        ),
        ["1"]
    );
}
