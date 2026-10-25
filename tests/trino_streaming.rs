#![cfg(feature = "trino")]
use arrow::{
    array::{ArrayRef, Float64Array, Int64Array},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use futures::{StreamExt, stream};
use orchiddb::execution::{
    exchange::{ExchangeRegistry, ExchangeServer, ExchangeServerOptions},
    stream::{BatchSource, BatchStream, InputRelation},
    trino::{TrinoOptions, TrinoSession},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

const TOKEN: &str = "orchid-test-exchange-token-32-bytes-minimum";
fn fixture() -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("source_id", DataType::Int64, false),
            Field::new("document_id", DataType::Int64, false),
            Field::new("score", DataType::Float64, false),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 1, 2, 2])),
            Arc::new(Int64Array::from(vec![10, 20, 10, 20])),
            Arc::new(Float64Array::from(vec![0.95, 0.6, 0.9, 0.6])),
        ],
    )
    .unwrap()
}
#[tokio::test]
async fn exchange_metadata_is_lazy_and_one_shot_reads_are_explicit() {
    let registry = ExchangeRegistry::default();
    let batch = fixture();
    let schema = batch.schema();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let source = BatchSource::once(BatchStream::new(
        schema,
        stream::once(async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(batch)
        }),
    ))
    .with_estimated_rows(4);
    let lease = registry.register(vec![source], Some(4)).unwrap();
    let server =
        ExchangeServer::bind(registry.clone(), ExchangeServerOptions::local(TOKEN.into())).unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://{}/v1/exchanges/{}", server.address(), lease.id());
    assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
    let metadata: serde_json::Value = client
        .get(&url)
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(metadata["estimated_rows"], 4);
    assert_eq!(metadata["replayable"], false);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    let bytes = client
        .get(format!("{url}/partitions/0"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let reader =
        arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None).unwrap();
    assert_eq!(reader.map(|b| b.unwrap().num_rows()).sum::<usize>(), 4);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        client
            .get(format!("{url}/partitions/0"))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    drop(lease);
    assert_eq!(
        client
            .get(url)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}
#[tokio::test]
async fn partial_remote_failure_is_not_successful_arrow_eof() {
    let registry = ExchangeRegistry::default();
    let batch = fixture();
    let source = BatchSource::once(BatchStream::new(
        batch.schema(),
        stream::iter([Ok(batch), Err("injected upstream failure".into())]),
    ));
    let lease = registry.register(vec![source], None).unwrap();
    let server =
        ExchangeServer::bind(registry, ExchangeServerOptions::local(TOKEN.into())).unwrap();
    let response = reqwest::Client::new()
        .get(format!(
            "http://{}/v1/exchanges/{}/partitions/0",
            server.address(),
            lease.id()
        ))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert!(
        response.bytes().await.is_err(),
        "truncated HTTP chunks must fail instead of returning partial rows"
    );
}
#[tokio::test]
async fn replay_factories_reopen_and_lease_drop_cancels_pending_reads() {
    let registry = ExchangeRegistry::default();
    let batch = fixture();
    let count = Arc::new(AtomicUsize::new(0));
    let opens = count.clone();
    let schema = batch.schema();
    let source = BatchSource::replayable(schema.clone(), move || {
        opens.fetch_add(1, Ordering::SeqCst);
        Ok(BatchStream::new(
            schema.clone(),
            stream::iter([Ok(batch.clone())]),
        ))
    });
    let lease = registry.register(vec![source], None).unwrap();
    let server =
        ExchangeServer::bind(registry.clone(), ExchangeServerOptions::local(TOKEN.into())).unwrap();
    let client = reqwest::Client::new();
    let url = format!(
        "http://{}/v1/exchanges/{}/partitions/0",
        server.address(),
        lease.id()
    );
    for _ in 0..2 {
        client
            .get(&url)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
    }
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let source = BatchSource::once(BatchStream::new(fixture().schema(), stream::pending()));
    let pending = registry.register(vec![source], None).unwrap();
    let response = client
        .get(format!(
            "http://{}/v1/exchanges/{}/partitions/0",
            server.address(),
            pending.id()
        ))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    drop(pending);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), response.bytes())
            .await
            .unwrap()
            .is_err()
    );
}

/// Run with tools/trino/streaming_fixture.py up and ORCHIDDB_TEST_TRINO_STREAMING=1.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_trino_streams_query_scoped_relations_and_nested_types() {
    if std::env::var_os("ORCHIDDB_TEST_TRINO_STREAMING").is_none() {
        return;
    }
    let registry = ExchangeRegistry::default();
    let mut options = ExchangeServerOptions::local(TOKEN.into());
    options.bind = "0.0.0.0:18883".parse().unwrap();
    let _server = ExchangeServer::bind(registry.clone(), options).unwrap();
    let mut options = TrinoOptions::new("http://127.0.0.1:18083", "orchid-test");
    options.exchange_catalog = "orchid".into();
    options.result_batch_rows = 1;
    let session = TrinoSession::new(options, registry).unwrap();
    let batch = fixture();
    let source = BatchSource::once(BatchStream::new(batch.schema(), stream::iter([Ok(batch)])))
        .with_estimated_rows(4);
    let output=session.execute("SELECT m.source_id, d.title, m.score FROM incoming m JOIN (VALUES (CAST(10 AS BIGINT), 'Arrow')) d(id,title) ON d.id=m.document_id WHERE m.score>=0.8 ORDER BY m.source_id",vec![InputRelation{name:"incoming".into(),source}]).await.unwrap();
    let batches = output.collect_batches().await.unwrap();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
    let batch = arrow::compute::concat_batches(&batches[0].schema(), &batches).unwrap();
    assert_eq!(
        batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .values()
            .as_ref(),
        &[1, 2]
    );

    // All CTEs, including injected Arrow exchanges, require aliases when a
    // statement contains a recursive WITH in Trino.
    let recursive_batch = fixture();
    let recursive_source = BatchSource::once(BatchStream::new(recursive_batch.schema(), stream::iter([Ok(recursive_batch)])));
    let recursive = session.execute(
        "WITH RECURSIVE numbers(n) AS (SELECT CAST(1 AS BIGINT) UNION ALL SELECT n + 1 FROM numbers WHERE n < 2) SELECT count(*) FROM incoming CROSS JOIN numbers",
        vec![InputRelation { name: "incoming".into(), source: recursive_source }],
    ).await.unwrap().collect_batches().await.unwrap();
    assert_eq!(recursive[0].column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0), 8);

    let empty = RecordBatch::try_new_with_options(
        Arc::new(Schema::empty()),
        vec![],
        &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(3)),
    )
    .unwrap();
    let empty_source =
        BatchSource::once(BatchStream::new(empty.schema(), stream::iter([Ok(empty)])));
    let empty_result = session
        .execute(
            "SELECT count(*) FROM incoming",
            vec![InputRelation {
                name: "incoming".into(),
                source: empty_source,
            }],
        )
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(
        empty_result[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0),
        3
    );

    use arrow::array::{
        BinaryArray, Date32Array, Decimal128Array, ListArray, StringArray, StructArray,
        Time64NanosecondArray, TimestampMicrosecondArray,
    };
    let nested = ListArray::from_iter_primitive::<arrow::datatypes::Int64Type, _, _>([
        Some(vec![Some(1), None]),
        Some(vec![]),
    ]);
    let row = StructArray::from(vec![(
        Arc::new(Field::new("label", DataType::Utf8, true)),
        Arc::new(StringArray::from(vec![Some("x"), None])) as ArrayRef,
    )]);
    let columns: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(vec![1, 2])),
        Arc::new(nested),
        Arc::new(BinaryArray::from(vec![Some(&[0u8, 1][..]), Some(&[][..])])),
        Arc::new(
            Decimal128Array::from(vec![12345, -100])
                .with_precision_and_scale(12, 2)
                .unwrap(),
        ),
        Arc::new(TimestampMicrosecondArray::from(vec![0, 1001]).with_timezone("UTC")),
        Arc::new(Date32Array::from(vec![0, 1])),
        Arc::new(Time64NanosecondArray::from(vec![
            1_000_000_001,
            2_000_000_002,
        ])),
        Arc::new(row),
    ];
    let fields = columns
        .iter()
        .enumerate()
        .map(|(i, c)| Field::new(format!("c{i}"), c.data_type().clone(), true))
        .collect::<Vec<_>>();
    let batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).unwrap();
    let original = batch.clone();
    let source = BatchSource::replayable(batch.schema(), move || {
        Ok(BatchStream::new(
            batch.schema(),
            stream::iter([Ok(batch.clone())]),
        ))
    });
    let output = session
        .execute(
            "SELECT * FROM incoming WHERE CAST(c1 AS ARRAY(BIGINT)) IS NOT NULL ORDER BY c0",
            vec![InputRelation {
                name: "incoming".into(),
                source: source.clone(),
            }],
        )
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    let result = arrow::compute::concat_batches(&output[0].schema(), &output).unwrap();
    assert_eq!(result.columns(), original.columns());
    let replay = session
        .execute(
            "SELECT count(*) FROM incoming a CROSS JOIN incoming b",
            vec![InputRelation {
                name: "incoming".into(),
                source,
            }],
        )
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(
        replay[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0),
        4
    );
    #[cfg(feature = "duckdb")]
    {
        use orchiddb::{engine::GraphEngine, ir::rel::mapping::{GraphMapping, NodeMapping, EdgeMapping}};
        let storage = duckdb::Connection::open_in_memory().unwrap();
        storage.execute_batch("CREATE TABLE people(id VARCHAR, name VARCHAR); INSERT INTO people VALUES ('a','Ada'),('b','Bob'); CREATE TABLE links(src VARCHAR, dst VARCHAR); INSERT INTO links VALUES ('a','b');").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.map_node(NodeMapping::table("Person", "people", "id").property("name", "name"));
        mapping.map_edge(EdgeMapping::table("KNOWS", "links", "src", "dst", "Person", "Person"));
        let mut graph = GraphEngine::mapped(storage, Arc::new(mapping)).unwrap();
        graph.set_sql_region_session(Box::new(session.clone()));
        let result = graph.cypher("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name AS source, b.name AS target").await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 1);
        assert_eq!(result.returned.batch.column(0).as_any().downcast_ref::<arrow::array::StringArray>().unwrap().value(0), "Ada");
        assert_eq!(result.returned.batch.column(1).as_any().downcast_ref::<arrow::array::StringArray>().unwrap().value(0), "Bob");
        assert_eq!(result.stats.duckdb_regions, 0);
        assert!(result.stats.other_sql_regions > 0);
    }

}

#[tokio::test]
async fn trino_result_pages_are_lazy_and_drop_cancels_and_revokes_inputs() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::{Mutex, atomic::AtomicBool},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let requests = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let observed = requests.clone();
    let worker = std::thread::spawn(move || {
        while !stop.load(Ordering::Acquire) {
            let (mut socket, _) = match listener.accept() {
                Ok(socket) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut input = BufReader::new(socket.try_clone().unwrap());
            let mut first = String::new();
            input.read_line(&mut first).unwrap();
            let mut length = 0;
            loop {
                let mut header = String::new();
                input.read_line(&mut header).unwrap();
                if header == "\r\n" {
                    break;
                }
                if let Some((key, value)) = header.split_once(':') {
                    if key.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            input.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            observed.lock().unwrap().push((first.clone(), body));
            let page = if first.starts_with("DELETE") {
                serde_json::json!({})
            } else if first.starts_with("POST") {
                serde_json::json!({"columns":[{"name":"n","typeSignature":{"rawType":"bigint","arguments":[]}}],"data":[[1]],"nextUri":format!("http://{address}/next")})
            } else {
                serde_json::json!({"data":[[2]]})
            };
            let bytes = serde_json::to_vec(&page).unwrap();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
            socket.write_all(&bytes).unwrap();
        }
    });
    struct Stop(Arc<AtomicBool>, Option<std::thread::JoinHandle<()>>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
            if let Some(worker) = self.1.take() { let _ = worker.join(); }
        }
    }
    let _stop = Stop(stopped, Some(worker));
    let registry = ExchangeRegistry::default();
    let session = TrinoSession::new(
        TrinoOptions::new(format!("http://{address}"), "test"),
        registry.clone(),
    )
    .unwrap();
    let batch = fixture();
    let source = BatchSource::once(BatchStream::new(batch.schema(), stream::iter([Ok(batch)])));
    let mut output = session
        .execute(
            "SELECT count(*) n FROM incoming",
            vec![InputRelation {
                name: "incoming".into(),
                source,
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        output
            .next()
            .await
            .unwrap()
            .unwrap()
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0),
        1
    );
    let id = {
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "next page fetched eagerly");
        requests[0]
            .1
            .split('"')
            .find(|part| part.len() == 33 && part.starts_with('e'))
            .unwrap()
            .to_string()
    };
    assert!(registry.metadata(&id).is_ok());
    drop(output);
    assert!(registry.metadata(&id).is_err());
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if requests
                .lock()
                .unwrap()
                .iter()
                .any(|(method, _)| method.starts_with("DELETE"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !requests
            .lock()
            .unwrap()
            .iter()
            .any(|(method, _)| method.starts_with("GET"))
    );
}
