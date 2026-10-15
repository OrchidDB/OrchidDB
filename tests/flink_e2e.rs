//! Opt-in real Flink/Kafka/Iceberg workflow, orchestrated by tools/diagnostics/flink-e2e/run.py.
#![cfg(all(feature = "kafka", feature = "duckdb"))]
use async_trait::async_trait;
use orchiddb::{compiler::Authorization, session::Query, streaming::*};
use rdkafka::{
    ClientConfig,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
struct Policy;
#[async_trait]
impl Authorizer for Policy {
    async fn check(
        &self,
        s: &Authorization,
        _: &str,
        id: &str,
        permission: &str,
    ) -> Result<bool, String> {
        Ok(s.subject_type == "user"
            && s.subject_id == "alice"
            && id != "forbidden"
            && (permission == "edit" || id != "secret"))
    }
}
fn schema(table: &str) -> StreamSchema {
    StreamSchema {
        tables: BTreeMap::from([(
            table.into(),
            Table {
                label: if table == "input" { "Delta" } else { "Total" }.into(),
                resource_type: "entity".into(),
                read_permission: "view".into(),
                write_permission: "edit".into(),
                properties: json!({"type":"object"}),
                edge: None,
            },
        )]),
    }
}
fn change(table: &str, key: &str, version: u64, value: Option<i64>) -> Change {
    Change {
        table: table.into(),
        key: key.into(),
        version,
        data: value.map(|v| {
            if table == "input" {
                json!({"delta":v})
            } else {
                json!({"total":v})
            }
            .as_object()
            .unwrap()
            .clone()
        }),
    }
}
fn query(text: &str) -> Query {
    let mut q = Query::cypher(text);
    q.authorization = Some(Authorization::new("user", "alice"));
    q
}
fn rows(batch: &arrow::record_batch::RecordBatch) -> Vec<Vec<String>> {
    (0..batch.num_rows())
        .map(|r| {
            (0..batch.num_columns())
                .map(|c| arrow::util::display::array_value_to_string(batch.column(c), r).unwrap())
                .collect()
        })
        .collect()
}
#[tokio::test]
#[ignore = "requires the Flink E2E orchestration script"]
async fn flink_materialization_and_iceberg_hybrid() {
    let phase = std::env::var("ORCHID_FLINK_PHASE").unwrap();
    let prefix = std::env::var("ORCHID_FLINK_TOPIC").unwrap();
    let input_topic = format!("{prefix}-input");
    let output_topic = format!("{prefix}-output");
    let path = std::env::var("ORCHID_FLINK_SNAPSHOT").unwrap();
    let mut config = ClientConfig::new();
    config
        .set("bootstrap.servers", "127.0.0.1:19092")
        .set("linger.ms", "0");
    if phase == "setup" {
        let admin: AdminClient<DefaultClientContext> = config.create().unwrap();
        let result = admin
            .create_topics(
                &[
                    NewTopic::new(&input_topic, 1, TopicReplication::Fixed(1)),
                    NewTopic::new(&output_topic, 1, TopicReplication::Fixed(1)),
                ],
                &AdminOptions::new(),
            )
            .await
            .unwrap();
        assert!(result.iter().all(Result::is_ok));
    }
    let input = StreamConnection::new(
        schema("input"),
        input_topic.clone(),
        Arc::new(KafkaLog::new(config.clone(), input_topic).unwrap()),
        Arc::new(Policy),
        Limits::default(),
    )
    .unwrap();
    let log = Arc::new(KafkaLog::new(config, output_topic.clone()).unwrap());
    let output = StreamConnection::new(
        schema("totals"),
        output_topic.clone(),
        log.clone(),
        Arc::new(Policy),
        Limits::default(),
    )
    .unwrap();
    let subject = Authorization::new("user", "alice");
    if phase == "setup" {
        assert!(
            input
                .insert(&subject, change("input", "forbidden", 1, Some(1)))
                .await
                .is_err()
        );
        for (key, version, value) in [
            ("a", 1, Some(10)),
            ("b", 1, Some(7)),
            ("a", 2, Some(5)),
            ("b", 2, None),
            ("secret", 1, Some(99)),
        ] {
            input
                .insert(&subject, change("input", key, version, value))
                .await
                .unwrap();
        }
        return;
    }
    let previous: Option<Snapshot> = if phase == "phase2" {
        Some(serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap())
    } else {
        None
    };
    let start = previous
        .as_ref()
        .map(|s| s.offsets.clone())
        .unwrap_or(BTreeMap::from([(0, 0)]));
    let mut sub = output
        .subscribe(
            query("MATCH (t:Total) WHERE t.total >= 12 RETURN t.total"),
            start,
        )
        .await
        .unwrap();
    if phase == "phase2" {
        for (key, version, value) in [
            ("a", 3, Some(3)),
            ("a", 3, Some(3)),
            ("b", 3, Some(2)),
            ("a", 4, None),
            ("d", 1, Some(15)),
            ("secret", 2, Some(1)),
        ] {
            input
                .insert(&subject, change("input", key, version, value))
                .await
                .unwrap();
        }
    }
    let expected = if phase == "phase1" {
        vec![
            ("a", 1, Some(vec![])),
            ("b", 1, Some(vec![])),
            ("a", 2, Some(vec![vec!["15".into()]])),
            ("b", 2, None),
        ]
    } else {
        vec![
            ("a", 3, Some(vec![vec!["18".into()]])),
            ("b", 3, Some(vec![])),
            ("a", 4, None),
            ("d", 1, Some(vec![vec!["15".into()]])),
        ]
    };
    for (key, version, values) in expected {
        let frame = tokio::time::timeout(Duration::from_secs(90), sub.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!((&*frame.key, frame.version), (key, version));
        assert_eq!(frame.rows.as_ref().map(rows), values);
    }
    let expected: Vec<Vec<String>> = if phase == "phase1" {
        vec![vec!["15".into()]]
    } else {
        vec![vec!["2".into()], vec!["15".into()]]
    };
    assert_eq!(
        rows(
            &output
                .query(query("MATCH (t:Total) RETURN t.total ORDER BY t.total"))
                .await
                .unwrap()
        ),
        expected
    );
    if phase == "phase1" {
        let snapshot = Snapshot {
            source: output_topic,
            offsets: log.bounds().await.unwrap().high,
            changes: vec![
                change("totals", "a", 2, Some(15)),
                change("totals", "b", 2, None),
                change("totals", "secret", 1, Some(99)),
            ],
        };
        std::fs::write(path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    } else {
        let metadata = std::env::var("ORCHID_ICEBERG_METADATA").unwrap();
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("SET custom_extension_repository='https://extensions.duckdb.org'; INSTALL iceberg; LOAD iceberg").unwrap();
        let mut snapshot = DuckDbSnapshot {
            connection: &db,
            snapshot_sql: format!(
                "SELECT manifest_json FROM iceberg_scan('{}')",
                metadata.replace('\'', "''")
            ),
        };
        assert_eq!(
            rows(
                &output
                    .query_hybrid(
                        query("MATCH (t:Total) RETURN t.total ORDER BY t.total"),
                        &mut snapshot
                    )
                    .await
                    .unwrap()
            ),
            expected
        );
        assert_eq!(
            rows(
                &output
                    .query_hybrid(
                        query("MATCH (t:Total) WHERE t.total >= 12 RETURN count(t)"),
                        &mut snapshot
                    )
                    .await
                    .unwrap()
            ),
            vec![vec!["1"]]
        );
    }
}
