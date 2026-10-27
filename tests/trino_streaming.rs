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

/// Optional live source test; no Orchid exchange connector is required.
#[tokio::test]
async fn real_trino_source_returns_nested_types() {
    if std::env::var_os("ORCHIDDB_TEST_TRINO_STREAMING").is_none() {
        return;
    }
    let session =
        TrinoSession::source(TrinoOptions::new("http://127.0.0.1:18083", "orchid-test")).unwrap();
    let batches=session.execute("SELECT CAST(42 AS BIGINT) AS id, ARRAY[CAST(1 AS BIGINT), CAST(NULL AS BIGINT)] AS items, DATE '2026-10-26' AS day",vec![]).await.unwrap().collect_batches().await.unwrap();
    assert_eq!(batches[0].num_rows(), 1);
    assert_eq!(
        batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0),
        42
    );
    assert!(matches!(
        batches[0].column(1).data_type(),
        DataType::List(_)
    ));
    assert_eq!(batches[0].column(2).data_type(), &DataType::Date32);
}

#[tokio::test]
async fn trino_source_rejects_intermediate_inputs_before_network_access() {
    let session = TrinoSession::source(TrinoOptions::new("http://127.0.0.1:1", "test")).unwrap();
    let batch = fixture();
    let source = BatchSource::once(BatchStream::new(batch.schema(), stream::iter([Ok(batch)])));
    let error = session
        .execute(
            "SELECT * FROM incoming",
            vec![InputRelation {
                name: "incoming".into(),
                source,
            }],
        )
        .await
        .unwrap_err();
    assert!(error.contains("cannot consume intermediate relations"));
}

#[tokio::test]
async fn trino_source_result_pages_are_lazy_and_drop_cancels() {
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
            if let Some(worker) = self.1.take() {
                let _ = worker.join();
            }
        }
    }
    let _stop = Stop(stopped, Some(worker));
    let session =
        TrinoSession::source(TrinoOptions::new(format!("http://{address}"), "test")).unwrap();
    let mut output = session
        .execute("SELECT CAST(1 AS BIGINT) AS n", vec![])
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
    {
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "next page fetched eagerly");
        assert_eq!(requests[0].1, "SELECT CAST(1 AS BIGINT) AS n");
    }
    drop(output);
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
