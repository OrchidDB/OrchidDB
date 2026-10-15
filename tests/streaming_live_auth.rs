#![cfg(all(feature = "kafka", feature = "orchid-catalog"))]
use async_trait::async_trait;
use base64::Engine;
use orchiddb::{compiler::Authorization, session::Query, streaming::*};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
struct Auth {
    url: String,
    http: reqwest::Client,
}
#[async_trait]
impl Authorizer for Auth {
    async fn check(&self, s: &Authorization, t: &str, id: &str, p: &str) -> Result<bool, String> {
        Ok(self
            .check_many(
                s,
                &[PermissionCheck {
                    resource_type: t,
                    resource_id: id,
                    permission: p,
                }],
            )
            .await?[0])
    }
    async fn check_many(
        &self,
        s: &Authorization,
        checks: &[PermissionCheck<'_>],
    ) -> Result<Vec<bool>, String> {
        let checks:Vec<_>=checks.iter().map(|c|json!({"resource_type":c.resource_type,"resource_id":c.resource_id,"permission":c.permission})).collect();
        self.http
            .post(&self.url)
            .json(&json!({"subject":s,"checks":checks}))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())
    }
}
struct Supplied(Snapshot);
#[async_trait(?Send)]
impl SnapshotSource for Supplied {
    async fn load(&mut self) -> Result<Snapshot, String> {
        Ok(self.0.clone())
    }
}
fn change(key: &str, version: u64) -> Change {
    serde_json::from_value(json!({"table":"docs","key":key,"version":version,"data":{"name":format!("{key}-{version}")}})).unwrap()
}
fn batch(b: arrow::record_batch::RecordBatch) -> Value {
    let mut bytes = vec![];
    {
        let mut w = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, &b.schema()).unwrap();
        w.write(&b).unwrap();
        w.finish().unwrap();
    }
    json!({"ipc":base64::engine::general_purpose::STANDARD.encode(bytes)})
}
#[tokio::test]
async fn rust_kafka_uses_application_spicedb_route() {
    let Ok(path) = std::env::var("ORCHIDDB_STREAM_FIXTURE") else {
        eprintln!("SKIP: live Kafka/SpiceDB fixture required");
        return;
    };
    let f: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut config = rdkafka::ClientConfig::new();
    for (k, v) in f["kafka"].as_object().unwrap() {
        config.set(k, v.as_str().unwrap());
    }
    let log = Arc::new(
        KafkaLog::new(
            config,
            format!("{}-rust", f["topic_prefix"].as_str().unwrap()),
        )
        .unwrap(),
    );
    let auth = Arc::new(Auth {
        url: f["auth_url"].as_str().unwrap().into(),
        http: reqwest::Client::new(),
    });
    let c = StreamConnection::new(
        serde_json::from_value(f["schema"].clone()).unwrap(),
        f["source"].as_str().unwrap().into(),
        log,
        auth,
        Limits::default(),
    )
    .unwrap();
    let subject: Authorization = serde_json::from_value(f["subject"].clone()).unwrap();
    assert!(
        c.insert(&subject, change("blocked", 1))
            .await
            .unwrap_err()
            .contains("denied")
    );
    c.insert(&subject, change("a", 1)).await.unwrap();
    c.insert_batch(
        &subject,
        &[change("b", 1), change("hidden", 1), change("a", 2)],
    )
    .await
    .unwrap();
    let mut q = Query::cypher(f["query"].as_str().unwrap());
    q.authorization = Some(subject.clone());
    let query = batch(c.query(q.clone()).await.unwrap());
    let snapshot = Snapshot {
        source: f["source"].as_str().unwrap().into(),
        offsets: serde_json::from_value(f["offsets"].clone()).unwrap(),
        changes: vec![change("a", 1), change("b", 1)],
    };
    let hybrid = batch(c.query_hybrid(q, &mut Supplied(snapshot)).await.unwrap());
    let mut q = Query::cypher(f["stream_query"].as_str().unwrap());
    q.authorization = Some(subject);
    let mut stream = c
        .subscribe(q, serde_json::from_value(f["offsets"].clone()).unwrap())
        .await
        .unwrap();
    let mut frames = vec![];
    let mut offsets = Offsets::new();
    for _ in 0..20 {
        let b = stream
            .next_batch(16, Duration::from_millis(100))
            .await
            .unwrap()
            .unwrap();
        offsets = b.offsets;
        for f in b.frames {
            frames.push(json!({"table":f.table,"key":f.key,"version":f.version.to_string(),"deleted":f.deleted,"result":f.rows.map(batch),"offsets":f.offsets}));
        }
        if offsets.get(&0) == Some(&4) {
            break;
        }
    }
    assert_eq!(frames.len(), 3);
    assert_eq!(offsets[&0], 4);
    std::fs::write(
        "/tmp/orchid-stream-rust-result.json",
        json!({"query":query,"hybrid":hybrid,"frames":frames,"offsets":offsets}).to_string(),
    )
    .unwrap();
}
