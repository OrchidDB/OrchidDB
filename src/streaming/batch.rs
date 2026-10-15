//! Bounded batching keeps authorization checks fresh while amortizing network
//! round trips, graph construction, and Arrow result conversion.
use super::*;
use std::{
    collections::{BTreeSet, HashMap},
    time::Instant,
};

/// All changes in a batch are checked before any produce call. Delivery itself
/// is not transactional; an error may follow partial delivery. Receipts preserve
/// input order, and repeated entity keys are produced serially in input order.
impl StreamConnection {
    pub async fn insert_batch(
        &self,
        subject: &Authorization,
        changes: &[Change],
    ) -> Result<Vec<(i32, i64)>, String> {
        if changes.len() > 4096 {
            return Err("insertion batch exceeds 4096 records".into());
        }
        let mut checks = Vec::with_capacity(changes.len());
        for change in changes {
            let table = self.validate_change(change)?;
            if change.data.is_some() {
                properties(change)?;
            }
            checks.push(PermissionCheck {
                resource_type: &table.resource_type,
                resource_id: &change.key,
                permission: &table.write_permission,
            });
        }
        let decisions = self.auth.check_many(subject, &checks).await?;
        if decisions.len() != changes.len() || decisions.iter().any(|v| !v) {
            return Err("insertion batch permission denied or incomplete".into());
        }
        let mut groups = BTreeMap::new();
        for (i, c) in changes.iter().enumerate() {
            groups
                .entry((&c.table, &c.key))
                .or_insert_with(Vec::new)
                .push((i, c));
        }
        let delivered: Vec<Vec<_>> = futures::stream::iter(groups.into_values())
            .map(|group| async move {
                let mut receipts = Vec::with_capacity(group.len());
                for (i, change) in group {
                    receipts.push((i, self.log.append(change).await?));
                }
                Ok::<_, String>(receipts)
            })
            .buffer_unordered(4096)
            .try_collect()
            .await?;
        let mut receipts = vec![(0, 0); changes.len()];
        for (i, receipt) in delivered.into_iter().flatten() {
            receipts[i] = receipt;
        }
        Ok(receipts)
    }
}
/// A checkpoint is safe to persist only after processing every frame in this
/// batch. Empty batches are heartbeats, or represent entirely denied/duplicate
/// input. They never contain denied resource identifiers or payloads.
pub struct FrameBatch {
    pub frames: Vec<Frame>,
    pub offsets: Offsets,
}
impl Subscription<'_> {
    /// Read up to max_records (1..=4096), waiting at most max_wait for the input
    /// batch. Authorization/execution have their own deadlines. Fully consistent
    /// authorization is evaluated per batch, with no cross-batch permission cache.
    /// Repeated keys end a batch so each version keeps its own replacement result.
    /// Cancelling collection or authorization retains all consumed input.
    pub async fn next_batch(
        &mut self,
        max_records: usize,
        max_wait: Duration,
    ) -> Result<Option<FrameBatch>, String> {
        if self.failed {
            return Err("subscription terminated after an error".into());
        }
        if max_records == 0 || max_records > 4096 || max_wait.is_zero() {
            return Err("batch requires 1..=4096 records and a positive wait".into());
        }
        let result = self.batch_inner(max_records, max_wait).await;
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    async fn batch_inner(
        &mut self,
        max_records: usize,
        max_wait: Duration,
    ) -> Result<Option<FrameBatch>, String> {
        if let Some(record) = self.pending.take() {
            self.buffered.push_front(LogItem::Record(record));
        }
        let deadline = Instant::now()
            .checked_add(max_wait)
            .ok_or("invalid batch wait")?;
        let mut count = 0;
        let mut items = 0;
        let mut keys = BTreeSet::new();
        let mut ended = false;
        loop {
            if count == max_records || items >= max_records * 8 {
                break;
            }
            if items == self.buffered.len() {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    break;
                };
                match tokio::time::timeout(remaining, self.stream.next()).await {
                    Err(_) => break,
                    Ok(None) => {
                        ended = true;
                        break;
                    }
                    Ok(Some(item)) => self.buffered.push_back(item?),
                }
            }
            if let LogItem::Record(record) = &self.buffered[items] {
                if !keys.insert((record.change.table.clone(), record.change.key.clone())) {
                    break;
                }
                count += 1;
            }
            items += 1;
        }
        if items == 0 && ended {
            return Ok(None);
        }
        let mut checks = Vec::new();
        let mut positions = Vec::new();
        for item in self.buffered.iter().take(items) {
            if let LogItem::Record(record) = item {
                let offset = *self
                    .offsets
                    .get(&record.partition)
                    .ok_or("unexpected partition")?;
                if record.offset < offset {
                    continue;
                }
                let c = &record.change;
                let table = self.connection.validate_change(c)?;
                if let Some(previous) = self.state.get(&(c.table.clone(), c.key.clone())) {
                    if previous.version == c.version && previous != c {
                        return Err("conflicting payloads for the same entity version".into());
                    }
                    if previous.version >= c.version {
                        continue;
                    }
                }
                checks.push(PermissionCheck {
                    resource_type: &table.resource_type,
                    resource_id: &c.key,
                    permission: &table.read_permission,
                });
                positions.push((record.partition, record.offset));
            }
        }
        let decisions = self
            .connection
            .auth
            .check_many(&self.subject, &checks)
            .await?;
        if decisions.len() != positions.len() {
            return Err("incomplete batch authorization response".into());
        }
        let allowed: HashMap<_, _> = positions.into_iter().zip(decisions).collect();
        let visible = self
            .buffered
            .iter()
            .take(items)
            .filter_map(|item| match item {
                LogItem::Record(r)
                    if r.change.data.is_some()
                        && allowed.get(&(r.partition, r.offset)) == Some(&true) =>
                {
                    Some((&r.change, &self.connection.schema.tables[&r.change.table]))
                }
                _ => None,
            })
            .collect();
        let graph = self.connection.visible_graph(visible)?;
        let (batch, index) =
            self.program
                .execute_indexed(&self.plan, graph, self.connection.limits.timeout)?;
        let typed_key = "orchiddb.cypher.typed_rows.v1";
        let mut metadata = batch.schema().metadata().clone();
        let typed_rows: Vec<Value> = metadata
            .remove(typed_key)
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let frame_rows = |row: Option<usize>| -> Result<RecordBatch, String> {
            let mut metadata = metadata.clone();
            let selected = match row {
                Some(i) => &typed_rows[i..i + 1],
                None => &typed_rows[0..0],
            };
            metadata.insert(
                typed_key.into(),
                serde_json::to_string(selected).map_err(|e| e.to_string())?,
            );
            let slice = match row {
                Some(i) => batch.slice(i, 1),
                None => batch.slice(0, 0),
            };
            let schema = Arc::new(arrow::datatypes::Schema::new_with_metadata(
                slice.schema().fields().clone(),
                metadata,
            ));
            RecordBatch::try_new(schema, slice.columns().to_vec()).map_err(|e| e.to_string())
        };
        let mut frames = Vec::new();
        // No await after this point: cancellation cannot interrupt a partial commit.
        for _ in 0..items {
            match self.buffered.pop_front().unwrap() {
                LogItem::Progress {
                    partition,
                    next_offset,
                } => {
                    let offset = self
                        .offsets
                        .get_mut(&partition)
                        .ok_or("unexpected partition")?;
                    *offset = (*offset).max(next_offset);
                }
                LogItem::Record(record) => {
                    let offset = self
                        .offsets
                        .get_mut(&record.partition)
                        .ok_or("unexpected partition")?;
                    if record.offset < *offset {
                        continue;
                    }
                    *offset = record.offset.checked_add(1).ok_or("offset overflow")?;
                    let Some(authorized) = allowed.get(&(record.partition, record.offset)) else {
                        continue;
                    };
                    let c = record.change;
                    self.connection.merge(&mut self.state, c.clone())?;
                    if *authorized {
                        let rows = if c.data.is_none() {
                            None
                        } else {
                            Some(frame_rows(index.get(&c.kafka_key()).copied())?)
                        };
                        frames.push(Frame {
                            table: c.table,
                            key: c.key,
                            version: c.version,
                            deleted: c.data.is_none(),
                            rows,
                            offsets: self.offsets.clone(),
                        });
                    }
                }
            }
        }
        Ok(Some(FrameBatch {
            frames,
            offsets: self.offsets.clone(),
        }))
    }
}
use futures::TryStreamExt;
