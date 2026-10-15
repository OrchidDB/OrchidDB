//! Internal language-binding protocol. Permission decisions stay in the host.
//! Handles are process-local capabilities, never HTTP authentication tokens.
use super::*;
use base64::Engine;
use serde_json::json;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};

type Reply = oneshot::Sender<Vec<bool>>;
enum Event {
    Auth(Value, Reply, std::time::Instant),
    Result(Result<Value, String>),
}
struct HostAuth {
    events: mpsc::Sender<Event>,
    timeout: Duration,
}
#[async_trait]
impl Authorizer for HostAuth {
    async fn check(
        &self,
        subject: &Authorization,
        rt: &str,
        id: &str,
        permission: &str,
    ) -> Result<bool, String> {
        Ok(self
            .check_many(
                subject,
                &[PermissionCheck {
                    resource_type: rt,
                    resource_id: id,
                    permission,
                }],
            )
            .await?[0])
    }
    async fn check_many(
        &self,
        subject: &Authorization,
        checks: &[PermissionCheck<'_>],
    ) -> Result<Vec<bool>, String> {
        if checks.is_empty() {
            return Ok(vec![]);
        }
        let (tx, rx) = oneshot::channel();
        let checks: Vec<_> = checks.iter().map(|c| json!({"resource_type":c.resource_type,"resource_id":c.resource_id,"permission":c.permission})).collect();
        let count = checks.len();
        let deadline = std::time::Instant::now() + self.timeout;
        self.events
            .send(Event::Auth(
                json!({"subject":subject,"checks":checks}),
                tx,
                deadline,
            ))
            .await
            .map_err(|_| "authorization client closed")?;
        let decisions = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), rx)
            .await
            .map_err(|_| "authorization callback timed out")?
            .map_err(|_| "authorization callback cancelled")?;
        // Tokio polls a ready receiver before its timer. Check wall-clock expiry
        // too, so executor starvation cannot turn a late grant into a write.
        if std::time::Instant::now() >= deadline {
            return Err("authorization callback timed out".into());
        }
        if decisions.len() != count {
            return Err("authorization must return one boolean per check".into());
        }
        Ok(decisions)
    }
}
struct Handle {
    commands: mpsc::Sender<Value>,
    events: AsyncMutex<mpsc::Receiver<Event>>,
    pending: Mutex<Option<(u64, Reply, std::time::Instant)>>,
    busy: AtomicBool,
    task: futures::future::AbortHandle,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn registry() -> &'static Mutex<BTreeMap<String, Arc<Handle>>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<String, Arc<Handle>>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
fn next_id() -> u64 {
    static ID: AtomicU64 = AtomicU64::new(1);
    ID.fetch_add(1, Ordering::Relaxed)
}
fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}
fn integer(v: &Value, key: &str, default: u64) -> Result<u64, String> {
    match v.get(key) {
        None => Ok(default),
        Some(n) => n
            .as_u64()
            .ok_or_else(|| format!("{key} must be a nonnegative integer")),
    }
}
fn query(v: &Value) -> Result<Query, String> {
    let mut q = Query::cypher(v["text"].as_str().ok_or("query text required")?);
    if let Some(p) = v.get("parameters") {
        q.parameters = parse(p)?;
    }
    q.authorization = Some(parse(&v["subject"])?);
    Ok(q)
}
fn batch(batch: RecordBatch) -> Result<Value, String> {
    let mut bytes = vec![];
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, &batch.schema())
            .map_err(|e| e.to_string())?;
        writer.write(&batch).map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(json!({"ipc":base64::engine::general_purpose::STANDARD.encode(bytes)}))
}
fn frames(batch_value: Option<FrameBatch>) -> Result<Value, String> {
    let Some(b) = batch_value else {
        return Ok(Value::Null);
    };
    // Share Arrow schemas and buffers across frames instead of encoding a full
    // IPC stream per entity. Separate groups preserve heterogeneous projections.
    let mut groups: Vec<(arrow::datatypes::SchemaRef, Vec<RecordBatch>, usize)> = Vec::new();
    let mut frames = Vec::with_capacity(b.frames.len());
    for f in b.frames {
        let result = if let Some(rows) = f.rows {
            let mut metadata = rows.schema().metadata().clone();
            // Typed row metadata contains values, so it differs for every
            // frame even when the Arrow fields match. Preserve its presence
            // while excluding its payload from schema grouping.
            if let Some(value) = metadata.get_mut("orchiddb.cypher.typed_rows.v1") {
                value.clear();
            }
            let normalized = Arc::new(arrow::datatypes::Schema::new_with_metadata(
                rows.schema().fields().clone(),
                metadata,
            ));
            let index = groups
                .iter()
                .position(|(schema, _, _)| schema == &normalized)
                .unwrap_or_else(|| {
                    groups.push((normalized, Vec::new(), 0));
                    groups.len() - 1
                });
            let group = &mut groups[index];
            let reference = json!({"batch":index,"offset":group.2,"length":rows.num_rows()});
            group.2 += rows.num_rows();
            group.1.push(rows);
            reference
        } else {
            Value::Null
        };
        frames.push(json!({"table":f.table,"key":f.key,"version":f.version.to_string(),"deleted":f.deleted,"result":result}));
    }
    let batches = groups
        .into_iter()
        .map(|(schema, mut chunks, _)| {
            let combined = if chunks.len() == 1 {
                chunks.pop().unwrap()
            } else {
                let mut metadata = schema.metadata().clone();
                let key = "orchiddb.cypher.typed_rows.v1";
                if metadata.contains_key(key) {
                    let mut typed_rows = Vec::<Value>::new();
                    for chunk in &chunks {
                        let rows: Vec<Value> =
                            serde_json::from_str(&chunk.schema().metadata()[key])
                                .map_err(|e| e.to_string())?;
                        if rows.len() != chunk.num_rows() {
                            return Err("typed frame metadata row count mismatch".into());
                        }
                        typed_rows.extend(rows);
                    }
                    metadata.insert(
                        key.into(),
                        serde_json::to_string(&typed_rows).map_err(|e| e.to_string())?,
                    );
                }
                let schema = Arc::new(arrow::datatypes::Schema::new_with_metadata(
                    schema.fields().clone(),
                    metadata,
                ));
                arrow::compute::concat_batches(&schema, &chunks).map_err(|e| e.to_string())?
            };
            batch(combined)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({"frames":frames,"batches":batches,"offsets":b.offsets}))
}
// The worker owns its connection and subscription together: no leaked connections
// or unsafe borrowed lifetimes. Only one operation per handle may be outstanding.
async fn worker(
    connection: StreamConnection,
    mut commands: mpsc::Receiver<Value>,
    events: mpsc::Sender<Event>,
) {
    while let Some(v) = commands.recv().await {
        if v["action"] == "subscribe" {
            let prepared = match (query(&v), parse(&v["offsets"])) {
                (Ok(q), Ok(o)) => connection.subscribe(q, o).await,
                (Err(e), _) | (_, Err(e)) => Err(e),
            };
            match prepared {
                Err(e) => {
                    let _ = events.send(Event::Result(Err(e))).await;
                }
                Ok(mut subscription) => {
                    if events.send(Event::Result(Ok(json!({})))).await.is_err() {
                        return;
                    }
                    while let Some(v) = commands.recv().await {
                        if v["action"] == "unsubscribe" {
                            let _ = events.send(Event::Result(Ok(json!({})))).await;
                            break;
                        }
                        let result = if v["action"] == "next" {
                            let max = integer(&v, "max_records", 1);
                            let wait = integer(&v, "wait_ms", 100);
                            let (max, wait) = match (max, wait) {
                                (Ok(max), Ok(wait)) => (max, wait),
                                (Err(e), _) | (_, Err(e)) => {
                                    if events.send(Event::Result(Err(e))).await.is_err() {
                                        return;
                                    }
                                    continue;
                                }
                            };
                            if max == 0 || max > 4096 || wait == 0 || wait > 60_000 {
                                Err("invalid stream batch bounds".into())
                            } else {
                                subscription
                                    .next_batch(max as usize, Duration::from_millis(wait))
                                    .await
                                    .and_then(frames)
                            }
                        } else {
                            Err(
                                "unsubscribe before querying or inserting on this connection"
                                    .into(),
                            )
                        };
                        if events.send(Event::Result(result)).await.is_err() {
                            return;
                        }
                    }
                }
            }
            continue;
        }
        let result = async {
            match v["action"].as_str() {
                Some("insert") => {
                    let subject = parse(&v["subject"])?;
                    let change = parse(&v["change"])?;
                    let (partition, offset) = connection.insert(&subject, change).await?;
                    Ok(json!({"partition":partition,"offset":offset}))
                }
                Some("insert_batch") => {
                    let receipts = connection
                        .insert_batch(
                            &parse(&v["subject"])?,
                            &parse::<Vec<Change>>(&v["changes"])?,
                        )
                        .await?;
                    Ok(json!({"receipts":receipts}))
                }
                Some("query") => batch(connection.query(query(&v)?).await?),
                // Snapshot loading is caller-owned (for example an Iceberg reader).
                // Use the same validated merge and checkpoint rules as Rust.
                Some("hybrid") => {
                    let snapshot: Snapshot = parse(&v["snapshot"])?;
                    // query_hybrid takes a non-Send snapshot source. Load the
                    // supplied immutable snapshot directly before the shared query.
                    batch(
                        tokio::time::timeout(
                            connection.limits.timeout,
                            connection.query_snapshot(query(&v)?, Some(snapshot)),
                        )
                        .await
                        .map_err(|_| "query timed out")??,
                    )
                }
                _ => Err("unknown streaming action".into()),
            }
        }
        .await;
        if events.send(Event::Result(result)).await.is_err() {
            return;
        }
    }
}

pub async fn command(v: Value) -> Result<Value, String> {
    if v["op"] == "stream_open" {
        let config: BTreeMap<String, String> = parse(&v["kafka"])?;
        if !config
            .get("bootstrap.servers")
            .is_some_and(|s| !s.is_empty())
        {
            return Err("Kafka bootstrap.servers required".into());
        }
        let mut kafka = rdkafka::ClientConfig::new();
        for (k, val) in config {
            kafka.set(k, val);
        }
        let log = Arc::new(KafkaLog::new(
            kafka,
            v["topic"].as_str().ok_or("topic required")?.into(),
        )?);
        return open(v, log);
    }
    let id = v["id"].as_str().ok_or("stream handle required")?;
    if v["op"] == "stream_close" {
        if let Some(h) = registry().lock().unwrap().remove(id) {
            h.task.abort();
        }
        return Ok(json!({}));
    }
    let h = registry()
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or("stream connection closed")?;
    match v["op"].as_str() {
        Some("stream_start") => {
            if h.busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err("stream connection busy".into());
            }
            if h.commands.try_send(v).is_err() {
                h.busy.store(false, Ordering::Release);
                return Err("stream worker closed".into());
            }
            Ok(json!({}))
        }
        Some("stream_poll") => {
            if !h.busy.load(Ordering::Acquire) {
                return Err("no stream operation pending".into());
            }
            if h.pending.lock().unwrap().is_some() {
                return Err("authorization reply required".into());
            }
            let mut events = h
                .events
                .try_lock()
                .map_err(|_| "stream poll already running")?;
            match tokio::time::timeout(Duration::from_millis(25), events.recv()).await {
                Err(_) => Ok(json!({"state":"pending"})),
                Ok(None) => {
                    h.busy.store(false, Ordering::Release);
                    Err("stream worker closed".into())
                }
                Ok(Some(Event::Auth(mut request, reply, deadline))) => {
                    let nonce = next_id();
                    request["state"] = json!("authorize");
                    request["nonce"] = json!(nonce.to_string());
                    *h.pending.lock().unwrap() = Some((nonce, reply, deadline));
                    Ok(request)
                }
                Ok(Some(Event::Result(result))) => {
                    h.busy.store(false, Ordering::Release);
                    result.map(|result| json!({"state":"done","result":result}))
                }
            }
        }
        Some("stream_authorize") => {
            let nonce: u64 = v["nonce"]
                .as_str()
                .ok_or("authorization nonce required")?
                .parse()
                .map_err(|_| "invalid authorization nonce")?;
            let decisions: Vec<bool> = parse(&v["decisions"])?;
            let mut pending = h.pending.lock().unwrap();
            if pending.as_ref().map(|p| p.0) != Some(nonce) {
                return Err("stale authorization reply".into());
            }
            let (_, reply, deadline) = pending.take().unwrap();
            if std::time::Instant::now() >= deadline {
                return Err("authorization expired".into());
            }
            reply.send(decisions).map_err(|_| "authorization expired")?;
            Ok(json!({}))
        }
        _ => Err("unknown stream command".into()),
    }
}
fn open(v: Value, log: Arc<dyn ChangeLog>) -> Result<Value, String> {
    let timeout = integer(&v, "auth_timeout_ms", 30_000)?;
    if timeout == 0 || timeout > 300_000 {
        return Err("auth timeout must be 1..=300000ms".into());
    }
    let (event_tx, event_rx) = mpsc::channel(1);
    let auth = Arc::new(HostAuth {
        events: event_tx.clone(),
        timeout: Duration::from_millis(timeout),
    });
    let options = &v["limits"];
    if !options.is_null() && !options.is_object() {
        return Err("limits must be an object".into());
    }
    let timeout_ms = integer(options, "timeout_ms", 60_000)?;
    if timeout_ms == 0 || timeout_ms > 86_400_000 {
        return Err("query timeout must be 1..=86400000ms".into());
    }
    let limits = Limits {
        entities: integer(options, "entities", 1_000_000)?
            .try_into()
            .map_err(|_| "entity limit too large")?,
        records: integer(options, "records", 10_000_000)?
            .try_into()
            .map_err(|_| "record limit too large")?,
        timeout: Duration::from_millis(timeout_ms),
    };
    let connection = StreamConnection::new(
        parse(&v["schema"])?,
        v["source"]
            .as_str()
            .ok_or("source incarnation required")?
            .into(),
        log,
        auth,
        limits,
    )?;
    let (tx, rx) = mpsc::channel(1);
    let (task, registration) = futures::future::AbortHandle::new_pair();
    executor()?
        .send((connection, rx, event_tx, registration))
        .map_err(|_| "stream executor closed")?;
    let handle = Arc::new(Handle {
        commands: tx,
        events: AsyncMutex::new(event_rx),
        pending: Mutex::new(None),
        busy: AtomicBool::new(false),
        task,
    });
    let id = next_id().to_string();
    registry().lock().unwrap().insert(id.clone(), handle);
    Ok(json!({"id":id}))
}

type Job = (
    StreamConnection,
    mpsc::Receiver<Value>,
    mpsc::Sender<Event>,
    futures::future::AbortRegistration,
);
fn executor() -> Result<&'static mpsc::UnboundedSender<Job>, String> {
    static EXECUTOR: OnceLock<Result<mpsc::UnboundedSender<Job>, String>> = OnceLock::new();
    EXECUTOR
        .get_or_init(|| {
            let (tx, mut rx) = mpsc::unbounded_channel::<Job>();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            std::thread::Builder::new()
                .name("orchid-streams".into())
                .stack_size(16 * 1024 * 1024)
                .spawn(move || {
                    let local = tokio::task::LocalSet::new();
                    runtime.block_on(local.run_until(async move {
                        while let Some((connection, commands, events, registration)) =
                            rx.recv().await
                        {
                            tokio::task::spawn_local(futures::future::Abortable::new(
                                worker(connection, commands, events),
                                registration,
                            ));
                        }
                    }));
                })
                .map_err(|e| e.to_string())?;
            Ok(tx)
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Log(Mutex<Vec<Change>>);
    #[async_trait]
    impl ChangeLog for Log {
        async fn append(&self, c: &Change) -> Result<(i32, i64), String> {
            let mut rows = self.0.lock().unwrap();
            let offset = rows.len() as i64;
            rows.push(c.clone());
            Ok((0, offset))
        }
        async fn bounds(&self) -> Result<Bounds, String> {
            Ok(Bounds {
                low: BTreeMap::from([(0, 0)]),
                high: BTreeMap::from([(0, self.0.lock().unwrap().len() as i64)]),
            })
        }
        fn follow(
            &self,
            offsets: Offsets,
        ) -> Result<BoxStream<'static, Result<LogItem, String>>, String> {
            let items: Vec<_> = self
                .0
                .lock()
                .unwrap()
                .iter()
                .enumerate()
                .skip(offsets[&0] as usize)
                .map(|(i, c)| {
                    Ok(LogItem::Record(LogRecord {
                        partition: 0,
                        offset: i as i64,
                        change: c.clone(),
                    }))
                })
                .collect();
            Ok(futures::stream::iter(items).boxed())
        }
    }
    fn options() -> Value {
        json!({"schema":{"tables":{"docs":{"label":"Doc","resource_type":"document","read_permission":"view","write_permission":"edit","properties":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}}},"source":"test-incarnation","auth_timeout_ms":1000})
    }
    fn subject() -> Value {
        json!({"subject_type":"user","subject_id":"alice"})
    }
    fn change(key: &str, version: u64) -> Value {
        json!({"table":"docs","key":key,"version":version,"data":{"name":format!("{key}-{version}")}})
    }
    async fn call(id: &str, op: &str, mut v: Value) -> Result<Value, String> {
        v["op"] = json!(op);
        v["id"] = json!(id);
        command(v).await
    }
    async fn run(id: &str, v: Value, allow: bool) -> Result<Value, String> {
        call(id, "stream_start", v).await?;
        loop {
            let e = call(id, "stream_poll", json!({})).await?;
            match e["state"].as_str().unwrap() {
                "done" => return Ok(e["result"].clone()),
                "authorize" => {
                    assert_eq!(e["subject"], subject());
                    let decisions = vec![allow; e["checks"].as_array().unwrap().len()];
                    call(
                        id,
                        "stream_authorize",
                        json!({"nonce":e["nonce"],"decisions":decisions}),
                    )
                    .await?;
                }
                _ => {}
            }
        }
    }
    fn decode(v: &Value) -> RecordBatch {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(v["ipc"].as_str().unwrap())
            .unwrap();
        arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
    }
    #[test]
    fn frame_buffers_are_shared_and_lossless() {
        use arrow::array::{ArrayRef, UInt64Array};
        let frames_value = (0..64)
            .map(|i| {
                let b = RecordBatch::try_from_iter([(
                    "id",
                    Arc::new(UInt64Array::from(vec![u64::MAX - i])) as ArrayRef,
                )])
                .unwrap();
                let schema = Arc::new(arrow::datatypes::Schema::new_with_metadata(
                    b.schema().fields().clone(),
                    std::collections::HashMap::from([
                        ("orchiddb.cypher.typed_columns.v1".into(), "[\"id\"]".into()),
                        (
                            "orchiddb.cypher.typed_rows.v1".into(),
                            json!([[{"type":"uint64","value":u64::MAX-i}]]).to_string(),
                        ),
                    ]),
                ));
                Frame {
                    table: "docs".into(),
                    key: i.to_string(),
                    version: u64::MAX,
                    deleted: false,
                    rows: Some(RecordBatch::try_new(schema, b.columns().to_vec()).unwrap()),
                    offsets: BTreeMap::from([(0, i as i64 + 1)]),
                }
            })
            .collect();
        let wire = frames(Some(FrameBatch {
            frames: frames_value,
            offsets: BTreeMap::from([(0, 64)]),
        }))
        .unwrap();
        assert_eq!(wire["batches"].as_array().unwrap().len(), 1);
        let batch = decode(&wire["batches"][0]);
        assert_eq!(batch.num_rows(), 64);
        let typed: Vec<Value> =
            serde_json::from_str(&batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"])
                .unwrap();
        assert_eq!(typed.len(), 64);
        assert_eq!(typed[63][0]["value"], json!(u64::MAX - 63));
        let values = batch
            .column(0)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        for i in 0..64 {
            assert_eq!(
                wire["frames"][i]["result"],
                json!({"batch":0,"offset":i,"length":1})
            );
            assert_eq!(wire["frames"][i]["version"], u64::MAX.to_string());
            assert_eq!(values.value(i), u64::MAX - i as u64);
        }
        assert!(integer(&json!({"wait_ms":-1}), "wait_ms", 100).is_err());
        assert!(integer(&json!({"wait_ms":"100"}), "wait_ms", 100).is_err());
    }
    #[tokio::test]
    async fn late_permission_grant_cannot_write() {
        let log = Arc::new(Log::default());
        let mut opts = options();
        opts["auth_timeout_ms"] = json!(10);
        let id = open(opts, log.clone()).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        call(
            &id,
            "stream_start",
            json!({"action":"insert","subject":subject(),"change":change("a",1)}),
        )
        .await
        .unwrap();
        let event = loop {
            let e = call(&id, "stream_poll", json!({})).await.unwrap();
            if e["state"] == "authorize" {
                break e;
            }
        };
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            call(
                &id,
                "stream_authorize",
                json!({"nonce":event["nonce"],"decisions":[true]})
            )
            .await
            .unwrap_err()
            .contains("expired")
        );
        assert!(log.0.lock().unwrap().is_empty());
        call(&id, "stream_close", json!({})).await.unwrap();
    }
    #[tokio::test]
    async fn host_auth_guards_insertion_queries_hybrid_and_streams() {
        let log = Arc::new(Log::default());
        let id = open(options(), log.clone()).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let insert = json!({"action":"insert","subject":subject(),"change":change("a",1)});
        assert!(
            run(&id, insert.clone(), false)
                .await
                .unwrap_err()
                .contains("denied")
        );
        assert_eq!(log.0.lock().unwrap().len(), 0);
        run(&id, insert, true).await.unwrap();
        run(&id,json!({"action":"insert_batch","subject":subject(),"changes":[change("b",1),change("a",2)]}),true).await.unwrap();
        let q = json!({"action":"query","subject":subject(),"text":"MATCH (d:Doc) RETURN d.name AS name ORDER BY name"});
        assert_eq!(
            decode(&run(&id, q.clone(), true).await.unwrap()).num_rows(),
            2
        );
        assert_eq!(
            decode(&run(&id, q.clone(), false).await.unwrap()).num_rows(),
            0
        );
        let mut hybrid = q;
        hybrid["action"] = json!("hybrid");
        hybrid["snapshot"] =
            json!({"source":"test-incarnation","offsets":{"0":1},"changes":[change("a",1)]});
        assert_eq!(decode(&run(&id, hybrid, true).await.unwrap()).num_rows(), 2);
        let subscribe = json!({"action":"subscribe","subject":subject(),"text":"MATCH (d:Doc) RETURN d.name AS name","offsets":{"0":0}});
        run(&id, subscribe.clone(), true).await.unwrap();
        let mut collected = Vec::new();
        let mut offsets = json!({});
        for _ in 0..5 {
            let frames = run(
                &id,
                json!({"action":"next","max_records":2,"wait_ms":10}),
                true,
            )
            .await
            .unwrap();
            collected.extend(frames["frames"].as_array().unwrap().clone());
            offsets = frames["offsets"].clone();
            if offsets["0"] == 3 {
                break;
            }
        }
        assert_eq!(collected.len(), 3);
        assert_eq!(offsets["0"], 3);
        run(&id, json!({"action":"unsubscribe"}), true)
            .await
            .unwrap();
        run(&id, subscribe, true).await.unwrap();
        let denied = run(
            &id,
            json!({"action":"next","max_records":2,"wait_ms":1}),
            false,
        )
        .await
        .unwrap();
        assert_eq!(denied["frames"], json!([]));
        assert!(!denied.to_string().contains("a-1"));
        call(&id, "stream_close", json!({})).await.unwrap();
        assert!(call(&id, "stream_start", json!({})).await.is_err());
    }
    #[tokio::test]
    async fn auth_is_bound_to_challenge_fails_closed_and_cancels() {
        let log = Arc::new(Log::default());
        let id = open(options(), log.clone()).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        call(
            &id,
            "stream_start",
            json!({"action":"insert","subject":subject(),"change":change("a",1)}),
        )
        .await
        .unwrap();
        assert!(
            call(&id, "stream_start", json!({}))
                .await
                .unwrap_err()
                .contains("busy")
        );
        let e = loop {
            let e = call(&id, "stream_poll", json!({})).await.unwrap();
            if e["state"] == "authorize" {
                break e;
            }
        };
        assert!(
            call(
                &id,
                "stream_authorize",
                json!({"nonce":"0","decisions":[true]})
            )
            .await
            .is_err()
        );
        assert!(
            call(
                &id,
                "stream_authorize",
                json!({"nonce":e["nonce"],"decisions":[1]})
            )
            .await
            .is_err()
        );
        call(
            &id,
            "stream_authorize",
            json!({"nonce":e["nonce"],"decisions":[]}),
        )
        .await
        .unwrap();
        let error = loop {
            match call(&id, "stream_poll", json!({})).await {
                Err(e) => break e,
                Ok(_) => {}
            }
        };
        assert!(error.contains("one boolean"));
        assert!(log.0.lock().unwrap().is_empty());
        // A previous grant cannot authorize the next operation.
        call(
            &id,
            "stream_start",
            json!({"action":"insert","subject":subject(),"change":change("b",1)}),
        )
        .await
        .unwrap();
        assert!(
            call(
                &id,
                "stream_authorize",
                json!({"nonce":e["nonce"],"decisions":[true]})
            )
            .await
            .is_err()
        );
        call(&id, "stream_close", json!({})).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(log.0.lock().unwrap().is_empty());
    }
}
