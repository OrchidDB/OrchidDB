//! Local Kafka throughput probe. Environment: KAFKA_BOOTSTRAP, BENCH_TOPIC,
//! BENCH_N, BENCH_MODE, BENCH_INFLIGHT, BENCH_KEYS, BENCH_LINGER_MS.
use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use orchiddb::{compiler::Authorization, session::Query, streaming::*};
use rdkafka::{
    ClientConfig, Message, Offset, TopicPartitionList,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    consumer::{Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
struct LocalAuth;
#[async_trait]
impl Authorizer for LocalAuth {
    async fn check(&self, _: &Authorization, _: &str, _: &str, _: &str) -> Result<bool, String> {
        Ok(true)
    }
}
static KEY_COUNT: OnceLock<usize> = OnceLock::new();
fn change(i: usize) -> Change {
    let keys = *KEY_COUNT.get_or_init(|| {
        std::env::var("BENCH_KEYS")
            .ok()
            .map(|s| s.parse::<usize>().expect("BENCH_KEYS integer"))
            .unwrap_or(usize::MAX)
            .max(1)
    });
    Change {
        table: "events".into(),
        key: format!("{}", i % keys),
        version: (i / keys + 1) as u64,
        data: Some(
            json!({"name":format!("event-{i}"),"score":i as i64,"body":"x".repeat(256)})
                .as_object()
                .unwrap()
                .clone(),
        ),
    }
}
fn schema() -> StreamSchema {
    StreamSchema {
        tables: BTreeMap::from([(
            "events".into(),
            Table {
                label: "Event".into(),
                resource_type: "event".into(),
                read_permission: "view".into(),
                write_permission: "edit".into(),
                properties: json!({"type":"object","required":["name","score","body"],"properties":{"name":{"type":"string"},"score":{"type":"integer"},"body":{"type":"string"}}}),
                edge: None,
            },
        )]),
    }
}
fn report(
    mode: &str,
    n: usize,
    start: Instant,
    steady: Instant,
    warmup: usize,
    mut us: Vec<u128>,
    checksum: u64,
) {
    let elapsed = start.elapsed().as_secs_f64();
    let steady_s = steady.elapsed().as_secs_f64();
    us.sort_unstable();
    let at = |p: f64| us[((us.len() - 1) as f64 * p) as usize];
    println!(
        "{}",
        json!({"mode":mode,"n":n,"elapsed_s":elapsed,"records_s":n as f64/elapsed,"warmup":warmup,"steady_records_s":(n-warmup) as f64/steady_s,"call_p50_us":at(0.5),"call_p95_us":at(0.95),"call_p99_us":at(0.99),"checksum":checksum,"latency_unit":if mode.ends_with("live"){"produce-to-frame"}else if mode.contains("batch"){"batch"}else{"record"},"producer_warmup":if mode.contains("write"){256.min(n)}else{0}})
    );
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let n = std::env::var("BENCH_N")
        .unwrap_or("3000".into())
        .parse::<usize>()?;
    let mode = std::env::var("BENCH_MODE").unwrap_or("native-read".into());
    if ![
        "seed",
        "native-read",
        "native-project",
        "native-write",
        "native-wire-write",
        "native-live",
        "orchid-live",
        "orchid-read",
        "orchid-batch-read",
        "orchid-write",
        "orchid-batch-write",
    ]
    .contains(&mode.as_str())
    {
        return Err("unknown benchmark mode (authorization is client supplied)".into());
    }
    let topic = std::env::var("BENCH_TOPIC").unwrap_or("orchid-bench-v1".into());
    let brokers = std::env::var("KAFKA_BOOTSTRAP").unwrap_or("127.0.0.1:19092".into());
    let inflight = std::env::var("BENCH_INFLIGHT")
        .unwrap_or("4096".into())
        .parse::<usize>()?;
    if inflight == 0 {
        return Err("BENCH_INFLIGHT must be positive".into());
    }
    let linger = std::env::var("BENCH_LINGER_MS").unwrap_or("5".into());
    let mut config = ClientConfig::new();
    config
        .set("bootstrap.servers", brokers)
        .set("enable.idempotence", "true")
        .set("acks", "all")
        .set("linger.ms", &linger)
        .set("batch.size", "1048576")
        .set("queue.buffering.max.messages", "1000000")
        .set("message.timeout.ms", "30000");
    let admin: AdminClient<DefaultClientContext> = config.create()?;
    let created = admin
        .create_topics(
            &[NewTopic::new(&topic, 6, TopicReplication::Fixed(1))],
            &AdminOptions::new(),
        )
        .await?;
    for r in created {
        if let Err((_, e)) = r {
            if e != rdkafka::types::RDKafkaErrorCode::TopicAlreadyExists {
                return Err(format!("topic: {e}").into());
            }
        }
    }
    let log = Arc::new(KafkaLog::new(config.clone(), topic.clone())?);
    let auth: Arc<dyn Authorizer> = Arc::new(LocalAuth);
    let connection = StreamConnection::new(
        schema(),
        topic.clone(),
        log.clone(),
        auth,
        Limits {
            entities: n + 1024,
            ..Limits::default()
        },
    )?;
    let batch_size = std::env::var("BENCH_BATCH")
        .unwrap_or("256".into())
        .parse::<usize>()?;
    let subject = Authorization::new("user", "bench");
    if mode.ends_with("live") {
        // A live materializer probe: start the consumer before producing each
        // event, then measure produce-start through delivery of its single frame.
        // This serial probe measures latency, not saturation throughput.
        let producer: FutureProducer = config.create()?;
        let mut q = Query::cypher(
            "MATCH (e:Event) WHERE e.score >= 0 RETURN e.name AS name, e.score AS score",
        );
        q.authorization = Some(subject.clone());
        let mut sub = if mode.starts_with("orchid") {
            Some(
                connection
                    .subscribe(q, (0..6).map(|p| (p, 0)).collect())
                    .await?,
            )
        } else {
            None
        };
        let consumer: Option<StreamConsumer> = if sub.is_none() {
            let mut cfg = config.clone();
            cfg.set("group.id", "orchid-live-native")
                .set("enable.auto.commit", "false")
                .set("enable.auto.offset.store", "false")
                .set("auto.offset.reset", "error")
                .set("isolation.level", "read_committed");
            let c: StreamConsumer = cfg.create()?;
            let mut assignment = TopicPartitionList::new();
            for p in 0..6 {
                assignment.add_partition_offset(&topic, p, Offset::Offset(0))?;
            }
            c.assign(&assignment)?;
            Some(c)
        } else {
            None
        };
        let start = Instant::now();
        let mut steady = start;
        let warmup = (n / 10).min(1000);
        let mut samples = Vec::new();
        let mut checksum = 0;
        for i in 0..n {
            let t = Instant::now();
            let c = change(i);
            let key = c.kafka_key();
            let payload = serde_json::to_vec(&c)?;
            producer
                .send(
                    FutureRecord::to(&topic).key(&key).payload(&payload),
                    Duration::from_secs(10),
                )
                .await
                .map_err(|(e, _)| e)?;
            if let Some(sub) = &mut sub {
                let frame = tokio::time::timeout(Duration::from_secs(15), sub.next())
                    .await??
                    .ok_or("stream ended")?;
                assert_eq!(frame.key, c.key);
                let rows = frame.rows.ok_or("missing result")?;
                assert_eq!(rows.num_rows(), 1);
                checksum += rows
                    .column(1)
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .ok_or("score type")?
                    .value(0) as u64;
            } else {
                let message = tokio::time::timeout(
                    Duration::from_secs(15),
                    consumer.as_ref().unwrap().recv(),
                )
                .await??;
                let got: Change = serde_json::from_slice(message.payload().ok_or("payload")?)?;
                assert_eq!(got.key, c.key);
                checksum += got.data.unwrap()["score"].as_u64().ok_or("score")?;
            }
            if i >= warmup {
                samples.push(t.elapsed().as_micros());
            }
            if i + 1 == warmup {
                steady = Instant::now();
            }
        }
        assert_eq!(checksum, (n * (n - 1) / 2) as u64);
        report(&mode, n, start, steady, warmup, samples, checksum);
        return Ok(());
    }
    if mode.contains("write") || mode == "seed" {
        let producer: FutureProducer = config.create()?;
        // Warm the active producer, metadata, authorization connection and broker.
        // Writer topics are separate from the seeded read topic.
        if mode != "seed" {
            let warm: Vec<_> = (0..256.min(n)).map(change).collect();
            if mode.starts_with("orchid") {
                if mode.contains("batch") {
                    connection.insert_batch(&subject, &warm).await?;
                } else {
                    futures::stream::iter(warm)
                        .map(|c| connection.insert(&subject, c))
                        .buffer_unordered(inflight)
                        .try_collect::<Vec<_>>()
                        .await?;
                }
            } else {
                for c in warm {
                    let payload = serde_json::to_vec(&c)?;
                    let key = c.kafka_key();
                    producer
                        .send(
                            FutureRecord::to(&topic).key(&key).payload(&payload),
                            Duration::from_secs(10),
                        )
                        .await
                        .map_err(|(e, _)| e)?;
                }
            }
        }
        let preencoded = if mode == "native-wire-write" {
            Some(
                (0..n)
                    .map(|i| {
                        let c = change(i);
                        (c.kafka_key(), serde_json::to_vec(&c).unwrap())
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        let start = Instant::now();
        if mode.contains("batch") {
            let mut samples = Vec::new();
            for begin in (0..n).step_by(batch_size) {
                let t = Instant::now();
                let changes: Vec<_> = (begin..n.min(begin + batch_size)).map(change).collect();
                let receipts = connection.insert_batch(&subject, &changes).await?;
                assert_eq!(receipts.len(), changes.len());
                samples.push(t.elapsed().as_micros());
            }
            report(&mode, n, start, start, 0, samples, n as u64);
            return Ok(());
        }
        let results: Vec<_> = futures::stream::iter(0..n)
            .map(|i| {
                let producer = &producer;
                let connection = &connection;
                let subject = &subject;
                let topic = &topic;
                let mode = &mode;
                let preencoded = &preencoded;
                async move {
                    let t = Instant::now();
                    if let Some(encoded) = preencoded {
                        producer
                            .send(
                                FutureRecord::to(topic)
                                    .key(&encoded[i].0)
                                    .payload(&encoded[i].1),
                                Duration::from_secs(10),
                            )
                            .await
                            .map_err(|(e, _)| e.to_string())?;
                        return Ok::<_, String>(t.elapsed().as_micros());
                    }
                    let c = change(i);
                    if mode.starts_with("orchid") {
                        connection.insert(subject, c).await?;
                    } else {
                        let key = c.kafka_key();
                        let payload = serde_json::to_vec(&c).map_err(|e| e.to_string())?;
                        producer
                            .send(
                                FutureRecord::to(topic).key(&key).payload(&payload),
                                Duration::from_secs(10),
                            )
                            .await
                            .map_err(|(e, _)| e.to_string())?;
                    }
                    Ok::<_, String>(t.elapsed().as_micros())
                }
            })
            .buffer_unordered(inflight)
            .try_collect()
            .await?;
        report(&mode, n, start, start, 0, results, n as u64);
        return Ok(());
    }
    let warmup = (n / 10).min(1000);
    let mut samples = Vec::with_capacity(n);
    let mut checksum = 0u64;
    if mode.starts_with("orchid") {
        let mut q = Query::cypher(
            "MATCH (e:Event) WHERE e.score >= 0 RETURN e.name AS name, e.score AS score",
        );
        q.authorization = Some(subject);
        let mut sub = connection
            .subscribe(q, (0..6).map(|p| (p, 0)).collect())
            .await?;
        let start = Instant::now();
        let mut steady = start;
        let mut seen = 0;
        let mut actual_warmup = warmup;
        while seen < n {
            let t = Instant::now();
            let frames = if mode.contains("batch") {
                sub.next_batch(batch_size.min(n - seen), Duration::from_millis(5))
                    .await?
                    .ok_or("unexpected end")?
                    .frames
            } else {
                vec![sub.next().await?.ok_or("unexpected end")?]
            };
            if frames.is_empty() {
                continue;
            }
            samples.push(t.elapsed().as_micros());
            for frame in frames {
                let rows = frame.rows.ok_or("missing result")?;
                if rows.num_rows() != 1 {
                    return Err("incorrect row count".into());
                }
                checksum += rows
                    .column(1)
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .ok_or("score type")?
                    .value(0) as u64;
                seen += 1;
            }
            if seen >= warmup && actual_warmup == warmup {
                steady = Instant::now();
                actual_warmup = seen + 1;
            }
        }
        let warmup = actual_warmup.saturating_sub(1);
        assert_eq!(checksum, (n * (n - 1) / 2) as u64);
        report(&mode, n, start, steady, warmup, samples, checksum);
    } else {
        config
            .set("group.id", "orchid-native-benchmark")
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", "error")
            .set("isolation.level", "read_committed");
        let consumer: StreamConsumer = config.create()?;
        let mut assignment = TopicPartitionList::new();
        for p in 0..6 {
            assignment.add_partition_offset(&topic, p, Offset::Offset(0))?;
        }
        consumer.assign(&assignment)?;
        let start = Instant::now();
        let mut steady = start;
        for i in 0..n {
            let t = Instant::now();
            let message = consumer.recv().await?;
            if mode == "native-project" {
                let c: Change = serde_json::from_slice(message.payload().ok_or("payload")?)?;
                let data = c.data.ok_or("data")?;
                checksum += data["score"].as_u64().ok_or("score")?;
                let _name = data["name"].as_str().ok_or("name")?;
            } else {
                checksum += message.payload().ok_or("payload")?.len() as u64;
            }
            samples.push(t.elapsed().as_micros());
            if i + 1 == warmup {
                steady = Instant::now();
            }
        }
        if mode == "native-project" {
            assert_eq!(checksum, (n * (n - 1) / 2) as u64);
        }
        report(&mode, n, start, steady, warmup, samples, checksum);
    }
    Ok(())
}
