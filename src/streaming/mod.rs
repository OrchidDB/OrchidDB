//! Versioned Kafka materializations, bounded hybrid reads, and event-local Cypher.
//! Flink owns stateful streaming computation. Schema and policy are registered
//! separately from query text, and all reads authorize before graph execution.
use crate::{
    compiler::Authorization,
    ir::{GraphPlan, Node, PropertyGraph},
    session::Query,
};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
mod batch;
mod event;
pub use batch::FrameBatch;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[cfg(feature = "kafka")]
mod kafka;
#[cfg(feature = "kafka")]
pub use kafka::KafkaLog;
#[cfg(feature = "spicedb")]
mod spicedb;
#[cfg(feature = "spicedb")]
pub use spicedb::SpiceDb;
#[cfg(feature = "duckdb")]
mod iceberg;
#[cfg(feature = "duckdb")]
pub use iceberg::DuckDbSnapshot;

/// Next offsets, never last-consumed offsets. A checkpoint covers every partition.
pub type Offsets = BTreeMap<i32, i64>;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub table: String,
    pub key: String,
    /// Monotonic per (table,key), assigned by the materializer, not wall-clock time.
    pub version: u64,
    /// None is a versioned delete. Keep deletes in Iceberg until replay is impossible.
    pub data: Option<Map<String, Value>>,
}
impl Change {
    pub fn kafka_key(&self) -> String {
        serde_json::to_string(&(&self.table, &self.key)).expect("string pair")
    }
}
#[derive(Clone, Debug)]
pub struct LogRecord {
    pub partition: i32,
    pub offset: i64,
    pub change: Change,
}
#[derive(Clone, Debug)]
pub enum LogItem {
    Record(LogRecord),
    /// Consumer position; needed to cross compacted offsets and transaction gaps.
    Progress {
        partition: i32,
        next_offset: i64,
    },
}
#[derive(Clone, Debug)]
pub struct Bounds {
    pub low: Offsets,
    pub high: Offsets,
}
#[async_trait]
pub trait ChangeLog: Send + Sync {
    async fn append(&self, change: &Change) -> Result<(i32, i64), String>;
    async fn bounds(&self) -> Result<Bounds, String>;
    /// Independent, manually assigned reader. Dropping it cancels consumption.
    fn follow(
        &self,
        offsets: Offsets,
    ) -> Result<BoxStream<'static, Result<LogItem, String>>, String>;
}
/// One resource check, evaluated for the batch's authenticated subject.
#[derive(Clone, Copy)]
pub struct PermissionCheck<'a> {
    pub resource_type: &'a str,
    pub resource_id: &'a str,
    pub permission: &'a str,
}
#[async_trait]
pub trait Authorizer: Send + Sync {
    async fn check(
        &self,
        subject: &Authorization,
        resource_type: &str,
        resource_id: &str,
        permission: &str,
    ) -> Result<bool, String>;
    /// Results must correspond one-for-one to input order. No permission cache.
    async fn check_many(
        &self,
        subject: &Authorization,
        checks: &[PermissionCheck<'_>],
    ) -> Result<Vec<bool>, String> {
        let mut decisions = Vec::with_capacity(checks.len());
        for chunk in checks.chunks(32) {
            let mut pending = Vec::with_capacity(chunk.len());
            for c in chunk {
                pending.push(self.check(subject, c.resource_type, c.resource_id, c.permission));
            }
            decisions.extend(futures::future::try_join_all(pending).await?);
        }
        Ok(decisions)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub label: String,
    pub resource_type: String,
    pub read_permission: String,
    pub write_permission: String,
    /// JSON Schema applied before producing and when consuming untrusted records.
    pub properties: Value,
    /// Edge rows refer to node keys using these columns and registered table names.
    pub edge: Option<Edge>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub source_table: String,
    pub source_column: String,
    pub target_table: String,
    pub target_column: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamSchema {
    pub tables: BTreeMap<String, Table>,
}
impl StreamSchema {
    fn validate(&self) -> Result<(), String> {
        let mut labels = std::collections::BTreeSet::new();
        for (name, table) in &self.tables {
            if [
                name,
                &table.label,
                &table.resource_type,
                &table.read_permission,
                &table.write_permission,
            ]
            .iter()
            .any(|s| s.is_empty())
                || !labels.insert((table.edge.is_some(), &table.label))
            {
                return Err("stream tables need nonempty names/policies and unique labels".into());
            }
            jsonschema::validator_for(&table.properties).map_err(|e| e.to_string())?;
            if let Some(edge) = &table.edge {
                for endpoint in [&edge.source_table, &edge.target_table] {
                    if !self.tables.get(endpoint).is_some_and(|t| t.edge.is_none()) {
                        return Err("edge endpoint must reference a registered node table".into());
                    }
                }
            }
        }
        Ok(())
    }
    fn validate_change(
        &self,
        change: &Change,
        validators: &BTreeMap<String, jsonschema::Validator>,
    ) -> Result<&Table, String> {
        let table = self
            .tables
            .get(&change.table)
            .ok_or("unregistered stream table")?;
        if change.key.is_empty() {
            return Err("empty entity key".into());
        }
        if let Some(data) = &change.data {
            validators[&change.table]
                .validate(&Value::Object(data.clone()))
                .map_err(|e| e.to_string())?;
            if let Some(edge) = &table.edge {
                for column in [&edge.source_column, &edge.target_column] {
                    if !data
                        .get(column)
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty())
                    {
                        return Err("edge endpoint keys must be nonempty strings".into());
                    }
                }
            }
        }
        Ok(table)
    }
}
/// Atomically published Iceberg state and Kafka next offsets from the SAME Flink
/// checkpoint. The topic incarnation prevents accidental reuse after recreation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub source: String,
    pub offsets: Offsets,
    pub changes: Vec<Change>,
}
#[async_trait(?Send)]
pub trait SnapshotSource {
    async fn load(&mut self) -> Result<Snapshot, String>;
}
#[derive(Clone, Debug)]
pub struct Limits {
    pub entities: usize,
    pub records: usize,
    pub timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entities: 1_000_000,
            records: 10_000_000,
            timeout: Duration::from_secs(60),
        }
    }
}
/// Register one materialization topic and its immutable incarnation identifier.
/// Use separate connections for independent consumers; no Kafka group commits.
pub struct StreamConnection {
    schema: StreamSchema,
    validators: BTreeMap<String, jsonschema::Validator>,
    source: String,
    log: Arc<dyn ChangeLog>,
    auth: Arc<dyn Authorizer>,
    limits: Limits,
}
impl StreamConnection {
    pub fn new(
        schema: StreamSchema,
        source: String,
        log: Arc<dyn ChangeLog>,
        auth: Arc<dyn Authorizer>,
        limits: Limits,
    ) -> Result<Self, String> {
        schema.validate()?;
        if source.is_empty()
            || limits.entities == 0
            || limits.records == 0
            || limits.timeout.is_zero()
        {
            return Err("source and positive limits required".into());
        }
        let validators = schema
            .tables
            .iter()
            .map(|(name, table)| {
                jsonschema::validator_for(&table.properties)
                    .map(|validator| (name.clone(), validator))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            schema,
            validators,
            source,
            log,
            auth,
            limits,
        })
    }
    fn validate_change(&self, change: &Change) -> Result<&Table, String> {
        self.schema.validate_change(change, &self.validators)
    }
    /// Authorization is mandatory, including for retries and deletes. A receipt
    /// is returned only after broker acknowledgement. A multi-call batch is not atomic.
    pub async fn insert(
        &self,
        subject: &Authorization,
        change: Change,
    ) -> Result<(i32, i64), String> {
        let table = self.validate_change(&change)?;
        if change.data.is_some() {
            properties(&change)?;
        }
        if !self
            .auth
            .check(
                subject,
                &table.resource_type,
                &change.key,
                &table.write_permission,
            )
            .await?
        {
            return Err("insertion permission denied".into());
        }
        self.log.append(&change).await
    }
    pub async fn query(&self, query: Query) -> Result<RecordBatch, String> {
        tokio::time::timeout(self.limits.timeout, self.query_snapshot(query, None))
            .await
            .map_err(|_| "query timed out")?
    }
    pub async fn query_hybrid(
        &self,
        query: Query,
        snapshot: &mut dyn SnapshotSource,
    ) -> Result<RecordBatch, String> {
        tokio::time::timeout(self.limits.timeout, async {
            self.query_snapshot(query, Some(snapshot.load().await?))
                .await
        })
        .await
        .map_err(|_| "hybrid query timed out")?
    }
    async fn query_snapshot(
        &self,
        query: Query,
        snapshot: Option<Snapshot>,
    ) -> Result<RecordBatch, String> {
        let plan = prepare(&query, false)?;
        let subject = query
            .authorization
            .as_ref()
            .ok_or("query subject required")?;
        let bounds = self.log.bounds().await?;
        let mut state = BTreeMap::new();
        let start = if let Some(snapshot) = snapshot {
            if snapshot.changes.len() > self.limits.records {
                return Err("snapshot record limit exceeded".into());
            }
            if snapshot.source != self.source {
                return Err("snapshot source incarnation mismatch".into());
            }
            for change in snapshot.changes {
                self.merge(&mut state, change)?;
            }
            snapshot.offsets
        } else {
            bounds.high.keys().map(|p| (*p, 0)).collect()
        };
        validate_offsets(&start, &bounds)?;
        let mut positions = start.clone();
        if positions != bounds.high {
            let mut stream = self.log.follow(start)?;
            let read = async {
                let mut records = 0;
                while !complete(&positions, &bounds.high) {
                    let item = stream
                        .next()
                        .await
                        .ok_or("log ended before bounded snapshot completed")??;
                    match item {
                        LogItem::Record(record) => {
                            let end = *bounds
                                .high
                                .get(&record.partition)
                                .ok_or("unexpected partition")?;
                            let next = positions
                                .get_mut(&record.partition)
                                .ok_or("unexpected partition")?;
                            if record.offset < *next {
                                continue;
                            }
                            *next = record.offset.checked_add(1).ok_or("offset overflow")?;
                            if record.offset < end {
                                records += 1;
                                if records > self.limits.records {
                                    return Err("snapshot record limit exceeded".into());
                                }
                                self.merge(&mut state, record.change)?;
                            }
                        }
                        LogItem::Progress {
                            partition,
                            next_offset,
                        } => {
                            let position = positions
                                .get_mut(&partition)
                                .ok_or("unexpected partition")?;
                            *position = (*position).max(next_offset);
                        }
                    }
                }
                Ok::<_, String>(())
            };
            tokio::time::timeout(self.limits.timeout, read)
                .await
                .map_err(|_| "snapshot read timed out")??;
        }
        let graph = self.graph(subject, state.values()).await?;
        Ok(
            crate::ir::rel::runtime::execute(&plan, &graph, Some(self.limits.timeout))
                .await?
                .0
                .batch,
        )
    }
    fn merge(
        &self,
        state: &mut BTreeMap<(String, String), Change>,
        change: Change,
    ) -> Result<bool, String> {
        self.validate_change(&change)?;
        let key = (change.table.clone(), change.key.clone());
        if let Some(previous) = state.get(&key) {
            if previous.version > change.version {
                return Ok(false);
            }
            if previous.version == change.version {
                if previous != &change {
                    return Err("conflicting payloads for the same entity version".into());
                }
                return Ok(false);
            }
        } else if state.len() >= self.limits.entities {
            return Err("entity limit exceeded".into());
        }
        state.insert(key, change);
        Ok(true)
    }
    async fn graph<'a>(
        &self,
        subject: &Authorization,
        changes: impl Iterator<Item = &'a Change>,
    ) -> Result<PropertyGraph, String> {
        let mut visible = Vec::new();
        for change in changes {
            let table = self.validate_change(change)?;
            if change.data.is_some()
                && self
                    .auth
                    .check(
                        subject,
                        &table.resource_type,
                        &change.key,
                        &table.read_permission,
                    )
                    .await?
            {
                visible.push((change, table));
            }
        }
        self.visible_graph(visible)
    }
    fn visible_graph(&self, visible: Vec<(&Change, &Table)>) -> Result<PropertyGraph, String> {
        use crate::ir::{EdgeTable, ElementId, NodeTable, Value as GraphValue};
        use arrow::{
            array::{ArrayRef, StringArray},
            datatypes::{DataType, Field, Schema},
            record_batch::RecordBatchOptions,
        };
        let identity = |table: &str, key: &str| -> Result<ElementId, String> {
            ElementId::new(datafusion::common::ScalarValue::Utf8(Some(
                serde_json::to_string(&(table, key)).map_err(|e| e.to_string())?,
            )))
        };
        let mut graph = PropertyGraph::new();
        let mut groups = BTreeMap::<&str, Vec<&Change>>::new();
        for (change, _) in visible {
            groups.entry(&change.table).or_default().push(change);
        }
        let mut nodes = std::collections::BTreeSet::new();
        for (name, changes) in &groups {
            let table = &self.schema.tables[*name];
            if table.edge.is_some() {
                continue;
            }
            let keys = changes
                .iter()
                .map(|c| identity(&c.table, &c.key))
                .collect::<Result<Vec<_>, _>>()?;
            let batch = RecordBatch::try_new_with_options(
                Arc::new(Schema::empty()),
                vec![],
                &RecordBatchOptions::new().with_row_count(Some(keys.len())),
            )
            .map_err(|e| e.to_string())?;
            graph
                .add_keyed_nodes(
                    NodeTable {
                        label: table.label.clone(),
                        batch,
                    },
                    keys.clone(),
                )
                .map_err(|e| e.to_string())?;
            for (change, id) in changes.iter().zip(keys) {
                let node = GraphValue::Node {
                    label: table.label.clone(),
                    id,
                };
                for (name, value) in properties(change)? {
                    graph
                        .set_property(&node, name, value)
                        .map_err(|e| e.to_string())?;
                }
                nodes.insert((change.table.as_str(), change.key.as_str()));
            }
        }
        for (name, changes) in &groups {
            let table = &self.schema.tables[*name];
            let Some(edge) = &table.edge else {
                continue;
            };
            let mut rows = Vec::new();
            for change in changes {
                let data = change.data.as_ref().unwrap();
                let source = data[&edge.source_column].as_str().unwrap();
                let target = data[&edge.target_column].as_str().unwrap();
                if nodes.contains(&(edge.source_table.as_str(), source))
                    && nodes.contains(&(edge.target_table.as_str(), target))
                {
                    rows.push((
                        *change,
                        identity(&edge.source_table, source)?,
                        identity(&edge.target_table, target)?,
                    ));
                }
            }
            if rows.is_empty() {
                continue;
            }
            let keys = rows
                .iter()
                .map(|(c, _, _)| identity(&c.table, &c.key))
                .collect::<Result<Vec<_>, _>>()?;
            let source = self.schema.tables[&edge.source_table].label.clone();
            let target = self.schema.tables[&edge.target_table].label.clone();
            let schema = Arc::new(Schema::new(vec![
                Field::new("__src_id", DataType::Utf8, false),
                Field::new("__dst_id", DataType::Utf8, false),
            ]));
            let sources = rows
                .iter()
                .map(|(_, s, _)| s.scalar().to_string())
                .collect::<Vec<_>>();
            let targets = rows
                .iter()
                .map(|(_, _, t)| t.scalar().to_string())
                .collect::<Vec<_>>();
            let batch = RecordBatch::try_new(
                schema,
                vec![
                    Arc::new(StringArray::from(sources)) as ArrayRef,
                    Arc::new(StringArray::from(targets)),
                ],
            )
            .map_err(|e| e.to_string())?;
            graph
                .add_keyed_edges(
                    EdgeTable {
                        rel_type: table.label.clone(),
                        src_label: source.clone(),
                        dst_label: target.clone(),
                        batch,
                    },
                    keys.clone(),
                )
                .map_err(|e| e.to_string())?;
            for ((change, src_id, dst_id), id) in rows.into_iter().zip(keys) {
                let rel = GraphValue::Edge {
                    rel_type: table.label.clone(),
                    id,
                    src_label: source.clone(),
                    src_id,
                    dst_label: target.clone(),
                    dst_id,
                    projected_properties: None,
                };
                for (name, value) in properties(change)? {
                    graph
                        .set_property(&rel, name, value)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(graph)
    }
    /// Event-local replacement frames, not a continuous joined result table.
    /// Flink must publish computed joins/aggregates as node records. Empty rows
    /// remove a previously matching key; tombstones have no rows. Checkpoint only
    /// after consuming a frame. Reconnects may replay frames (idempotent by key/version).
    pub async fn subscribe(
        &self,
        query: Query,
        offsets: Offsets,
    ) -> Result<Subscription<'_>, String> {
        let plan = prepare(&query, true)?;
        if query.authorization.is_none() {
            return Err("query subject required".into());
        }
        let bounds = self.log.bounds().await?;
        validate_offsets(&offsets, &bounds)?;
        let program = event::EventProgram::new(&plan)?;
        Ok(Subscription {
            program,
            buffered: Default::default(),
            connection: self,
            plan,
            subject: query.authorization.unwrap(),
            stream: self.log.follow(offsets.clone())?,
            offsets,
            state: BTreeMap::new(),
            pending: None,
            failed: false,
        })
    }
}
fn properties(change: &Change) -> Result<BTreeMap<String, crate::ir::Value>, String> {
    change
        .data
        .as_ref()
        .ok_or("delete has no properties")?
        .iter()
        .map(|(k, v)| json_value(v).map(|value| (k.clone(), value)))
        .collect()
}
// The fixture importer converts out-of-range integers to floating point and can
// panic on arbitrary-precision JSON numbers. Production records must preserve
// integer precision and report unsupported magnitudes instead.
fn json_value(value: &Value) -> Result<crate::ir::Value, String> {
    use crate::ir::Value as G;
    Ok(match value {
        Value::Null => G::Null,
        Value::Bool(v) => G::Bool(*v),
        Value::String(v) => G::String(v.clone()),
        Value::Array(v) => G::List(v.iter().map(json_value).collect::<Result<_, _>>()?),
        Value::Object(v) => G::Map(
            v.iter()
                .map(|(k, v)| json_value(v).map(|v| (k.clone(), v)))
                .collect::<Result<_, _>>()?,
        ),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                G::Int(i)
            } else if let Some(i) = v.as_u64() {
                G::UInt64(i)
            } else {
                if !v.as_f64().is_some_and(f64::is_finite) {
                    return Err("JSON number magnitude is unsupported".into());
                }
                let text = v.to_string();
                if text.contains(['.', 'e', 'E']) {
                    G::BigDecimal(text.parse().map_err(|_| "invalid JSON decimal")?)
                } else {
                    G::BigInt(text.parse().map_err(|_| "invalid JSON integer")?)
                }
            }
        }
    })
}

fn complete(positions: &Offsets, end: &Offsets) -> bool {
    end.iter()
        .all(|(p, e)| positions.get(p).is_some_and(|v| v >= e))
}
pub fn validate_offsets(offsets: &Offsets, bounds: &Bounds) -> Result<(), String> {
    if offsets.keys().ne(bounds.high.keys()) || bounds.low.keys().ne(bounds.high.keys()) {
        return Err(
            "checkpoint partition set mismatch; publish a new materialization checkpoint".into(),
        );
    }
    for (p, offset) in offsets {
        if *offset < 0 || *offset < bounds.low[p] || *offset > bounds.high[p] {
            return Err("Kafka retention gap or checkpoint outside log bounds".into());
        }
    }
    Ok(())
}
fn prepare(query: &Query, streaming: bool) -> Result<GraphPlan, String> {
    if query.language != "cypher" {
        return Err("stream connection requires Cypher".into());
    }
    let mut ast = crate::language::cypher::parse_query(&query.text).map_err(|e| e.to_string())?;
    crate::language::cypher::parameters::bind_parameters(
        &mut ast,
        &query
            .parameters
            .iter()
            .map(|(k, v)| json_value(v).map(|v| (k.clone(), v)))
            .collect::<Result<_, String>>()?,
    )?;
    let plan = crate::language::cypher::CypherPlanner::new()
        .plan(&ast)
        .map_err(|e| e.to_string())?;
    crate::ir::analysis::validate_read_capabilities(
        &plan,
        crate::ir::analysis::ReadCapabilities::LOCAL_DUCKDB,
    )
    .map_err(|e| e.to_string())?;
    if streaming {
        validate_stream_plan(&plan.root)?;
    }
    Ok(plan)
}
fn validate_stream_plan(node: &Node) -> Result<(), String> {
    match node {
        Node::GraphReturn { input, .. }
        | Node::GraphProject { input, .. }
        | Node::GraphFilter { input, .. }
        | Node::GraphBind { input, .. } => validate_stream_plan(input),
        Node::GraphNodeScan { .. } => Ok(()),
        Node::GraphApply {
            kind: crate::ir::ApplyKind::Inner,
            left,
            right,
            correlation,
            ..
        } if matches!(left.as_ref(), Node::GraphOneRow) && correlation.is_empty() => {
            validate_stream_plan(right)
        }
        _ => Err("streaming Cypher supports a single node MATCH with filters and projections; materialize joins, aggregation, sorting and limits in Flink".into()),
    }
}
pub struct Frame {
    pub table: String,
    pub key: String,
    pub version: u64,
    pub deleted: bool,
    pub rows: Option<RecordBatch>,
    pub offsets: Offsets,
}
pub struct Subscription<'a> {
    program: event::EventProgram,
    buffered: std::collections::VecDeque<LogItem>,
    connection: &'a StreamConnection,
    plan: GraphPlan,
    subject: Authorization,
    stream: BoxStream<'static, Result<LogItem, String>>,
    offsets: Offsets,
    state: BTreeMap<(String, String), Change>,
    pending: Option<LogRecord>,
    failed: bool,
}
impl Subscription<'_> {
    pub async fn next(&mut self) -> Result<Option<Frame>, String> {
        if self.failed {
            return Err("subscription terminated after an error".into());
        }
        let result = self.next_inner().await;
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    async fn next_inner(&mut self) -> Result<Option<Frame>, String> {
        loop {
            if self.pending.is_none() {
                match match self.buffered.pop_front() {
                    Some(item) => Some(item),
                    None => self.stream.next().await.transpose()?,
                } {
                    None => return Ok(None),
                    Some(LogItem::Progress {
                        partition,
                        next_offset,
                    }) => {
                        let offset = self
                            .offsets
                            .get_mut(&partition)
                            .ok_or("unexpected partition")?;
                        *offset = (*offset).max(next_offset);
                        continue;
                    }
                    Some(LogItem::Record(record)) => self.pending = Some(record),
                }
            }
            // Retain a consumed record across cancellation of this future. No
            // cursor/state changes until authorization and execution complete.
            let record = self.pending.as_ref().unwrap();
            let offset = *self
                .offsets
                .get(&record.partition)
                .ok_or("unexpected partition")?;
            if record.offset < offset {
                self.pending = None;
                continue;
            }
            let change = &record.change;
            let table = self.connection.validate_change(change)?;
            let key = (change.table.clone(), change.key.clone());
            if let Some(previous) = self.state.get(&key) {
                if previous.version == change.version && previous != change {
                    return Err("conflicting payloads for the same entity version".into());
                }
                if previous.version >= change.version {
                    self.offsets.insert(
                        record.partition,
                        record.offset.checked_add(1).ok_or("offset overflow")?,
                    );
                    self.pending = None;
                    continue;
                }
            } else if self.state.len() >= self.connection.limits.entities {
                return Err("entity limit exceeded".into());
            }
            let allowed = self
                .connection
                .auth
                .check(
                    &self.subject,
                    &table.resource_type,
                    &change.key,
                    &table.read_permission,
                )
                .await?;
            let rows = if !allowed || change.data.is_none() {
                None
            } else {
                let graph = self.connection.visible_graph(vec![(change, table)])?;
                Some(
                    self.program
                        .execute(&self.plan, graph, self.connection.limits.timeout)?,
                )
            };
            let record = self.pending.take().unwrap();
            self.offsets.insert(
                record.partition,
                record.offset.checked_add(1).ok_or("offset overflow")?,
            );
            let change = record.change;
            self.state.insert(key, change.clone());
            if allowed {
                return Ok(Some(Frame {
                    table: change.table,
                    key: change.key,
                    version: change.version,
                    deleted: change.data.is_none(),
                    rows,
                    offsets: self.offsets.clone(),
                }));
            }
        }
    }
}
