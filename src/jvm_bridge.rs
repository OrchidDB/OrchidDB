//! Version 1 JSON-lines native storage API for the supported JVM executor.
//!
//! Every store owns its graph and transaction checkpoint. Handles are scoped to
//! this store and must never be transferred between sessions. `commit` persists
//! a lossless native snapshot when a path is configured; close/EOF discard pending
//! writes. Outside an atomic block, a failed mutation restores its statement
//! checkpoint. Savepoints form atomic blocks: they avoid per-mutation copies,
//! and any failure poisons the block until `rollbackTo` (or `close`). Reads never
//! copy the graph. Property-record materialization is a read cache operation.
use crate::ir::catalog::Cardinality;
use crate::ir::value::gremlin_set;
use crate::ir::{PropertyGraph, Value};
use serde_json::{Value as Json, json};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type Result<T> = std::result::Result<T, String>;
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

struct Savepoint {
    id: u64,
    graph: PropertyGraph,
    runtime: BTreeMap<String, Json>,
    failed: bool,
}

/// A single-threaded native graph session. Synchronize callers externally.
pub struct Store {
    graph: PropertyGraph,
    checkpoint: PropertyGraph,
    session: String,
    path: Option<PathBuf>,
    _lock: Option<File>,
    closed: bool,
    runtime: BTreeMap<String, Json>,
    checkpoint_runtime: BTreeMap<String, Json>,
    savepoints: Vec<Savepoint>,
    next_savepoint: u64,
    handle_generations: RefCell<BTreeMap<String, u64>>,
    next_generation: Cell<u64>,
}
impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}
impl Store {
    pub fn new() -> Self {
        Self::from_graph(PropertyGraph::new())
    }
    pub fn from_graph(graph: PropertyGraph) -> Self {
        graph.enable_null_property_values(true);
        Self::from_execution_graph(graph)
    }
    /// Preserve the caller's null policy and uncommitted state for an IR node.
    pub(crate) fn from_execution_graph(graph: PropertyGraph) -> Self {
        // Legacy Arrow/scalar properties acquire native property identities on
        // first access. Allocate those once before the two transaction views can
        // diverge, otherwise unrelated writer allocations could change a reader's
        // property ID when that writer commits. Native records already have IDs.
        for label in graph.labels() {
            let mut keys = graph.node_property_keys(&label);
            keys.sort();
            for id in graph.node_ids(&label).unwrap_or_default() {
                graph.properties(
                    &Value::Node {
                        label: label.clone(),
                        id,
                    },
                    &keys,
                );
            }
        }
        Self {
            checkpoint: graph.clone(),
            graph,
            session: format!(
                "{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
            ),
            path: None,
            _lock: None,
            closed: false,
            runtime: BTreeMap::new(),
            checkpoint_runtime: BTreeMap::new(),
            savepoints: vec![],
            next_savepoint: 1,
            handle_generations: RefCell::new(BTreeMap::new()),
            next_generation: Cell::new(1),
        }
    }
    /// Open an exclusive native snapshot file; concurrent writers are rejected.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let requested = path.as_ref();
        let path = if requested.exists() {
            std::fs::canonicalize(requested).map_err(err)?
        } else {
            let parent = requested
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::canonicalize(parent).map_err(err)?.join(
                requested
                    .file_name()
                    .ok_or("snapshot path must name a file")?,
            )
        };
        let lock_path = path.with_extension(format!(
            "{}lock",
            path.extension()
                .and_then(|x| x.to_str())
                .map(|x| format!("{x}."))
                .unwrap_or_default()
        ));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|e| format!("native store already open or cannot lock: {e}"))?;
        let graph = match std::fs::read(&path) {
            Ok(bytes) => PropertyGraph::snapshot_decode(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PropertyGraph::new(),
            Err(e) => return Err(e.to_string()),
        };
        let mut store = Self::from_graph(graph);
        store.path = Some(path);
        store._lock = Some(lock);
        Ok(store)
    }
    pub(crate) fn validate_execution_finish(&self) -> Result<()> {
        if self.closed || !self.savepoints.is_empty() {
            return Err("JVM IR left an incomplete native atomic block".into());
        }
        if !self.runtime.is_empty() {
            return Err("JVM runtime objects cannot escape an IR invocation through graph properties".into());
        }
        Ok(())
    }
    pub fn graph(&self) -> &PropertyGraph {
        &self.graph
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    /// Report an invalid wire request and poison any active atomic block.
    pub fn protocol_error(&mut self, error: impl Into<String>) -> Json {
        if let Some(block) = self.savepoints.last_mut() {
            block.failed = true;
        }
        json!({"ok":false,"error":error.into()})
    }
    pub fn request(&mut self, request: &Json) -> Json {
        let op = request.get("op").and_then(Json::as_str).unwrap_or("");
        if let Some(committed) = request.get("committed") {
            if committed == &Json::Bool(true) {
                return self.read_committed(request, op);
            }
            if committed != &Json::Bool(false) {
                return json!({"ok":false,"error":"committed must be a boolean"});
            }
        }
        if self.savepoints.last().is_some_and(|s| s.failed) && !matches!(op, "rollbackTo" | "close")
        {
            return json!({"ok":false,"error":"atomic block failed; rollbackTo or close is required"});
        }
        // Control operations validate before modifying state and own the one
        // checkpoint they need. Reads only materialize stable native records.
        // A block's checkpoint subsumes every mutation checkpoint inside it.
        let mutation = matches!(
            op,
            "addVertex" | "addEdge" | "setVertexProperty" | "setProperty" | "remove"
        );
        let before = if mutation && self.savepoints.is_empty() {
            Some(self.graph.clone())
        } else {
            None
        };
        let before_runtime =
            if before.is_some() && matches!(op, "setVertexProperty" | "setProperty") {
                Some(self.runtime.clone())
            } else {
                None
            };
        match self.dispatch(request) {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => {
                if let Some(before) = before {
                    self.graph = before;
                    self.prune_handles();
                }
                if let Some(before_runtime) = before_runtime {
                    self.runtime = before_runtime;
                }
                self.protocol_error(error)
            }
        }
    }
    /// Read the last native commit without exposing or disturbing pending writes.
    /// The process still serializes individual requests; swapping the two native
    /// views avoids cloning the graph for every read. Materialized property records
    /// remain in their own view. A foreign read failure cannot poison a writer's
    /// savepoint, and committed reads remain available while that block is failed.
    fn read_committed(&mut self, request: &Json, op: &str) -> Json {
        if !matches!(op, "vertices" | "edges" | "adjacent" | "properties") {
            return json!({"ok":false,"error":"committed view only supports read operations"});
        }
        std::mem::swap(&mut self.graph, &mut self.checkpoint);
        std::mem::swap(&mut self.runtime, &mut self.checkpoint_runtime);
        let result = self.dispatch(request);
        std::mem::swap(&mut self.graph, &mut self.checkpoint);
        std::mem::swap(&mut self.runtime, &mut self.checkpoint_runtime);
        match result {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => json!({"ok":false,"error":error}),
        }
    }
    fn dispatch(&mut self, r: &Json) -> Result<Json> {
        if self.closed {
            return Err("native store is closed".into());
        }
        if let Some(v) = r.get("version") {
            if v.as_u64() != Some(1) {
                return Err("unsupported protocol version".into());
            }
        }
        let op = string(r, "op")?;
        if matches!(op, "commit" | "rollback") && !self.savepoints.is_empty() {
            return Err(
                "release or rollbackTo the active atomic block before completing the transaction"
                    .into(),
            );
        }
        match op {
            "hello" => Ok(
                json!({"version":1,"session":self.session,"storage":"orchiddb-native","persistent":self.path.is_some(),"committedReads":true,"nullPropertyValues":self.graph.supports_null_property_values()}),
            ),
            "begin" => Ok(Json::Null), // read/write transaction begins at open or the last commit/rollback
            "commit" => {
                self.persist()?;
                self.checkpoint = self.graph.clone();
                self.checkpoint_runtime = self.runtime.clone();
                self.prune_handles();
                self.savepoints.clear();
                Ok(Json::Null)
            }
            "rollback" => {
                self.graph = self.checkpoint.clone();
                self.prune_handles();
                self.runtime = self.checkpoint_runtime.clone();
                self.savepoints.clear();
                Ok(Json::Null)
            }
            "savepoint" => {
                let id = self.next_savepoint;
                self.next_savepoint += 1;
                self.savepoints.push(Savepoint {
                    id,
                    graph: self.graph.clone(),
                    runtime: self.runtime.clone(),
                    failed: false,
                });
                Ok(json!(id))
            }
            "rollbackTo" | "release" => {
                let id = field(r, "id")?.as_u64().ok_or("invalid savepoint id")?;
                if self.savepoints.last().map(|s| s.id) != Some(id) {
                    return Err("savepoints must be completed in LIFO order".into());
                }
                let Savepoint { graph, runtime, .. } = self.savepoints.pop().unwrap();
                if op == "rollbackTo" {
                    self.graph = graph;
                    self.prune_handles();
                    self.runtime = runtime;
                }
                Ok(Json::Null)
            }
            "close" => {
                self.graph = std::mem::take(&mut self.checkpoint);
                self.runtime = std::mem::take(&mut self.checkpoint_runtime);
                self.savepoints.clear();
                self.handle_generations.borrow_mut().clear();
                self.closed = true;
                self._lock = None;
                Ok(Json::Null)
            }
            "vertices" | "edges" => {
                let edge = op == "edges";
                let ids = optional_array(r, "ids")?;
                let values = if ids.is_empty() {
                    self.elements(edge)?
                } else {
                    ids.iter()
                        .map(|id| self.decode(id))
                        .collect::<Result<Vec<_>>>()?
                        .iter()
                        .filter_map(|id| self.lookup_id(id, edge))
                        .collect()
                };
                self.encode_many(values)
            }
            "addVertex" => {
                let label = r
                    .get("label")
                    .map(|_| string(r, "label"))
                    .transpose()?
                    .unwrap_or("vertex");
                validate_name(label)?;
                let properties = self.pairs(r, "properties")?;
                let id = r.get("id").map(|id| self.decode_id(id)).transpose()?;
                let vertex = self.graph.insert_node(label, BTreeMap::new());
                if let Some(id) = id {
                    self.graph.set_element_public_id(&vertex, id).map_err(err)?;
                } else {
                    self.graph
                        .assign_generated_public_id(&vertex)
                        .map_err(err)?;
                }
                for (key, value) in properties {
                    self.graph
                        .set_jvm_vertex_property(
                            &vertex,
                            &key,
                            value,
                            Cardinality::Single,
                            BTreeMap::new(),
                        )
                        .map_err(err)?;
                }
                self.encode(&vertex)
            }
            "addEdge" => {
                let out = self.resolve(field(r, "out")?)?;
                let input = self.resolve(field(r, "in")?)?;
                let label = string(r, "label")?;
                validate_name(label)?;
                let properties = self.pairs(r, "properties")?;
                let id = r.get("id").map(|id| self.decode_id(id)).transpose()?;
                let edge = self
                    .graph
                    .insert_edge(label, &out, &input, BTreeMap::new())
                    .map_err(err)?;
                if let Some(id) = id {
                    self.graph.set_element_public_id(&edge, id).map_err(err)?;
                } else {
                    self.graph.assign_generated_public_id(&edge).map_err(err)?;
                }
                for (key, value) in properties {
                    self.graph
                        .set_gremlin_property(&edge, &key, value)
                        .map_err(err)?;
                }
                self.encode(&edge)
            }
            "properties" => {
                let owner = self.resolve(field(r, "owner")?)?;
                let keys = strings(r, "keys")?;
                self.encode_many(self.graph.jvm_properties(&owner, &keys))
            }
            "setVertexProperty" => {
                let owner = self.resolve(field(r, "owner")?)?;
                let key = string(r, "key")?;
                validate_name(key)?;
                let wire = field(r, "value")?;
                let runtime = self.runtime_value(wire)?;
                let value = if runtime.is_some() {
                    Value::Null
                } else {
                    self.decode(wire)?
                };
                let cardinality = match string(r, "cardinality")? {
                    "single" => Cardinality::Single,
                    "list" => Cardinality::List,
                    "set" => Cardinality::Set,
                    _ => return Err("invalid cardinality".into()),
                };
                let meta = self.pairs(r, "meta")?.into_iter().collect();
                let id = r.get("id").map(|id| self.decode_id(id)).transpose()?;
                let p = self
                    .graph
                    .set_jvm_vertex_property(&owner, key, value, cardinality, meta)
                    .map_err(err)?;
                if let Some(id) = id {
                    self.graph
                        .set_vertex_property_public_id(&p, id)
                        .map_err(err)?;
                }
                let address = self.handle(&p)?.to_string();
                if let Some(runtime) = runtime {
                    self.runtime.insert(address, runtime);
                } else {
                    self.runtime.remove(&address);
                }
                self.encode(&p)
            }
            "setProperty" => {
                let owner = self.resolve(field(r, "owner")?)?;
                let key = string(r, "key")?;
                validate_name(key)?;
                if !matches!(owner, Value::Edge { .. } | Value::VertexProperty { .. }) {
                    return Err("setProperty requires edge or vertex property owner".into());
                }
                let wire = field(r, "value")?;
                let runtime = self.runtime_value(wire)?;
                let value = if runtime.is_some() {
                    Value::Null
                } else {
                    self.decode(wire)?
                };
                self.graph
                    .set_gremlin_property(&owner, key, value)
                    .map_err(err)?;
                let p = self
                    .graph
                    .jvm_properties(&owner, &[key.into()])
                    .into_iter()
                    .next()
                    .ok_or("property was not stored")?;
                let address = self.handle(&p)?.to_string();
                if let Some(runtime) = runtime {
                    self.runtime.insert(address, runtime);
                } else {
                    self.runtime.remove(&address);
                }
                self.encode(&p)
            }
            "remove" => {
                let owner = self.resolve(field(r, "owner")?)?;
                self.graph.delete_value(&owner, true).map_err(err)?;
                Ok(Json::Null)
            }
            "adjacent" => {
                let vertex = self.resolve(field(r, "vertex")?)?;
                let Value::Node { label, id } = vertex else {
                    return Err("adjacent requires vertex".into());
                };
                let labels = strings(r, "labels")?;
                let direction = string(r, "direction")?;
                if !["OUT", "IN", "BOTH"].contains(&direction) {
                    return Err("invalid direction".into());
                }
                let mut edges = vec![];
                if direction != "IN" {
                    edges.extend(self.graph.out_edges(&label, id, &labels));
                }
                if direction != "OUT" {
                    edges.extend(self.graph.in_edges(&label, id, &labels));
                }
                self.encode_many(
                    edges
                        .into_iter()
                        .map(|(label, row, _, _)| self.edge(&label, row))
                        .collect::<Result<Vec<_>>>()?,
                )
            }
            _ => Err(format!("unsupported native operation: {op}")),
        }
    }
    fn persist(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let bytes = self.graph.snapshot_encode()?;
        let temporary = path.with_extension(format!("{}-commit", self.session));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(err)?;
            file.write_all(&bytes).map_err(err)?;
            file.sync_all().map_err(err)?;
            std::fs::rename(&temporary, path).map_err(err)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
    fn lookup_id(&self, id: &Value, edge: bool) -> Option<Value> {
        self.graph.find_element_by_public_id(id, edge).or_else(|| {
            // Numeric string conversion is only a fallback. An actual string
            // ID must win even when a numerically equivalent ID also exists.
            let Value::String(text) = id else { return None };
            let number = text.parse::<bigdecimal::BigDecimal>().ok()?;
            self.graph
                .find_element_by_public_id(&Value::BigDecimal(number), edge)
        })
    }
    fn elements(&self, edge: bool) -> Result<Vec<Value>> {
        let mut result = vec![];
        if edge {
            for label in self.graph.rel_types() {
                for row in self.graph.edge_ids(&label) {
                    result.push(self.edge(&label, row)?);
                }
            }
        } else {
            for label in self.graph.labels() {
                for row in self.graph.node_ids(&label).map_err(err)? {
                    result.push(Value::Node {
                        label: label.clone(),
                        id: row,
                    });
                }
            }
        }
        Ok(result)
    }
    fn edge(&self, label: &str, row: i64) -> Result<Value> {
        let (src_label, src_id, dst_label, dst_id) = self
            .graph
            .live_edge_endpoints(label, row)
            .ok_or("edge does not exist")?;
        Ok(Value::Edge {
            rel_type: label.into(),
            id: row,
            src_label,
            src_id,
            dst_label,
            dst_id,
            projected_properties: None,
        })
    }
    fn handle(&self, value: &Value) -> Result<Json> {
        let mut h = match value {
            Value::Node { label, id } => json!({"kind":"vertex","label":label,"row":id}),
            Value::Edge { rel_type, id, .. } => json!({"kind":"edge","label":rel_type,"row":id}),
            Value::VertexProperty { id, owner, key, .. } => {
                json!({"kind":"vertex_property","owner":self.handle(owner)?,"key":key,"row":id})
            }
            Value::Property { owner, key, .. } => {
                json!({"kind":"property","owner":self.handle(owner)?,"key":key})
            }
            _ => return Err("expected native element".into()),
        };
        h["session"] = json!(self.session);
        let key = h.to_string();
        let generation = *self
            .handle_generations
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let generation = self.next_generation.get();
                self.next_generation.set(generation + 1);
                generation
            });
        h["generation"] = json!(generation);
        Ok(h)
    }
    fn prune_handles(&mut self) {
        let candidates = self
            .handle_generations
            .borrow()
            .iter()
            .filter_map(|(key, generation)| {
                let mut h: Json = serde_json::from_str(key).expect("internal handle JSON");
                h["generation"] = json!(generation);
                self.resolve(&h).is_err().then(|| key.clone())
            })
            .collect::<Vec<_>>();
        // A writer may have removed an element that still exists for committed
        // readers. Keep its identity until it is absent from both native views.
        std::mem::swap(&mut self.graph, &mut self.checkpoint);
        let invalid = candidates
            .into_iter()
            .filter(|key| {
                let mut handle: Json = serde_json::from_str(key).expect("internal handle JSON");
                handle["generation"] = json!(self.handle_generations.borrow()[key]);
                self.resolve(&handle).is_err()
            })
            .collect::<Vec<_>>();
        std::mem::swap(&mut self.graph, &mut self.checkpoint);
        for key in invalid {
            self.handle_generations.borrow_mut().remove(&key);
        }
    }
    fn resolve(&self, h: &Json) -> Result<Value> {
        let generation = field(h, "generation")?
            .as_u64()
            .ok_or("missing handle generation")?;
        let mut address = h.clone();
        address
            .as_object_mut()
            .ok_or("invalid handle")?
            .remove("generation");
        if self
            .handle_generations
            .borrow()
            .get(&address.to_string())
            .copied()
            != Some(generation)
        {
            return Err("expired native handle".into());
        }
        if string(h, "session")? != self.session {
            return Err("handle belongs to another native session".into());
        }
        match string(h, "kind")? {
            "vertex" => {
                let label = string(h, "label")?;
                let row = integer(field(h, "row")?)?;
                if !self.graph.node_is_live(label, row) {
                    return Err("vertex does not exist".into());
                }
                Ok(Value::Node {
                    label: label.into(),
                    id: row,
                })
            }
            "edge" => self.edge(string(h, "label")?, integer(field(h, "row")?)?),
            kind @ ("vertex_property" | "property") => {
                let owner = self.resolve(field(h, "owner")?)?;
                let key = string(h, "key")?;
                let row = if kind == "vertex_property" {
                    Some(integer(field(h, "row")?)?)
                } else {
                    None
                };
                self.graph
                    .jvm_properties(&owner, &[key.into()])
                    .into_iter()
                    .find(|v| match (v, row) {
                        (Value::VertexProperty { id, .. }, Some(row)) => *id == row,
                        (Value::Property { .. }, None) => true,
                        _ => false,
                    })
                    .ok_or_else(|| "property does not exist".into())
            }
            _ => Err("unknown native handle kind".into()),
        }
    }
    fn encode_many(&self, values: Vec<Value>) -> Result<Json> {
        Ok(Json::Array(
            values
                .iter()
                .map(|v| self.encode(v))
                .collect::<Result<Vec<_>>>()?,
        ))
    }
    pub fn encode(&self, v: &Value) -> Result<Json> {
        let tagged = |kind: &str, value: Json| json!({"type":kind,"value":value});
        Ok(match v {
            Value::Null => json!({"type":"null"}),
            Value::Bool(v) => tagged("boolean", json!(v)),
            Value::String(v) => tagged("string", json!(v)),
            Value::Byte(v) => tagged("byte", json!(v)),
            Value::Short(v) => tagged("short", json!(v)),
            Value::Int(v) => tagged("int", json!(v)),
            Value::Long(v) => tagged("long", json!(v.to_string())),
            Value::Float32(v) => tagged("float", json!(float_string(*v as f64, v.to_string()))),
            Value::Float(v) => tagged("double", json!(float_string(*v, v.to_string()))),
            Value::BigInt(v) => tagged("bigint", json!(v.to_string())),
            Value::BigDecimal(v) => tagged("bigdecimal", json!(v.to_string())),
            Value::List(v) => tagged("list", self.encode_many(v.clone())?),
            Value::Path(v) => tagged("path", self.encode_many(v.clone())?),
            Value::MapEntry(pair) => tagged("entry", json!([self.encode(&pair.0)?,self.encode(&pair.1)?])),
            Value::Set(v) => tagged("set", self.encode_many(v.clone())?),
            Value::Map(v) => tagged(
                "map",
                Json::Array(
                    v.iter()
                        .map(|(k, v)| {
                            Ok(json!([
                                self.encode(&Value::String(k.clone()))?,
                                self.encode(v)?
                            ]))
                        })
                        .collect::<Result<_>>()?,
                ),
            ),
            Value::TypedMap(v) => tagged(
                "map",
                Json::Array(
                    v.iter()
                        .map(|(k, v)| Ok(json!([self.encode(k)?, self.encode(v)?])))
                        .collect::<Result<_>>()?,
                ),
            ),
            Value::Node { label, .. } => {
                json!({"type":"vertex","handle":self.handle(v)?,"label":label,"id":self.encode(&self.graph.element_public_id(v))?})
            }
            Value::Edge {
                rel_type,
                src_label,
                src_id,
                dst_label,
                dst_id,
                ..
            } => {
                json!({"type":"edge","handle":self.handle(v)?,"label":rel_type,"id":self.encode(&self.graph.element_public_id(v))?,"out":self.encode(&Value::Node{label:src_label.clone(),id:*src_id})?,"in":self.encode(&Value::Node{label:dst_label.clone(),id:*dst_id})?})
            }
            Value::VertexProperty {
                owner, key, value, ..
            } => {
                json!({"type":"vertex_property","handle":self.handle(v)?,"label":key,"key":key,"value":self.runtime.get(&self.handle(v)?.to_string()).cloned().map(Ok).unwrap_or_else(||self.encode(value))?,"owner":self.encode(owner)?,"id":self.encode(&self.graph.element_public_id(v))?})
            }
            Value::Property { owner, key, value } => {
                json!({"type":"property","handle":self.handle(v)?,"key":key,"value":self.runtime.get(&self.handle(v)?.to_string()).cloned().map(Ok).unwrap_or_else(||self.encode(value))?,"owner":self.encode(owner)?})
            }
            _ => return Err("unsupported native value type in JVM protocol".into()),
        })
    }
    pub fn decode(&self, j: &Json) -> Result<Value> {
        let kind = string(j, "type")?;
        if kind == "vertex_ref" || kind == "edge_ref" {
            let id = self.decode_id(field(j, "id")?)?;
            return self
                .graph
                .find_element_by_public_id(&id, kind == "edge_ref")
                .ok_or_else(|| "referenced element does not exist".into());
        }
        if kind == "vertex_property_ref" {
            let owner = self.decode(field(j, "owner")?)?;
            if !matches!(owner, Value::Node { .. }) {
                return Err("vertex property reference requires vertex owner".into());
            }
            let id = self.decode_id(field(j, "id")?)?;
            return self
                .graph
                .jvm_properties(&owner, &[])
                .into_iter()
                .find(|p| self.graph.element_public_id(p).three_valued_eq(&id) == Some(true))
                .ok_or_else(|| "referenced vertex property does not exist".into());
        }
        if kind == "null" {
            return Ok(Value::Null);
        }
        if ["vertex", "edge", "vertex_property", "property"].contains(&kind) {
            return self.resolve(field(j, "handle")?);
        }
        let v = field(j, "value")?;
        let number = || -> Result<String> {
            match v {
                Json::String(s) => Ok(s.clone()),
                Json::Number(n) => Ok(n.to_string()),
                _ => Err("expected numeric value".into()),
            }
        };
        Ok(match kind {
            "boolean" => Value::Bool(v.as_bool().ok_or("expected boolean")?),
            "string" => Value::String(v.as_str().ok_or("expected string")?.into()),
            "byte" => Value::Byte(number()?.parse().map_err(err)?),
            "short" => Value::Short(number()?.parse().map_err(err)?),
            "int" => Value::Int(number()?.parse::<i32>().map_err(err)? as i64),
            "long" => Value::Long(number()?.parse().map_err(err)?),
            "float" => Value::Float32(number()?.parse().map_err(err)?),
            "double" => Value::Float(number()?.parse().map_err(err)?),
            "bigint" => Value::BigInt(number()?.parse().map_err(err)?),
            "bigdecimal" => Value::BigDecimal(number()?.parse().map_err(err)?),
            "list" | "set" | "path" => {
                let values = v
                    .as_array()
                    .ok_or("expected typed array")?
                    .iter()
                    .map(|x| self.decode(x))
                    .collect::<Result<_>>()?;
                if kind == "path" { Value::Path(values) }
                else if kind == "set" {
                    gremlin_set(values)
                } else {
                    Value::List(values)
                }
            }
            "entry" => {
                let pair=v.as_array().filter(|p|p.len()==2).ok_or("expected map entry pair")?;
                Value::MapEntry(Box::new((self.decode(&pair[0])?,self.decode(&pair[1])?)))
            },
            "map" => Value::TypedMap(
                v.as_array()
                    .ok_or("expected map pairs")?
                    .iter()
                    .map(|x| {
                        let pair = x
                            .as_array()
                            .filter(|x| x.len() == 2)
                            .ok_or("expected map pair")?;
                        Ok((self.decode(&pair[0])?, self.decode(&pair[1])?))
                    })
                    .collect::<Result<_>>()?,
            ),
            _ => return Err(format!("unsupported JVM value type: {kind}")),
        })
    }
    fn runtime_value(&self, j: &Json) -> Result<Option<Json>> {
        if j.get("type").and_then(Json::as_str) != Some("jvm_runtime") {
            return Ok(None);
        }
        if self.path.is_some() {
            return Err("JVM runtime objects cannot be persisted".into());
        }
        if string(j, "session")?.is_empty() || field(j, "id")?.as_u64().is_none() {
            return Err("invalid JVM runtime identity".into());
        }
        Ok(Some(j.clone()))
    }
    fn decode_id(&self, j: &Json) -> Result<Value> {
        let v = self.decode(j)?;
        let valid = match &v {
            Value::String(_)
            | Value::Byte(_)
            | Value::Short(_)
            | Value::Int(_)
            | Value::Long(_)
            | Value::BigInt(_)
            | Value::BigDecimal(_) => true,
            Value::Float(v) => v.is_finite(),
            Value::Float32(v) => v.is_finite(),
            _ => false,
        };
        if !valid {
            return Err("IDs must be strings or finite numbers".into());
        }
        Ok(v)
    }
    fn pairs(&self, r: &Json, name: &str) -> Result<Vec<(String, Value)>> {
        optional_array(r, name)?
            .iter()
            .map(|p| {
                let pair = p
                    .as_array()
                    .filter(|p| p.len() == 2)
                    .ok_or("expected [key,typedValue] property pair")?;
                let key = pair[0].as_str().ok_or("property key must be a string")?;
                validate_name(key)?;
                Ok((key.into(), self.decode(&pair[1])?))
            })
            .collect()
    }
}
fn float_string(value: f64, finite: String) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        finite
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn field<'a>(j: &'a Json, key: &str) -> Result<&'a Json> {
    j.get(key).ok_or_else(|| format!("missing field {key}"))
}
fn string<'a>(j: &'a Json, key: &str) -> Result<&'a str> {
    field(j, key)?
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))
}
fn integer(j: &Json) -> Result<i64> {
    j.as_i64()
        .ok_or_else(|| "expected integer handle row".into())
}
fn optional_array<'a>(j: &'a Json, key: &str) -> Result<&'a [Json]> {
    match j.get(key) {
        None => Ok(&[]),
        Some(v) => v
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("{key} must be an array")),
    }
}
fn strings(j: &Json, key: &str) -> Result<Vec<String>> {
    optional_array(j, key)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must contain strings"))
        })
        .collect()
}
fn validate_name(s: &str) -> Result<()> {
    if s.is_empty() || s.starts_with('~') {
        Err("empty or hidden name".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(s: &mut Store, r: Json) -> Json {
        let out = s.request(&r);
        assert_eq!(out["ok"], true, "{out}");
        out["value"].clone()
    }
    fn vertex(s: &mut Store, id: &str) -> Json {
        call(
            s,
            json!({"op":"addVertex","label":"person","id":{"type":"string","value":id}}),
        )
    }
    fn property(s: &mut Store, v: &Json, key: &str, value: Json) -> Json {
        call(
            s,
            json!({"op":"setVertexProperty","owner":v["handle"],"key":key,"cardinality":"list","value":value}),
        )
    }
    #[test]
    fn typed_values_are_lossless_and_invalid_inputs_rejected() {
        let s = Store::new();
        for input in [
            json!({"type":"long","value":"9223372036854775807"}),
            json!({"type":"bigint","value":"9999999999999999999999999999999"}),
            json!({"type":"bigdecimal","value":"1.2300"}),
            json!({"type":"byte","value":-128}),
            json!({"type":"short","value":32767}),
            json!({"type":"int","value":42}),
            json!({"type":"set","value":[]}),
            json!({"type":"null"}),
            json!({"type":"double","value":"-0"}),
            json!({"type":"float","value":"NaN"}),
        ] {
            assert_eq!(s.encode(&s.decode(&input).unwrap()).unwrap(), input);
        }
        for input in [
            json!({"type":"byte","value":128}),
            json!({"type":"int","value":2147483648i64}),
            json!({"type":"uuid","value":"x"}),
            json!({"type":"boolean","value":"true"}),
            json!({"type":"map","value":[[1]]}),
        ] {
            assert!(s.decode(&input).is_err());
        }
        let nested = json!({"type":"map","value":[[{"type":"int","value":3},{"type":"set","value":[{"type":"long","value":"3"},{"type":"int","value":3}]}]]});
        assert_eq!(s.encode(&s.decode(&nested).unwrap()).unwrap(), nested);
    }
    #[test]
    fn legacy_property_identity_is_stable_across_committed_and_writer_reads() {
        use crate::ir::catalog::nodes_from_columns;
        use arrow::array::{ArrayRef, Int64Array};
        use std::sync::Arc;
        let mut graph = PropertyGraph::new();
        graph.add_nodes(nodes_from_columns(
            "person",
            vec![("age", Arc::new(Int64Array::from(vec![7, 8])) as ArrayRef)],
        ));
        let mut s = Store::from_graph(graph);
        let vertices = call(&mut s, json!({"op":"vertices","committed":true}));
        // Allocate an unrelated writer property before the first reader property
        // access. Both native views must already agree on legacy property IDs.
        property(
            &mut s,
            &vertices[0],
            "writer",
            json!({"type":"int","value":9}),
        );
        let committed = call(&mut s, json!({"op":"properties","committed":true,"owner":vertices[1]["handle"],"keys":["age"]}))[0].clone();
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","owner":vertices[1]["handle"],"keys":["age"]})
            ),
            json!([committed])
        );
        call(&mut s, json!({"op":"commit"}));
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","committed":true,"owner":vertices[1]["handle"],"keys":["age"]})
            ),
            json!([committed])
        );
        property(
            &mut s,
            &vertices[1],
            "temporary",
            json!({"type":"int","value":10}),
        );
        call(&mut s, json!({"op":"rollback"}));
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","committed":true,"owner":vertices[1]["handle"],"keys":["age"]})
            ),
            json!([committed])
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","owner":committed["handle"]})
            ),
            json!([])
        );
    }

    #[test]
    fn committed_reads_hide_pending_topology_properties_and_runtime_values() {
        let mut s = Store::new();
        assert_eq!(call(&mut s, json!({"op":"hello"}))["committedReads"], true);
        let a = vertex(&mut s, "a");
        assert_eq!(
            call(&mut s, json!({"op":"vertices","committed":true})),
            json!([])
        );
        let runtime = json!({"type":"jvm_runtime","session":"family","id":1});
        let p = property(&mut s, &a, "compute", runtime.clone());
        call(&mut s, json!({"op":"commit"}));
        let b = vertex(&mut s, "b");
        let edge = call(
            &mut s,
            json!({"op":"addEdge","out":a["handle"],"in":b["handle"],"label":"knows"}),
        );
        property(
            &mut s,
            &a,
            "compute",
            json!({"type":"jvm_runtime","session":"family","id":2}),
        );
        assert_eq!(
            call(&mut s, json!({"op":"vertices","committed":true})),
            json!([a])
        );
        assert_eq!(
            call(&mut s, json!({"op":"edges","committed":true})),
            json!([])
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"adjacent","committed":true,"vertex":a["handle"],"direction":"OUT"})
            ),
            json!([])
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","committed":true,"owner":a["handle"]})
            ),
            json!([p])
        );
        assert_eq!(
            call(&mut s, json!({"op":"properties","owner":a["handle"]}))
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(call(&mut s, json!({"op":"edges"})), json!([edge]));
        call(&mut s, json!({"op":"commit"}));
        assert_eq!(
            call(&mut s, json!({"op":"vertices","committed":true}))
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            call(&mut s, json!({"op":"edges","committed":true})),
            json!([edge])
        );
        let values = call(
            &mut s,
            json!({"op":"properties","committed":true,"owner":a["handle"]}),
        );
        assert_eq!(values[0]["value"], runtime);
        assert_eq!(values[1]["value"]["id"], 2);
        call(&mut s, json!({"op":"close"}));
        assert_eq!(
            s.request(&json!({"op":"vertices","committed":true}))["ok"],
            false
        );
    }

    #[test]
    fn committed_handles_survive_pending_deletion_and_failed_writer_statements() {
        let mut s = Store::new();
        let a = vertex(&mut s, "a");
        let p = property(
            &mut s,
            &a,
            "name",
            json!({"type":"string","value":"before"}),
        );
        call(
            &mut s,
            json!({"op":"setProperty","owner":p["handle"],"key":"source","value":{"type":"string","value":"committed"}}),
        );
        let edge = call(
            &mut s,
            json!({"op":"addEdge","out":a["handle"],"in":a["handle"],"label":"self","properties":[["weight",{"type":"int","value":7}]]}),
        );
        call(&mut s, json!({"op":"commit"}));
        let reader = call(&mut s, json!({"op":"vertices","committed":true}))[0].clone();
        call(&mut s, json!({"op":"remove","owner":a["handle"]}));
        let replacement = vertex(&mut s, "a");
        // An ordinary failed statement restores its writer snapshot and prunes
        // handles. The committed reader must still retain its older identities.
        assert_eq!(
            s.request(&json!({"op":"addVertex","id":{"type":"string","value":"a"}}))["ok"],
            false
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","committed":true,"owner":reader["handle"]})
            )[0]["value"]["value"],
            "before"
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","committed":true,"owner":p["handle"]})
            )[0]["value"]["value"],
            "committed"
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","committed":true,"owner":edge["handle"]})
            )[0]["value"]["value"],
            7
        );
        assert_eq!(call(&mut s, json!({"op":"vertices"})), json!([replacement]));
        call(&mut s, json!({"op":"rollback"}));
        assert_eq!(
            call(&mut s, json!({"op":"properties","owner":reader["handle"]}))[0]["value"]["value"],
            "before"
        );
        assert_eq!(
            s.request(&json!({"op":"properties","owner":replacement["handle"]}))["ok"],
            false
        );
        call(&mut s, json!({"op":"remove","owner":a["handle"]}));
        call(&mut s, json!({"op":"commit"}));
        let new_a = vertex(&mut s, "a");
        call(&mut s, json!({"op":"commit"}));
        assert_ne!(new_a["handle"], reader["handle"]);
        assert_eq!(
            s.request(&json!({"op":"properties","committed":true,"owner":reader["handle"]}))["ok"],
            false
        );
        assert_eq!(
            call(&mut s, json!({"op":"vertices","committed":true})),
            json!([new_a])
        );
    }

    #[test]
    fn committed_read_errors_and_failed_writer_blocks_are_independent() {
        let mut s = Store::new();
        let a = vertex(&mut s, "a");
        call(&mut s, json!({"op":"commit"}));
        let savepoint = call(&mut s, json!({"op":"savepoint"}));
        assert_eq!(
            s.request(&json!({"op":"remove","committed":true,"owner":a["handle"]}))["ok"],
            false
        );
        assert_eq!(
            s.request(&json!({"op":"properties","committed":true,"owner":{}}))["ok"],
            false
        );
        assert_eq!(
            s.request(&json!({"op":"vertices","committed":"true"}))["ok"],
            false
        );
        let b = vertex(&mut s, "b");
        assert_eq!(
            s.request(&json!({"op":"properties","committed":true,"owner":b["handle"]}))["ok"],
            false
        );
        assert_eq!(
            call(&mut s, json!({"op":"vertices"}))
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            s.request(&json!({"op":"addVertex","id":{"type":"string","value":"a"}}))["ok"],
            false
        );
        assert_eq!(s.request(&json!({"op":"vertices"}))["ok"], false);
        assert_eq!(
            call(&mut s, json!({"op":"vertices","committed":true})),
            json!([a])
        );
        call(&mut s, json!({"op":"rollbackTo","id":savepoint}));
        assert_eq!(call(&mut s, json!({"op":"vertices"})), json!([a]));
        assert_eq!(
            call(&mut s, json!({"op":"vertices","committed":true})),
            json!([a])
        );
    }

    #[test]
    fn reads_writes_identity_null_and_statement_atomicity() {
        let mut s = Store::new();
        let a = vertex(&mut s, "a");
        let b = vertex(&mut s, "b");
        let vp = property(&mut s, &a, "nullable", json!({"type":"null"}));
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","owner":a["handle"],"keys":["nullable"]})
            )[0],
            vp
        );
        let meta = call(
            &mut s,
            json!({"op":"setProperty","owner":vp["handle"],"key":"meta","value":{"type":"null"}}),
        );
        assert_eq!(meta["value"]["type"], "null");
        let edge = call(
            &mut s,
            json!({"op":"addEdge","out":a["handle"],"in":b["handle"],"label":"knows","properties":[["weight",{"type":"double","value":"0.5"}]]}),
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"adjacent","vertex":a["handle"],"direction":"OUT"})
            ),
            json!([edge])
        );
        let duplicate = s.request(
            &json!({"op":"addVertex","label":"person","id":{"type":"string","value":"a"}}),
        );
        assert_eq!(duplicate["ok"], false);
        assert_eq!(
            call(&mut s, json!({"op":"vertices"}))
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let mut another = Store::new();
        assert_eq!(
            another.request(&json!({"op":"remove","owner":a["handle"]}))["ok"],
            false
        );
        call(&mut s, json!({"op":"remove","owner":a["handle"]}));
        assert_eq!(call(&mut s, json!({"op":"edges"})), json!([]));
        assert_eq!(
            s.request(&json!({"version":2,"op":"vertices"}))["ok"],
            false
        );
    }
    #[test]
    fn rollback_savepoints_runtime_values_and_handle_reuse() {
        let mut s = Store::new();
        let a = vertex(&mut s, "a");
        call(&mut s, json!({"op":"commit"}));
        let runtime = json!({"type":"jvm_runtime","session":"java-family","id":1});
        let p = property(&mut s, &a, "compute", runtime.clone());
        assert_eq!(p["value"], runtime);
        let savepoint = call(&mut s, json!({"op":"savepoint"}));
        let b = vertex(&mut s, "b");
        call(&mut s, json!({"op":"rollbackTo","id":savepoint}));
        let c = vertex(&mut s, "c");
        assert_ne!(b["handle"], c["handle"]);
        assert_eq!(
            s.request(&json!({"op":"remove","owner":b["handle"]}))["ok"],
            false
        );
        assert_eq!(
            call(&mut s, json!({"op":"properties","owner":a["handle"]}))[0]["value"],
            runtime
        );
        call(&mut s, json!({"op":"rollback"}));
        assert_eq!(
            call(&mut s, json!({"op":"properties","owner":a["handle"]})),
            json!([])
        );
        call(&mut s, json!({"op":"close"}));
        assert_eq!(s.request(&json!({"op":"vertices"}))["ok"], false);
    }
    #[test]
    fn failed_atomic_blocks_cannot_expose_or_commit_partial_writes() {
        let mut s = Store::new();
        let committed = vertex(&mut s, "committed");
        call(&mut s, json!({"op":"commit"}));
        let prior = vertex(&mut s, "before-block");
        let outer = call(&mut s, json!({"op":"savepoint"}));
        let retained = vertex(&mut s, "outer");
        let inner = call(&mut s, json!({"op":"savepoint"}));
        let temporary = vertex(&mut s, "inner");
        property(
            &mut s,
            &committed,
            "compute",
            json!({"type":"jvm_runtime","session":"family","id":1}),
        );
        // Duplicate IDs fail after native insertion, so this exercises a
        // genuinely partial statement in the optimized block, not validation.
        assert_eq!(
            s.request(&json!({"op":"addVertex","id":{"type":"string","value":"committed"}}))["ok"],
            false
        );
        for forbidden in [
            json!({"op":"vertices"}),
            json!({"op":"properties","owner":committed["handle"]}),
            json!({"op":"commit"}),
            json!({"op":"rollback"}),
            json!({"op":"release","id":inner}),
            json!({"op":"savepoint"}),
            json!({"op":"addVertex"}),
        ] {
            assert_eq!(s.request(&forbidden)["ok"], false, "{forbidden}");
        }
        assert_eq!(
            s.request(&json!({"op":"rollbackTo","id":outer}))["ok"],
            false
        );
        call(&mut s, json!({"op":"rollbackTo","id":inner}));
        assert_eq!(
            call(&mut s, json!({"op":"vertices"}))
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"properties","owner":committed["handle"]})
            ),
            json!([])
        );
        assert!(s.resolve(&temporary["handle"]).is_err());
        assert!(s.resolve(&retained["handle"]).is_ok());
        assert!(s.resolve(&prior["handle"]).is_ok());
        call(&mut s, json!({"op":"release","id":outer}));
        call(&mut s, json!({"op":"commit"}));
        let last = call(&mut s, json!({"op":"savepoint"}));
        vertex(&mut s, "discard-on-close");
        assert_eq!(s.request(&json!({"op":"unsupported"}))["ok"], false);
        assert_eq!(s.request(&json!({"op":"release","id":last}))["ok"], false);
        call(&mut s, json!({"op":"close"}));
        assert_eq!(s.graph.node_ids("person").unwrap().len(), 3);
        assert!(s.savepoints.is_empty());
    }

    #[test]
    fn id_lookup_prefers_exact_strings_before_numeric_fallback() {
        let mut s = Store::new();
        let number = call(
            &mut s,
            json!({"op":"addVertex","id":{"type":"long","value":"42"}}),
        );
        let query = json!({"op":"vertices","ids":[{"type":"string","value":"42"}]});
        assert_eq!(call(&mut s, query.clone()), json!([number]));
        let exact = call(
            &mut s,
            json!({"op":"addVertex","id":{"type":"string","value":"42"}}),
        );
        assert_eq!(call(&mut s, query), json!([exact]));
        assert_eq!(
            call(
                &mut s,
                json!({"op":"vertices","ids":[{"type":"long","value":"42"}]})
            ),
            json!([number])
        );
        let edge = call(
            &mut s,
            json!({"op":"addEdge","out":number["handle"],"in":exact["handle"],"label":"link","id":{"type":"int","value":7}}),
        );
        let query = json!({"op":"edges","ids":[{"type":"string","value":"7"}]});
        assert_eq!(call(&mut s, query.clone()), json!([edge]));
        let exact_edge = call(
            &mut s,
            json!({"op":"addEdge","out":number["handle"],"in":exact["handle"],"label":"link","id":{"type":"string","value":"7"}}),
        );
        assert_eq!(call(&mut s, query), json!([exact_edge]));
        assert_eq!(
            call(
                &mut s,
                json!({"op":"vertices","ids":[{"type":"string","value":"unknown"}]})
            ),
            json!([])
        );
        assert_eq!(
            call(
                &mut s,
                json!({"op":"edges","ids":[{"type":"string","value":"99"}]})
            ),
            json!([])
        );
    }

    #[test]
    fn direct_handle_lookup_respects_arrow_groups_overlay_deletion_and_rollback() {
        use crate::ir::catalog::{edges_from_columns, nodes_from_columns_with_count};
        let mut graph = PropertyGraph::new();
        graph.add_nodes(nodes_from_columns_with_count("a", vec![], 2));
        graph.add_nodes(nodes_from_columns_with_count("b", vec![], 1));
        graph
            .add_edges(edges_from_columns("r", "a", "a", vec![0], vec![1], vec![]))
            .unwrap();
        graph
            .add_edges(edges_from_columns("r", "a", "b", vec![1], vec![0], vec![]))
            .unwrap();
        assert!(graph.node_is_live("a", 0));
        assert!(!graph.node_is_live("a", -1));
        assert!(!graph.node_is_live("a", 2));
        assert!(!graph.node_is_live("missing", 0));
        assert!(graph.live_edge_endpoints("r", -1).is_none());
        assert!(graph.live_edge_endpoints("r", 2).is_none());
        assert!(graph.live_edge_endpoints("missing", 0).is_none());
        let mut s = Store::from_graph(graph);
        let vertices = call(&mut s, json!({"op":"vertices"}));
        let edges = call(&mut s, json!({"op":"edges"}));
        assert_eq!(edges.as_array().unwrap().len(), 2);
        assert_eq!(edges[1]["in"]["label"], "b");
        let overlay = call(&mut s, json!({"op":"addVertex","label":"a"}));
        let added = call(
            &mut s,
            json!({"op":"addEdge","out":overlay["handle"],"in":vertices[0]["handle"],"label":"r"}),
        );
        for edge in [&edges[0], &edges[1], &added] {
            assert!(s.resolve(&edge["handle"]).is_ok());
        }
        call(&mut s, json!({"op":"remove","owner":edges[0]["handle"]}));
        assert!(s.resolve(&edges[0]["handle"]).is_err());
        assert!(s.resolve(&edges[1]["handle"]).is_ok());
        call(&mut s, json!({"op":"remove","owner":vertices[0]["handle"]}));
        assert!(s.resolve(&vertices[0]["handle"]).is_err());
        assert!(s.resolve(&added["handle"]).is_err());
        call(&mut s, json!({"op":"rollback"}));
        assert!(s.resolve(&vertices[0]["handle"]).is_ok());
        assert!(s.resolve(&edges[0]["handle"]).is_ok());
        assert!(s.resolve(&edges[1]["handle"]).is_ok());
        assert!(s.resolve(&overlay["handle"]).is_err());
        assert!(s.resolve(&added["handle"]).is_err());
        assert_eq!(call(&mut s, json!({"op":"edges"})), edges);
    }

    #[test]
    fn persistent_commit_reopen_rollback_and_exclusive_lock() {
        let path = std::env::temp_dir().join(format!(
            "orchiddb-jvm-test-{}-{}.ngsp",
            std::process::id(),
            SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut s = Store::open(&path).unwrap();
        assert!(Store::open(&path).is_err());
        let a = vertex(&mut s, "a");
        property(
            &mut s,
            &a,
            "amount",
            json!({"type":"bigdecimal","value":"100.00100"}),
        );
        assert_eq!(s.request(&json!({"op":"setVertexProperty","owner":a["handle"],"key":"runtime","cardinality":"single","value":{"type":"jvm_runtime","session":"java","id":1}}))["ok"],false);
        call(&mut s, json!({"op":"commit"}));
        vertex(&mut s, "uncommitted");
        drop(s);
        let mut reopened = Store::open(&path).unwrap();
        let vertices = call(&mut reopened, json!({"op":"vertices"}));
        assert_eq!(vertices.as_array().unwrap().len(), 1);
        assert_eq!(
            call(
                &mut reopened,
                json!({"op":"properties","owner":vertices[0]["handle"]})
            )[0]["value"],
            json!({"type":"bigdecimal","value":"100.00100"})
        );
        drop(reopened);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("ngsp.lock")).unwrap();
    }
}

#[cfg(test)]
#[path = "jvm_bridge/user_keys_tests.rs"]
mod user_keys_tests;
