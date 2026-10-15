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
    Auth(Value, Reply),
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
        self.events
            .send(Event::Auth(json!({"subject":subject,"checks":checks}), tx))
            .await
            .map_err(|_| "authorization client closed")?;
        let decisions = tokio::time::timeout(self.timeout, rx)
            .await
            .map_err(|_| "authorization callback timed out")?
            .map_err(|_| "authorization callback cancelled")?;
        if decisions.len() != count {
            return Err("authorization must return one boolean per check".into());
        }
        Ok(decisions)
    }
}
struct Handle {
    commands: mpsc::Sender<Value>,
    events: AsyncMutex<mpsc::Receiver<Event>>,
    pending: Mutex<Option<(u64, Reply)>>,
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
    let frames = b.frames.into_iter().map(|f| Ok(json!({"table":f.table,"key":f.key,"version":f.version.to_string(),"deleted":f.deleted,"result":f.rows.map(batch).transpose()?,"offsets":f.offsets}))).collect::<Result<Vec<Value>, String>>()?;
    Ok(json!({"frames":frames,"offsets":b.offsets}))
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
                            let max = v["max_records"].as_u64().unwrap_or(1);
                            let wait = v["wait_ms"].as_u64().unwrap_or(100);
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
                Ok(Some(Event::Auth(mut request, reply))) => {
                    let nonce = next_id();
                    request["state"] = json!("authorize");
                    request["nonce"] = json!(nonce.to_string());
                    *h.pending.lock().unwrap() = Some((nonce, reply));
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
            pending
                .take()
                .unwrap()
                .1
                .send(decisions)
                .map_err(|_| "authorization expired")?;
            Ok(json!({}))
        }
        _ => Err("unknown stream command".into()),
    }
}
fn open(v: Value, log: Arc<dyn ChangeLog>) -> Result<Value, String> {
    let timeout = v["auth_timeout_ms"].as_u64().unwrap_or(30_000);
    if timeout == 0 || timeout > 300_000 {
        return Err("auth timeout must be 1..=300000ms".into());
    }
    let (event_tx, event_rx) = mpsc::channel(1);
    let auth = Arc::new(HostAuth {
        events: event_tx.clone(),
        timeout: Duration::from_millis(timeout),
    });
    let connection = StreamConnection::new(
        parse(&v["schema"])?,
        v["source"]
            .as_str()
            .ok_or("source incarnation required")?
            .into(),
        log,
        auth,
        Limits::default(),
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
