use super::*;
use rdkafka::{
    ClientConfig, Message, Offset, TopicPartitionList,
    consumer::{Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord},
};
/// One topic per materialization. All clients use explicit partitions, disabled
/// commits, read_committed, and fail on offset reset. TLS/SASL settings are supplied
/// through librdkafka ClientConfig. Required safety settings override caller values.
pub struct KafkaLog {
    config: ClientConfig,
    topic: String,
    producer: FutureProducer,
}
impl KafkaLog {
    pub fn new(mut config: ClientConfig, topic: String) -> Result<Self, String> {
        if topic.is_empty() {
            return Err("Kafka topic required".into());
        }
        config
            .set("enable.idempotence", "true")
            .set("acks", "all")
            .set("message.timeout.ms", "30000");
        let producer = config
            .create()
            .map_err(|e| format!("Kafka producer: {e}"))?;
        Ok(Self {
            config,
            topic,
            producer,
        })
    }
    fn consumer(&self) -> Result<StreamConsumer, String> {
        let mut config = self.config.clone();
        config
            .set("group.id", "orchiddb-independent-reader")
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", "error")
            .set("isolation.level", "read_committed")
            .set("enable.partition.eof", "true");
        config.create().map_err(|e| format!("Kafka consumer: {e}"))
    }
}
#[async_trait]
impl ChangeLog for KafkaLog {
    async fn append(&self, change: &Change) -> Result<(i32, i64), String> {
        let payload = serde_json::to_vec(change).map_err(|e| e.to_string())?;
        let key = change.kafka_key();
        let delivery = self
            .producer
            .send(
                FutureRecord::to(&self.topic).key(&key).payload(&payload),
                Duration::from_secs(10),
            )
            .await
            .map_err(|(e, _)| format!("Kafka delivery failed: {e}"))?;
        Ok((delivery.partition, delivery.offset))
    }
    async fn bounds(&self) -> Result<Bounds, String> {
        let consumer = self.consumer()?;
        let topic = self.topic.clone();
        tokio::task::spawn_blocking(move || {
            let metadata = consumer
                .fetch_metadata(Some(&topic), Duration::from_secs(10))
                .map_err(|e| e.to_string())?;
            let metadata = metadata
                .topics()
                .iter()
                .find(|t| t.name() == topic)
                .ok_or("Kafka topic missing")?;
            if metadata.error().is_some() || metadata.partitions().is_empty() {
                return Err("Kafka topic metadata unavailable".into());
            }
            let mut bounds = Bounds {
                low: Offsets::new(),
                high: Offsets::new(),
            };
            for p in metadata.partitions() {
                let (low, high) = consumer
                    .fetch_watermarks(&topic, p.id(), Duration::from_secs(10))
                    .map_err(|e| e.to_string())?;
                bounds.low.insert(p.id(), low);
                bounds.high.insert(p.id(), high);
            }
            Ok(bounds)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    fn follow(
        &self,
        offsets: Offsets,
    ) -> Result<BoxStream<'static, Result<LogItem, String>>, String> {
        let consumer = self.consumer()?;
        let mut assignment = TopicPartitionList::new();
        for (partition, offset) in offsets {
            assignment
                .add_partition_offset(&self.topic, partition, Offset::Offset(offset))
                .map_err(|e| e.to_string())?;
        }
        consumer.assign(&assignment).map_err(|e| e.to_string())?;
        let topic = self.topic.clone();
        Ok(Box::pin(futures::stream::try_unfold(
            (consumer, topic),
            |(consumer, topic)| async move {
                let item = loop {
                    break match consumer.recv().await {
                        Ok(message) => {
                            let payload = message.payload().ok_or(
                                "bare Kafka tombstone: use a versioned Change delete envelope",
                            )?;
                            let change: Change = serde_json::from_slice(payload)
                                .map_err(|e| format!("invalid Kafka change: {e}"))?;
                            if message.key() != Some(change.kafka_key().as_bytes()) {
                                return Err("Kafka key does not match entity identity".into());
                            }
                            LogItem::Record(LogRecord {
                                partition: message.partition(),
                                offset: message.offset(),
                                change,
                            })
                        }
                        Err(rdkafka::error::KafkaError::PartitionEOF(partition)) => {
                            let positions = consumer.position().map_err(|e| e.to_string())?;
                            let offset = positions
                                .find_partition(&topic, partition)
                                .ok_or("Kafka position missing")?
                                .offset();
                            let Offset::Offset(next_offset) = offset else {
                                // An empty partition can report EOF before its position is known.
                                // Do not infer progress from the high watermark (open transactions).
                                continue;
                            };
                            LogItem::Progress {
                                partition,
                                next_offset,
                            }
                        }
                        Err(e) => return Err(format!("Kafka read failed: {e}")),
                    };
                };
                Ok(Some((item, (consumer, topic))))
            },
        )))
    }
}
