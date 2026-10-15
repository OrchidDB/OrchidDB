use async_trait::async_trait;
use futures::stream::BoxStream;
use orchiddb::{compiler::Authorization, session::Query, streaming::*};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Log {
    records: Mutex<Vec<Change>>,
    low: i64,
}
#[async_trait]
impl ChangeLog for Log {
    async fn append(&self, change: &Change) -> Result<(i32, i64), String> {
        let mut records = self.records.lock().unwrap();
        records.push(change.clone());
        Ok((0, records.len() as i64 - 1))
    }
    async fn bounds(&self) -> Result<Bounds, String> {
        Ok(Bounds {
            low: BTreeMap::from([(0, self.low)]),
            high: BTreeMap::from([(0, self.records.lock().unwrap().len() as i64)]),
        })
    }
    fn follow(
        &self,
        offsets: Offsets,
    ) -> Result<BoxStream<'static, Result<LogItem, String>>, String> {
        let records: Vec<_> = self
            .records
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .skip(offsets[&0] as usize)
            .map(|(offset, change)| {
                Ok(LogItem::Record(LogRecord {
                    partition: 0,
                    offset: offset as i64,
                    change: change.clone(),
                }))
            })
            .collect();
        Ok(Box::pin(futures::stream::iter(records)))
    }
}
struct Auth;
#[async_trait]
impl Authorizer for Auth {
    async fn check(
        &self,
        subject: &Authorization,
        _: &str,
        key: &str,
        permission: &str,
    ) -> Result<bool, String> {
        if subject.subject_id == "error" {
            return Err("auth unavailable".into());
        }
        Ok(subject.subject_id == "alice" && key != "secret" && permission != "forbidden")
    }
}
fn schema() -> StreamSchema {
    StreamSchema {
        tables: BTreeMap::from([(
            "people".into(),
            Table {
                label: "Person".into(),
                resource_type: "person".into(),
                read_permission: "view".into(),
                write_permission: "edit".into(),
                properties: json!({"type":"object","required":["name"],"properties":{"name":{"type":"string"},"age":{"type":"integer"}}}),
                edge: None,
            },
        )]),
    }
}
fn change(key: &str, version: u64, name: Option<&str>) -> Change {
    Change {
        table: "people".into(),
        key: key.into(),
        version,
        data: name.map(|n| json!({"name":n,"age":30}).as_object().unwrap().clone()),
    }
}
fn query(text: &str) -> Query {
    let mut q = Query::cypher(text);
    q.authorization = Some(Authorization::new("user", "alice"));
    q
}
fn connection(log: Arc<Log>) -> StreamConnection {
    StreamConnection::new(
        schema(),
        "topic-v1".into(),
        log,
        Arc::new(Auth),
        Limits::default(),
    )
    .unwrap()
}
struct Ice(Snapshot);
#[async_trait(?Send)]
impl SnapshotSource for Ice {
    async fn load(&mut self) -> Result<Snapshot, String> {
        Ok(self.0.clone())
    }
}
fn strings(batch: &arrow::record_batch::RecordBatch) -> Vec<Vec<String>> {
    (0..batch.num_rows())
        .map(|r| {
            (0..batch.num_columns())
                .map(|c| arrow::util::display::array_value_to_string(batch.column(c), r).unwrap())
                .collect()
        })
        .collect()
}
#[tokio::test]
async fn insert_checks_schema_and_authorization_before_producing() {
    let log = Arc::new(Log::default());
    let c = connection(log.clone());
    assert!(
        c.insert(
            &Authorization::new("user", "bob"),
            change("a", 1, Some("A"))
        )
        .await
        .is_err()
    );
    assert!(
        c.insert(
            &Authorization::new("user", "error"),
            change("a", 1, Some("A"))
        )
        .await
        .is_err()
    );
    let mut invalid = change("a", 1, Some("A"));
    invalid.data = Some(json!({"name":42}).as_object().unwrap().clone());
    assert!(
        c.insert(&Authorization::new("user", "alice"), invalid)
            .await
            .is_err()
    );
    assert!(log.records.lock().unwrap().is_empty());
    assert_eq!(
        c.insert(
            &Authorization::new("user", "alice"),
            change("a", 1, Some("A"))
        )
        .await
        .unwrap(),
        (0, 0)
    );
}
#[tokio::test]
async fn hybrid_merges_before_filtering_and_counting_and_keeps_tombstones() {
    let log = Arc::new(Log {
        records: Mutex::new(vec![
            change("a", 1, Some("old")),
            change("a", 2, Some("new")),
            change("b", 2, None),
            change("c", 1, Some("same")),
            change("d", 1, Some("same")),
            change("secret", 1, Some("hidden")),
            change("b", 1, Some("resurrection")),
        ]),
        low: 0,
    });
    let c = connection(log);
    let mut ice = Ice(Snapshot {
        source: "topic-v1".into(),
        offsets: BTreeMap::from([(0, 1)]),
        changes: vec![
            change("a", 1, Some("old")),
            change("b", 1, Some("deleted")),
            change("c", 1, Some("same")),
        ],
    });
    let result = c
        .query_hybrid(
            query("MATCH (p:Person) RETURN p.name AS name ORDER BY name"),
            &mut ice,
        )
        .await
        .unwrap();
    assert_eq!(
        strings(&result),
        vec![vec!["new"], vec!["same"], vec!["same"]]
    );
    let result = c
        .query_hybrid(
            query("MATCH (p:Person) WHERE p.name = 'old' RETURN count(p) AS n"),
            &mut ice,
        )
        .await
        .unwrap();
    assert_eq!(strings(&result), vec![vec!["0"]]);
    let result = c
        .query(query("MATCH (p:Person) RETURN count(p) AS n"))
        .await
        .unwrap();
    assert_eq!(strings(&result), vec![vec!["3"]]);
}
#[tokio::test]
async fn hybrid_rejects_conflicts_and_retention_gaps() {
    let log = Arc::new(Log {
        records: Mutex::new(vec![change("a", 1, Some("other"))]),
        low: 0,
    });
    let c = connection(log);
    let mut ice = Ice(Snapshot {
        source: "topic-v1".into(),
        offsets: BTreeMap::from([(0, 0)]),
        changes: vec![change("a", 1, Some("original"))],
    });
    assert!(
        c.query_hybrid(query("MATCH (p) RETURN p"), &mut ice)
            .await
            .unwrap_err()
            .contains("conflicting")
    );
    ice.0.source = "wrong".into();
    assert!(
        c.query_hybrid(query("MATCH (p) RETURN p"), &mut ice)
            .await
            .unwrap_err()
            .contains("incarnation")
    );
    let c = connection(Arc::new(Log {
        records: Mutex::new(vec![change("a", 1, Some("a"))]),
        low: 1,
    }));
    assert!(
        c.query(query("MATCH (p) RETURN p"))
            .await
            .unwrap_err()
            .contains("retention gap")
    );
}
#[tokio::test]
async fn stream_emits_replacements_and_deletes_but_not_duplicate_versions() {
    let log = Arc::new(Log {
        records: Mutex::new(vec![
            change("a", 1, Some("A")),
            change("a", 1, Some("A")),
            change("secret", 1, Some("hidden")),
            change("a", 2, Some("B")),
            change("a", 3, None),
        ]),
        low: 0,
    });
    let c = connection(log);
    let mut q = query("MATCH (p:Person) WHERE p.name = $name RETURN p.name AS name");
    q.parameters.insert("name".into(), json!("A"));
    let mut stream = c.subscribe(q, BTreeMap::from([(0, 0)])).await.unwrap();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(strings(first.rows.as_ref().unwrap()), vec![vec!["A"]]);
    assert_eq!(first.offsets[&0], 1);
    let second = stream.next().await.unwrap().unwrap();
    assert_eq!(second.version, 2);
    assert_eq!(second.rows.unwrap().num_rows(), 0);
    assert_eq!(second.offsets[&0], 4);
    let third = stream.next().await.unwrap().unwrap();
    assert!(third.deleted);
    assert!(third.rows.is_none());
    assert_eq!(third.offsets[&0], 5);
    assert!(stream.next().await.unwrap().is_none());
}
#[tokio::test]
async fn rejects_stateful_stream_queries_and_all_query_mutations() {
    let c = connection(Arc::new(Log::default()));
    for text in [
        "MATCH (p) RETURN count(p)",
        "MATCH (p) RETURN DISTINCT p.name",
        "MATCH (p) RETURN p ORDER BY p.name",
        "MATCH (p) RETURN p LIMIT 1",
        "MATCH (p)-[:KNOWS]->(q) RETURN q",
        "MATCH (p),(q) RETURN p,q",
        "CREATE (:Person {name:'x'})",
        "MATCH (p) CALL { WITH p SET p.name = 'x' RETURN p AS x } RETURN x",
    ] {
        assert!(
            c.subscribe(query(text), BTreeMap::from([(0, 0)]))
                .await
                .is_err(),
            "accepted {text}"
        );
    }
    assert!(c.query(query("CREATE (:Person {name:'x'})")).await.is_err());
    assert!(c.query(Query::cypher("MATCH (p) RETURN p")).await.is_err());
}
#[test]
fn validates_all_partition_boundaries() {
    let b = Bounds {
        low: BTreeMap::from([(0, 4), (1, 8)]),
        high: BTreeMap::from([(0, 9), (1, 12)]),
    };
    assert!(validate_offsets(&BTreeMap::from([(0, 4), (1, 8)]), &b).is_ok());
    for o in [
        BTreeMap::from([(0, 4)]),
        BTreeMap::from([(0, 3), (1, 8)]),
        BTreeMap::from([(0, 9), (1, 13)]),
    ] {
        assert!(validate_offsets(&o, &b).is_err());
    }
}

#[cfg(feature = "kafka")]
#[tokio::test]
async fn kafka_wire_roundtrip_with_librdkafka_mock_brokers() {
    use futures::StreamExt;
    let cluster = rdkafka::mocking::MockCluster::new(1).unwrap();
    cluster.create_topic("materialized", 2, 1).unwrap();
    let mut config = rdkafka::ClientConfig::new();
    config.set("bootstrap.servers", cluster.bootstrap_servers());
    let log = Arc::new(KafkaLog::new(config, "materialized".into()).unwrap());
    let c = StreamConnection::new(
        schema(),
        "v1".into(),
        log.clone(),
        Arc::new(Auth),
        Limits::default(),
    )
    .unwrap();
    let receipt = c
        .insert(
            &Authorization::new("user", "alice"),
            change("a", 1, Some("wire")),
        )
        .await
        .unwrap();
    let bounds = log.bounds().await.unwrap();
    assert_eq!(bounds.high.values().sum::<i64>(), 1);
    let mut stream = log.follow(BTreeMap::from([(0, 0), (1, 0)])).unwrap();
    let record = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if let LogItem::Record(r) = stream.next().await.unwrap().unwrap() {
                break r;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!((record.partition, record.offset), receipt);
    assert_eq!(record.change, change("a", 1, Some("wire")));
    let batch = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        c.query(query("MATCH (p:Person) RETURN p.name")),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(strings(&batch), vec![vec!["wire"]]);
}

#[cfg(feature = "spicedb")]
#[tokio::test]
async fn spicedb_http_contract_and_fail_closed_decisions() {
    use std::io::{Read, Write};
    for (decision, expected) in [
        ("PERMISSIONSHIP_HAS_PERMISSION", Some(true)),
        ("PERMISSIONSHIP_NO_PERMISSION", Some(false)),
        ("PERMISSIONSHIP_CONDITIONAL_PERMISSION", Some(false)),
        ("unknown", None),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length: usize = headers
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(str::to_owned)
                        })
                        .unwrap()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        assert!(headers.starts_with("POST /v1/permissions/check "));
                        assert!(
                            headers
                                .to_ascii_lowercase()
                                .contains("authorization: bearer test-token")
                        );
                        let body: serde_json::Value =
                            serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                        assert_eq!(body["consistency"], json!({"fullyConsistent":true}));
                        assert_eq!(
                            body["resource"],
                            json!({"objectType":"person","objectId":"a"})
                        );
                        assert_eq!(body["permission"], "edit");
                        assert_eq!(body["subject"]["object"]["objectId"], "alice");
                        break;
                    }
                }
            }
            let body = json!({"permissionship":decision}).to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        let auth = SpiceDb::new(&endpoint, "test-token".into()).unwrap();
        let result = auth
            .check(&Authorization::new("user", "alice"), "person", "a", "edit")
            .await;
        assert_eq!(result.ok(), expected);
        server.join().unwrap();
    }
    assert!(SpiceDb::new("http://example.com", "token".into()).is_err());
}

#[tokio::test]
async fn authorization_filters_before_aggregates_and_denies_new_stream_events() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Revocable(AtomicBool);
    #[async_trait]
    impl Authorizer for Revocable {
        async fn check(
            &self,
            _: &Authorization,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<bool, String> {
            Ok(self.0.load(Ordering::SeqCst))
        }
    }
    let auth = Arc::new(Revocable(AtomicBool::new(true)));
    let log = Arc::new(Log {
        records: Mutex::new(vec![change("a", 1, Some("A")), change("a", 2, Some("B"))]),
        low: 0,
    });
    let c =
        StreamConnection::new(schema(), "v1".into(), log, auth.clone(), Limits::default()).unwrap();
    let mut stream = c
        .subscribe(query("MATCH (p) RETURN p.name"), BTreeMap::from([(0, 0)]))
        .await
        .unwrap();
    assert!(stream.next().await.unwrap().is_some());
    auth.0.store(false, Ordering::SeqCst);
    assert!(stream.next().await.unwrap().is_none());
    assert_eq!(
        strings(&c.query(query("MATCH (p) RETURN count(p)")).await.unwrap()),
        vec![vec!["0"]]
    );
}

#[tokio::test]
async fn cancelling_next_during_authorization_does_not_drop_the_record() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Blocking(AtomicBool);
    #[async_trait]
    impl Authorizer for Blocking {
        async fn check(
            &self,
            _: &Authorization,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<bool, String> {
            if self.0.load(Ordering::SeqCst) {
                futures::future::pending::<()>().await;
            }
            Ok(true)
        }
    }
    let auth = Arc::new(Blocking(AtomicBool::new(true)));
    let log = Arc::new(Log {
        records: Mutex::new(vec![change("a", 1, Some("A"))]),
        low: 0,
    });
    let c =
        StreamConnection::new(schema(), "v1".into(), log, auth.clone(), Limits::default()).unwrap();
    let mut stream = c
        .subscribe(query("MATCH (p) RETURN p.name"), BTreeMap::from([(0, 0)]))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), stream.next())
            .await
            .is_err()
    );
    auth.0.store(false, Ordering::SeqCst);
    let frame = stream.next().await.unwrap().unwrap();
    assert_eq!(frame.key, "a");
    assert_eq!(frame.offsets[&0], 1);
}

#[tokio::test]
async fn table_scoped_identity_and_edge_permissions_survive_hybrid_queries() {
    let mut s = schema();
    let mut company = s.tables["people"].clone();
    company.label = "Company".into();
    company.resource_type = "company".into();
    s.tables.insert("companies".into(), company);
    s.tables.insert(
        "works".into(),
        Table {
            label: "WORKS".into(),
            resource_type: "employment".into(),
            read_permission: "view".into(),
            write_permission: "edit".into(),
            properties: json!({"type":"object"}),
            edge: Some(Edge {
                source_table: "people".into(),
                source_column: "person".into(),
                target_table: "companies".into(),
                target_column: "company".into(),
            }),
        },
    );
    let mut company = change("same-key", 1, Some("Orchid"));
    company.table = "companies".into();
    let edge = Change {
        table: "works".into(),
        key: "edge".into(),
        version: 1,
        data: Some(
            json!({"person":"same-key","company":"same-key"})
                .as_object()
                .unwrap()
                .clone(),
        ),
    };
    let log = Arc::new(Log {
        records: Mutex::new(vec![change("same-key", 1, Some("Alice")), company, edge]),
        low: 0,
    });
    let c = StreamConnection::new(s, "v1".into(), log, Arc::new(Auth), Limits::default()).unwrap();
    let result = c
        .query(query(
            "MATCH (p:Person)-[:WORKS]->(c:Company) RETURN p.name,c.name",
        ))
        .await
        .unwrap();
    assert_eq!(strings(&result), vec![vec!["Alice", "Orchid"]]);
    let result = c
        .query(query("MATCH (p) RETURN id(p) AS id ORDER BY id"))
        .await
        .unwrap();
    assert_eq!(
        strings(&result),
        vec![
            vec!["[\"companies\",\"same-key\"]"],
            vec!["[\"people\",\"same-key\"]"]
        ]
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn duckdb_snapshot_adapter_reads_one_atomic_manifest() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE checkpoint(manifest VARCHAR)")
        .unwrap();
    let snapshot = Snapshot {
        source: "topic-v1".into(),
        offsets: BTreeMap::from([(0, 1)]),
        changes: vec![change("a", 1, Some("ice"))],
    };
    db.execute(
        "INSERT INTO checkpoint VALUES (?)",
        [serde_json::to_string(&snapshot).unwrap()],
    )
    .unwrap();
    let mut source = DuckDbSnapshot {
        connection: &db,
        snapshot_sql: "SELECT manifest FROM checkpoint".into(),
    };
    let log = Arc::new(Log {
        records: Mutex::new(vec![
            change("a", 1, Some("ice")),
            change("a", 2, Some("tail")),
        ]),
        low: 1,
    });
    let result = connection(log)
        .query_hybrid(query("MATCH (p) RETURN p.name"), &mut source)
        .await
        .unwrap();
    assert_eq!(strings(&result), vec![vec!["tail"]]);
    db.execute("INSERT INTO checkpoint SELECT * FROM checkpoint", [])
        .unwrap();
    assert!(source.load().await.unwrap_err().contains("exactly one"));
}

#[tokio::test]
async fn compacted_offsets_complete_without_waiting_for_missing_records() {
    struct Compacted;
    #[async_trait]
    impl ChangeLog for Compacted {
        async fn append(&self, _: &Change) -> Result<(i32, i64), String> {
            unreachable!()
        }
        async fn bounds(&self) -> Result<Bounds, String> {
            Ok(Bounds {
                low: BTreeMap::from([(0, 0), (1, 0)]),
                high: BTreeMap::from([(0, 10), (1, 4)]),
            })
        }
        fn follow(
            &self,
            _: Offsets,
        ) -> Result<BoxStream<'static, Result<LogItem, String>>, String> {
            Ok(Box::pin(futures::stream::iter(vec![
                Ok(LogItem::Record(LogRecord {
                    partition: 0,
                    offset: 7,
                    change: change("a", 2, Some("A")),
                })),
                Ok(LogItem::Progress {
                    partition: 1,
                    next_offset: 4,
                }),
                Ok(LogItem::Progress {
                    partition: 0,
                    next_offset: 10,
                }),
            ])))
        }
    }
    let c = StreamConnection::new(
        schema(),
        "v1".into(),
        Arc::new(Compacted),
        Arc::new(Auth),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        strings(&c.query(query("MATCH (p) RETURN p.name")).await.unwrap()),
        vec![vec!["A"]]
    );
}

#[tokio::test]
async fn json_numbers_preserve_large_integers_and_fail_without_panicking() {
    let log = Arc::new(Log::default());
    let c = connection(log.clone());
    let mut value = change("a", 1, Some("A"));
    value.data.as_mut().unwrap().insert(
        "large".into(),
        serde_json::from_str("18446744073709551617").unwrap(),
    );
    c.insert(&Authorization::new("user", "alice"), value)
        .await
        .unwrap();
    assert_eq!(
        strings(&c.query(query("MATCH (p) RETURN p.large")).await.unwrap()),
        vec![vec!["18446744073709551617"]]
    );
    let mut invalid = change("b", 1, Some("B"));
    invalid
        .data
        .as_mut()
        .unwrap()
        .insert("large".into(), serde_json::from_str("1e9999").unwrap());
    assert!(
        c.insert(&Authorization::new("user", "alice"), invalid)
            .await
            .is_err()
    );
    assert_eq!(log.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn prepared_event_kernels_match_full_execution() {
    let log = Arc::new(Log {
        records: Mutex::new(vec![change("a", 1, Some("Alice"))]),
        low: 0,
    });
    let c = connection(log);
    for text in [
        "MATCH (p:Person) RETURN p.name, p.age + 2 AS age",
        "MATCH (p) WITH p.name AS name, p.age AS age WHERE age > 20 RETURN toUpper(name) AS name, age",
        "MATCH (p) WHERE p.missing IS NULL RETURN coalesce(p.missing, p.name) AS name",
        "MATCH (p) RETURN id(p) AS key, labels(p) AS labels, p AS node",
        "MATCH (p) RETURN p.name AS __orchid_event_identity",
        "MATCH (p:Other) RETURN p.name",
        "MATCH (p) WHERE p.age < 0 RETURN p.name",
        "MATCH (p) RETURN CASE WHEN p.age = 30 THEN 'yes' ELSE 'no' END AS answer",
    ] {
        let expected = c.query(query(text)).await.unwrap();
        let mut sub = c
            .subscribe(query(text), BTreeMap::from([(0, 0)]))
            .await
            .unwrap();
        let actual = sub.next().await.unwrap().unwrap().rows.unwrap();
        assert_eq!(strings(&actual), strings(&expected), "{text}");
        assert_eq!(actual.schema(), expected.schema(), "{text}");
    }
}

#[tokio::test]
async fn batches_preserve_filtered_replacements_versions_deletes_and_checkpoints() {
    let log = Arc::new(Log {
        records: Mutex::new(vec![
            change("a", 1, Some("A")),
            change("b", 1, Some("B")),
            change("secret", 1, Some("hidden")),
            change("a", 2, Some("B")),
            change("a", 2, Some("B")),
            change("a", 3, None),
        ]),
        low: 0,
    });
    let c = connection(log);
    let mut sub = c
        .subscribe(
            query("MATCH (p:Person) WHERE p.name='A' RETURN p.name"),
            BTreeMap::from([(0, 0)]),
        )
        .await
        .unwrap();
    let first = sub
        .next_batch(256, std::time::Duration::from_millis(10))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.frames.len(), 2);
    assert_eq!(first.offsets[&0], 3);
    assert_eq!(
        strings(first.frames[0].rows.as_ref().unwrap()),
        vec![vec!["A"]]
    );
    assert_eq!(first.frames[1].rows.as_ref().unwrap().num_rows(), 0);
    // Switching between batch and single delivery must preserve prefetched records.
    let next = sub.next().await.unwrap().unwrap();
    assert_eq!(next.version, 2);
    assert_eq!(next.offsets[&0], 4);
    let second = sub
        .next_batch(256, std::time::Duration::from_millis(10))
        .await
        .unwrap()
        .unwrap();
    assert!(second.frames.is_empty());
    assert_eq!(second.offsets[&0], 5);
    let third = sub
        .next_batch(256, std::time::Duration::from_millis(10))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(third.frames.len(), 1);
    assert!(third.frames[0].deleted);
    assert_eq!(third.offsets[&0], 6);
    assert!(
        sub.next_batch(256, std::time::Duration::from_millis(10))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn insert_batch_checks_every_permission_before_writing_and_keeps_key_order() {
    let log = Arc::new(Log::default());
    let c = connection(log.clone());
    assert!(
        c.insert_batch(
            &Authorization::new("user", "alice"),
            &[change("a", 1, Some("A")), change("secret", 1, Some("no"))]
        )
        .await
        .is_err()
    );
    assert!(log.records.lock().unwrap().is_empty());
    let changes = [
        change("a", 1, Some("A")),
        change("b", 1, Some("B")),
        change("a", 2, None),
    ];
    let receipts = c
        .insert_batch(&Authorization::new("user", "alice"), &changes)
        .await
        .unwrap();
    assert!(receipts[0].1 < receipts[2].1);
    let records = log.records.lock().unwrap();
    for (change, receipt) in changes.iter().zip(receipts) {
        assert_eq!(change, &records[receipt.1 as usize]);
    }
}

#[tokio::test]
async fn batch_cancellation_retains_input_and_does_not_reuse_authorization() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Blocking(AtomicBool);
    #[async_trait]
    impl Authorizer for Blocking {
        async fn check(
            &self,
            _: &Authorization,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<bool, String> {
            if self.0.load(Ordering::SeqCst) {
                futures::future::pending::<()>().await;
            }
            Ok(true)
        }
    }
    let auth = Arc::new(Blocking(AtomicBool::new(true)));
    let log = Arc::new(Log {
        records: Mutex::new(vec![change("a", 1, Some("A")), change("b", 1, Some("B"))]),
        low: 0,
    });
    let c =
        StreamConnection::new(schema(), "v1".into(), log, auth.clone(), Limits::default()).unwrap();
    let mut sub = c
        .subscribe(query("MATCH (p) RETURN p.name"), BTreeMap::from([(0, 0)]))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            sub.next_batch(2, std::time::Duration::from_millis(1))
        )
        .await
        .is_err()
    );
    auth.0.store(false, Ordering::SeqCst);
    let batch = sub
        .next_batch(2, std::time::Duration::from_millis(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(batch.frames.len(), 2);
    assert_eq!(batch.offsets[&0], 2);
}

#[cfg(all(feature = "kafka", feature = "spicedb"))]
#[tokio::test]
#[ignore = "requires the isolated tools/diagnostics/streaming.compose.yml stack and setup_stream_bench.py"]
async fn live_kafka_spicedb_batches_and_revocation() {
    use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
    use rdkafka::client::DefaultClientContext;
    let topic = format!(
        "orchid-live-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut config = rdkafka::ClientConfig::new();
    config.set("bootstrap.servers", "127.0.0.1:19092");
    let admin: AdminClient<DefaultClientContext> = config.create().unwrap();
    let result = admin
        .create_topics(
            &[NewTopic::new(&topic, 6, TopicReplication::Fixed(1))],
            &AdminOptions::new(),
        )
        .await
        .unwrap();
    assert!(result.iter().all(Result::is_ok));
    let auth =
        Arc::new(SpiceDb::new("http://127.0.0.1:18443", "orchid-benchmark-only".into()).unwrap());
    let subject = Authorization::new("user", "alice");
    let a = format!("{topic}-a");
    let b = format!("{topic}-b");
    let client = reqwest::Client::new();
    let update = |keys: Vec<String>, op: &str| {
        let updates:Vec<_>=keys.iter().flat_map(|key|["viewer","editor"].map(|relation|json!({"operation":op,"relationship":{"resource":{"objectType":"event","objectId":key},"relation":relation,"subject":{"object":{"objectType":"user","objectId":"*"}}}}))).collect();
        client
            .post("http://127.0.0.1:18443/v1/relationships/write")
            .bearer_auth("orchid-benchmark-only")
            .json(&json!({"updates":updates}))
    };
    update(vec![a.clone(), b.clone()], "OPERATION_TOUCH")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let decisions = auth
        .check_many(
            &subject,
            &[
                PermissionCheck {
                    resource_type: "event",
                    resource_id: &a,
                    permission: "view",
                },
                PermissionCheck {
                    resource_type: "event",
                    resource_id: "missing-resource",
                    permission: "view",
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(decisions, [true, false]);
    let mut s = schema();
    s.tables.get_mut("people").unwrap().resource_type = "event".into();
    let log = Arc::new(KafkaLog::new(config, topic.clone()).unwrap());
    let c = StreamConnection::new(s, topic.clone(), log, auth, Limits::default()).unwrap();
    c.insert_batch(
        &subject,
        &[change(&a, 1, Some("A")), change(&b, 1, Some("B"))],
    )
    .await
    .unwrap();
    let mut sub = c
        .subscribe(
            query("MATCH (p) RETURN p.name"),
            (0..6).map(|p| (p, 0)).collect(),
        )
        .await
        .unwrap();
    let frames = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let mut frames = Vec::new();
        while frames.len() < 2 {
            frames.extend(
                sub.next_batch(2, std::time::Duration::from_millis(20))
                    .await
                    .unwrap()
                    .unwrap()
                    .frames,
            );
        }
        frames
    })
    .await
    .unwrap();
    assert_eq!(frames.len(), 2);
    for frame in frames {
        let rows = frame.rows.unwrap();
        let metadata = rows.schema();
        let typed: serde_json::Value =
            serde_json::from_str(&metadata.metadata()["orchiddb.cypher.typed_rows.v1"]).unwrap();
        assert_eq!(typed.as_array().unwrap().len(), 1);
        assert_eq!(typed[0].as_array().unwrap().len(), 1);
        assert!(
            !metadata.metadata()["orchiddb.cypher.typed_columns.v1"].contains("__orchid_event")
        );
    }
    c.insert(&subject, change(&b, 2, Some("B2"))).await.unwrap();
    update(vec![b.clone()], "OPERATION_DELETE")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert!(
        c.insert(&subject, change(&b, 3, Some("forbidden")))
            .await
            .is_err()
    );
    let denied = sub
        .next_batch(2, std::time::Duration::from_millis(100))
        .await
        .unwrap()
        .unwrap();
    assert!(denied.frames.is_empty());
    assert_eq!(
        strings(&c.query(query("MATCH (p) RETURN p.name")).await.unwrap()),
        vec![vec!["A"]]
    );
    update(vec![a, b], "OPERATION_DELETE")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let result = admin
        .delete_topics(&[&topic], &AdminOptions::new())
        .await
        .unwrap();
    assert!(result.iter().all(Result::is_ok));
}

#[cfg(feature = "spicedb")]
#[tokio::test]
async fn spicedb_bulk_rejects_incomplete_mismatched_and_unknown_decisions() {
    use std::io::{Read, Write};
    for case in [
        "allow",
        "deny",
        "conditional",
        "unknown",
        "missing",
        "wrong-key",
        "error",
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let request = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(str::to_owned)
                        })
                        .unwrap()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        assert!(headers.starts_with("POST /v1/permissions/checkbulk "));
                        let body: serde_json::Value =
                            serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                        assert_eq!(body["consistency"], json!({"fullyConsistent":true}));
                        break body["items"][0].clone();
                    }
                }
            };
            let decision = match case {
                "deny" => "PERMISSIONSHIP_NO_PERMISSION",
                "conditional" => "PERMISSIONSHIP_CONDITIONAL_PERMISSION",
                "unknown" => "UNKNOWN",
                _ => "PERMISSIONSHIP_HAS_PERMISSION",
            };
            let mut pair = json!({"request":request,"item":{"permissionship":decision}});
            if case == "wrong-key" {
                pair["request"]["resource"]["objectId"] = json!("someone-else");
            }
            if case == "error" {
                pair["error"] = json!({"code":13});
            }
            let body = if case == "missing" {
                json!({"pairs":[]})
            } else {
                json!({"pairs":[pair]})
            }
            .to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        let result = SpiceDb::new(&endpoint, "test-token".into())
            .unwrap()
            .check_many(
                &Authorization::new("user", "alice"),
                &[PermissionCheck {
                    resource_type: "person",
                    resource_id: "a",
                    permission: "view",
                }],
            )
            .await;
        match case {
            "allow" => assert_eq!(result.unwrap(), vec![true]),
            "deny" | "conditional" => assert_eq!(result.unwrap(), vec![false]),
            _ => assert!(result.is_err(), "{case}"),
        }
        server.join().unwrap();
    }
}
