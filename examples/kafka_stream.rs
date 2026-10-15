//! KAFKA_BOOTSTRAP, KAFKA_TOPIC, KAFKA_SOURCE_ID, ORCHID_SUBJECT, ORCHID_ENTITY and ORCHID_VERSION identify services/data.
use async_trait::async_trait;
use orchiddb::{compiler::Authorization, session::Query, streaming::*};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
// Example client-owned policy. Replace with your trusted authorizer/cache.
struct ClientPolicy {
    subject: String,
    key: String,
}
#[async_trait]
impl Authorizer for ClientPolicy {
    async fn check(
        &self,
        subject: &Authorization,
        resource_type: &str,
        resource_id: &str,
        permission: &str,
    ) -> Result<bool, String> {
        Ok(subject.subject_type == "user"
            && subject.subject_id == self.subject
            && resource_type == "person"
            && resource_id == self.key
            && ["view", "edit"].contains(&permission))
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = |key| std::env::var(key).map_err(|_| format!("set {key}"));
    let mut config = rdkafka::ClientConfig::new();
    config
        .set("bootstrap.servers", env("KAFKA_BOOTSTRAP")?)
        // Avoid an intentional linger delay for this single-record example.
        .set("linger.ms", "0");
    let log = Arc::new(KafkaLog::new(config, env("KAFKA_TOPIC")?)?);
    let auth = Arc::new(ClientPolicy {
        subject: env("ORCHID_SUBJECT")?,
        key: env("ORCHID_ENTITY")?,
    });
    let schema = StreamSchema {
        tables: BTreeMap::from([(
            "people".into(),
            Table {
                label: "Person".into(),
                resource_type: "person".into(),
                read_permission: "view".into(),
                write_permission: "edit".into(),
                edge: None,
                properties: json!({"type":"object","required":["name"],"properties":{"name":{"type":"string"}}}),
            },
        )]),
    };
    let subject = Authorization::new("user", env("ORCHID_SUBJECT")?);
    let connection = StreamConnection::new(
        schema,
        env("KAFKA_SOURCE_ID")?,
        log.clone(),
        auth,
        Limits::default(),
    )?;
    // Capture the start BEFORE producing so the subscription sees this event.
    let start = log.bounds().await?.high;
    let version = env("ORCHID_VERSION")?.parse()?;
    connection
        .insert(
            &subject,
            Change {
                table: "people".into(),
                key: env("ORCHID_ENTITY")?,
                version,
                data: Some(json!({"name":"Alice"}).as_object().unwrap().clone()),
            },
        )
        .await?;
    let mut query = Query::cypher("MATCH (p:Person) WHERE p.name = $name RETURN p.name AS name");
    query.parameters.insert("name".into(), json!("Alice"));
    query.authorization = Some(subject);
    let mut subscription = connection.subscribe(query, start).await?;
    while let Some(frame) = subscription.next().await? {
        println!(
            "{}:{} version={} deleted={} rows={:?} checkpoint={:?}",
            frame.table, frame.key, frame.version, frame.deleted, frame.rows, frame.offsets
        );
    }
    Ok(())
}
