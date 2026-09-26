//! In-memory property-graph store backed by Apache Arrow.
//!
//! Managed query kernels access a `PropertyGraph`; each label has an Arrow
//! `RecordBatch` of node properties, and each relationship type has a
//! `RecordBatch` of edge rows whose first two columns are `__src_id` and
//! `__dst_id` (interpreted as logical row ids into the corresponding label
//! tables). This keeps the on-disk story Arrow-native while letting the
//! native kernels work in typed `Value`s.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use bigdecimal::BigDecimal;
use num_bigint::BigInt;

use crate::ir::value::Value;
use crate::ir::ElementId;

#[cfg(any(feature = "duckdb", test))]
pub(crate) mod incremental;
pub(crate) mod snapshot;
mod properties;
pub use properties::Cardinality;
mod builders;
mod mutations;
mod keyed;
pub(crate) mod source;
mod values;

pub use builders::{edges_from_columns, nodes_from_columns, nodes_from_columns_with_count};
pub(crate) use values::{array_value, parse_debug_value};
use values::{column_value, map_property_value, map_property_value_if_present};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CatalogError {
    #[error("unknown node label `{0}`")]
    UnknownLabel(String),
    #[error("unknown relationship type `{0}`")]
    UnknownRelType(String),
    #[error("schema mismatch: {0}")]
    Schema(String),
    #[error("delete integrity: {0}")]
    DeleteIntegrity(String),
}

pub type CatalogResult<T> = Result<T, CatalogError>;

/// Property-graph node table.
#[derive(Debug, Clone)]
pub struct NodeTable {
    pub label: String,
    pub batch: RecordBatch,
}

/// Property-graph edge table. The schema is `__src_id, __dst_id, …
/// properties`. `src_label` / `dst_label` describe the endpoint label; for
/// now we only support homogeneous endpoints per relationship type.
#[derive(Debug, Clone)]
pub struct EdgeTable {
    pub rel_type: String,
    pub src_label: String,
    pub dst_label: String,
    pub batch: RecordBatch,
}

/// Cheap isolated checkpoints. Reads share state; the first write detaches it.
/// Correlated DataFusion subplans can checkpoint a large native fixture without
/// copying its complete overlay for every incoming traverser.
#[derive(Debug, Clone, Default)]
struct SnapshotCell<T: Clone>(RefCell<Arc<T>>);
impl<T: Clone> SnapshotCell<T> {
    fn new(value:T)->Self {Self(RefCell::new(Arc::new(value)))}
    fn borrow(&self)->std::cell::Ref<'_,T> {std::cell::Ref::map(self.0.borrow(),|v|v.as_ref())}
    fn borrow_mut(&self)->std::cell::RefMut<'_,T> {std::cell::RefMut::map(self.0.borrow_mut(),Arc::make_mut)}
    fn share_from(&self,other:&Self) {let shared=other.0.borrow().clone();*self.0.borrow_mut()=shared;}
}

#[derive(Debug, Clone, Default)]
pub struct PropertyGraph {
    pub(crate) procedures: Arc<crate::ir::procedures::ProcedureCatalog>,
    /// One timestamp per statement, shared by scalar kernels and SQL planning.
    /// Execution context only; never persisted as graph data.
    statement_clock: SnapshotCell<Option<chrono::DateTime<chrono::Utc>>>,
    pub(crate) source_keys: bool,
    pub(crate) source: Option<Arc<dyn source::GraphSource>>,
    pub(crate) mapping: Option<Arc<crate::ir::rel::mapping::GraphMapping>>,
    pub nodes: HashMap<String, NodeTable>,
    node_keys: HashMap<String, Vec<ElementId>>,
    pub(crate) mapped_defaults: HashMap<(bool, String), BTreeMap<String, Value>>,
    pub(crate) unsupported_defaults: HashMap<(bool, String), BTreeSet<String>>,
    pub(crate) key_types: HashMap<(bool, String), DataType>,
    node_row_locations: Arc<HashMap<(String, ElementId), usize>>,
    edge_keys: HashMap<String, Vec<ElementId>>,
    /// Multiple relationship types are allowed. They are stored under the
    /// rel_type key.
    pub edges: HashMap<String, EdgeTable>,
    /// All physical edge tables for a relationship type. Cypher fixtures
    /// can model relationship groups such as `LIKES(FROM A TO B, FROM B TO C)`;
    /// the public `edges` map keeps a representative table for older callers,
    /// while scans/expands use this grouped storage.
    edge_tables: HashMap<String, Vec<EdgeTable>>,
    edge_row_locations: Arc<HashMap<(String, ElementId), EdgeRowLocation>>,
    edge_row_counts: HashMap<String, i64>,
    /// Insertion order of `add_nodes` calls. Cypher conformance output
    /// uses this index as the high half of node `_ID` printers, so we
    /// expose it alongside the underlying hash-keyed storage.
    node_order: Vec<String>,
    /// Insertion order of `add_edges` calls; shares a numbering space
    /// with `node_order` (edges are numbered after all nodes).
    edge_order: Vec<String>,
    /// Base indexes are shared across clones; adding base tables uses copy-on-write.
    /// This avoids copying every edge when cloning a cached fixture or session.
    /// Outgoing adjacency cache: `(src_label, src_id, rel_type)` →
    /// list of (edge_row, dst_label, dst_id).
    out_adj: Arc<HashMap<(String, ElementId, String), Vec<EdgeRef>>>,
    /// Incoming adjacency cache.
    in_adj: Arc<HashMap<(String, ElementId, String), Vec<EdgeRef>>>,
    /// Session-local graph mutations layered above immutable Arrow
    /// fixture tables. This keeps Graph IR mutation semantics visible to
    /// normal scans/property reads without rebuilding Arrow batches per row.
    overlay: SnapshotCell<GraphOverlay>,
    /// Persistence work since the last successful flush. Derived state only:
    /// snapshots do not carry it, and failed statements restore it with the graph.
    pending: SnapshotCell<PendingChanges>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PendingChanges {
    pub nodes: BTreeSet<(String, ElementId)>,
    pub edges: BTreeSet<(String, ElementId)>,
}

#[derive(Debug, Clone)]
struct EdgeRef {
    edge_row: ElementId,
    other_label: String,
    other_id: ElementId,
}

#[derive(Debug, Clone, Copy)]
struct EdgeRowLocation {
    table_index: usize,
    local_row: i64,
}

/// An edge added by `CREATE`/`MERGE` after the catalog was built. Kept in
/// the overlay rather than an Arrow batch so writes stay cheap and the
/// original batches remain shareable.
#[derive(Debug, Clone)]
struct InsertedEdge {
    src_label: String,
    src_id: ElementId,
    dst_label: String,
    dst_id: ElementId,
    properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
struct GraphOverlay {
    allow_null_property_values: bool,
    node_label_sets: BTreeMap<(String, ElementId), BTreeSet<String>>,
    cypher_ids: BTreeMap<(bool, String, ElementId), i64>,
    // Gremlin edge properties may contain null; scalar null overrides remain tombstones.
    edge_null_properties: BTreeMap<(String, ElementId), BTreeSet<String>>,
    vertex_properties: BTreeMap<(String, ElementId), BTreeMap<String, Vec<properties::VertexPropertyRecord>>>,
    next_property_id: i64,
    public_ids: BTreeMap<(bool, String, ElementId), Value>,
    public_id_lookup: HashMap<String, Vec<(bool,String,ElementId)>>,
    unassigned_public_ids: BTreeSet<(bool,String,ElementId)>,
    inserted_nodes: HashMap<(String, ElementId), BTreeMap<String, Value>>,
    node_property_overrides: HashMap<(String, ElementId), BTreeMap<String, Value>>,
    deleted_nodes: HashSet<(String, ElementId)>,
    /// Keyed by (rel_type, edge_row); `BTreeMap` so iteration is id-ordered.
    inserted_edges: BTreeMap<(String, ElementId), InsertedEdge>,
    edge_property_overrides: HashMap<(String, ElementId), BTreeMap<String, Value>>,
    deleted_edges: HashSet<(String, ElementId)>,
    /// Per-label / per-rel-type insert counters. Kept alongside the maps so
    /// allocating the next id stays O(1) — a bulk `CREATE` loop would
    /// otherwise be quadratic in the number of rows it writes.
    inserted_node_counts: HashMap<String, i64>,
    inserted_edge_counts: HashMap<String, i64>,
    /// Adjacency for overlay edges, so expanding a node does not scan
    /// every edge written so far.
    inserted_out_adj: HashMap<(String, ElementId), Vec<(String, ElementId)>>,
    inserted_in_adj: HashMap<(String, ElementId), Vec<(String, ElementId)>>,
    /// Elements whose base-table property bag was wholesale replaced by
    /// `SET n = {…}`. Any key not present in the overrides reads as null.
    replaced_node_properties: HashSet<(String, ElementId)>,
    replaced_edge_properties: HashSet<(String, ElementId)>,
    /// Distinct property keys written per label / rel-type, in first-seen
    /// order, maintained as writes happen.
    ///
    /// Rendering an element asks for its label's property keys, so
    /// recomputing them by walking every overlay element made bulk writes
    /// quadratic — 60k `CREATE`s spent essentially all their time in
    /// `memcmp` here, and 150k took over ten minutes instead of under a
    /// second. Insert and override are tracked separately so the exposed key
    /// order stays "everything created, then everything overridden"; both are
    /// insertion-ordered, where the maps they replaced were `HashMap`s whose
    /// iteration order varied between runs.
    inserted_node_keys: BTreeMap<String, Vec<String>>,
    override_node_keys: BTreeMap<String, Vec<String>>,
    inserted_edge_keys: BTreeMap<String, Vec<String>>,
    override_edge_keys: BTreeMap<String, Vec<String>>,
}

/// Record every key of `properties` against `label`, preserving first-seen
/// order. The per-label key list is bounded by the schema, so this linear
/// scan is cheap — unlike scanning the elements themselves.
fn note_keys(
    keys: &mut BTreeMap<String, Vec<String>>,
    label: &str,
    properties: &BTreeMap<String, Value>,
) {
    if properties.is_empty() {
        return;
    }
    let entry = keys.entry(label.to_string()).or_default();
    for (key, value) in properties {
        // A null-valued property is not a property: `CREATE (n {x: null})`
        // and `SET n.x = null` both leave `x` absent, so it must not enter
        // the exposed key list.
        if matches!(value, Value::Null) {
            continue;
        }
        if !entry.iter().any(|existing| existing == key) {
            entry.push(key.clone());
        }
    }
}

/// Record a single key written against `label`.
fn note_key(keys: &mut BTreeMap<String, Vec<String>>, label: &str, key: &str) {
    let entry = keys.entry(label.to_string()).or_default();
    if !entry.iter().any(|existing| existing == key) {
        entry.push(key.to_string());
    }
}

impl GraphOverlay {
    fn edge_is_live(&self, rel_type: &str, edge_row: ElementId) -> bool {
        !self
            .deleted_edges
            .contains(&(rel_type.to_string(), edge_row.clone()))
    }
}

impl PropertyGraph {
    #[cfg(feature = "duckdb")]
    pub(crate) fn pending_changes(&self) -> PendingChanges {
        self.pending.borrow().clone()
    }

    #[cfg(feature = "duckdb")]
    pub(crate) fn clear_pending_changes(&self) {
        *self.pending.borrow_mut() = PendingChanges::default();
    }

    /// Whether writes have changed the immutable Arrow catalog.
    pub fn has_mutations(&self) -> bool {
        let overlay = self.overlay.borrow();
        !overlay.node_label_sets.is_empty()
            || !overlay.inserted_node_counts.is_empty()
            || !overlay.inserted_edge_counts.is_empty()
            || !overlay.node_property_overrides.is_empty()
            || !overlay.edge_property_overrides.is_empty()
            || !overlay.deleted_nodes.is_empty()
            || !overlay.deleted_edges.is_empty()
            || !overlay.replaced_node_properties.is_empty()
            || !overlay.replaced_edge_properties.is_empty()
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn begin_statement(&self) {
        *self.statement_clock.borrow_mut() = Some(chrono::Utc::now());
    }

    pub(crate) fn statement_time(&self) -> chrono::DateTime<chrono::Utc> {
        self.statement_clock.borrow().unwrap_or_else(chrono::Utc::now)
    }

    pub fn add_nodes(&mut self, table: NodeTable) {
        if !self.node_order.iter().any(|name| name == &table.label) {
            self.node_order.push(table.label.clone());
        }
        let keys = (0..table.batch.num_rows()).map(|row| ElementId::from(row as i64)).collect();
        self.add_keyed_nodes(table, keys).expect("unique fixture keys");
    }

    /// Insertion-ordered node labels — used for Cypher `_ID` printing,
    /// where the high half of the id encodes the node table index in the
    /// order it was registered (not alphabetic).
    pub fn node_label_order(&self) -> &[String] {
        &self.node_order
    }

    /// Insertion-ordered edge rel-types, sharing the numbering space
    /// with `node_label_order` (edges follow nodes).
    pub fn edge_rel_order(&self) -> &[String] {
        &self.edge_order
    }

    pub fn add_edges(&mut self, table: EdgeTable) -> CatalogResult<()> {
        let schema = table.batch.schema();
        if schema.fields().len() < 2
            || schema.field(0).name() != "__src_id"
            || schema.field(1).name() != "__dst_id"
        {
            return Err(CatalogError::Schema(format!(
                "edge table `{}` must start with __src_id, __dst_id",
                table.rel_type
            )));
        }
        let base = self.edge_row_counts.get(&table.rel_type).copied().unwrap_or(0);
        let keys = (0..table.batch.num_rows()).map(|row| ElementId::from(base + row as i64)).collect();
        self.add_keyed_edges(table, keys)
    }

    pub fn add_keyed_edges(&mut self, table: EdgeTable, keys: Vec<ElementId>) -> CatalogResult<()> {
        if table.batch.num_columns()<2 || table.batch.schema().field(0).name()!="__src_id" || table.batch.schema().field(1).name()!="__dst_id" {
            return Err(CatalogError::Schema("edge table must start with __src_id and __dst_id".into()));
        }

        keyed::validate_keys(&keys, table.batch.num_rows())?;
        let base=self.edge_row_counts.get(&table.rel_type).copied().unwrap_or(0);
        self.source_keys |= keys.iter().enumerate().any(|(row,key)|key.as_i64()!=Some(base+row as i64));
        if keys.iter().any(|key| self.edge_row_locations.contains_key(&(table.rel_type.clone(), key.clone()))) {
            return Err(CatalogError::Schema("duplicate edge identity".into()));
        }
        let endpoints = (0..table.batch.num_rows()).map(|row| {
            Ok((keyed::array_key(table.batch.column(0), row)?, keyed::array_key(table.batch.column(1), row)?))
        }).collect::<CatalogResult<Vec<_>>>()?;
        let rel_type = table.rel_type.clone();
        let table_index = self
            .edge_tables
            .get(&rel_type)
            .map(|tables| tables.len())
            .unwrap_or(0);

        let out_adj = Arc::make_mut(&mut self.out_adj);
        let in_adj = Arc::make_mut(&mut self.in_adj);
        let edge_row_locations = Arc::make_mut(&mut self.edge_row_locations);
        for row in 0..table.batch.num_rows() {
            let (s, d) = &endpoints[row];
            let global_row = keys[row].clone();
            out_adj
                .entry((table.src_label.clone(), s.clone(), table.rel_type.clone()))
                .or_default()
                .push(EdgeRef {
                    edge_row: global_row.clone(),
                    other_label: table.dst_label.clone(),
                    other_id: d.clone(),
                });
            in_adj
                .entry((table.dst_label.clone(), d.clone(), table.rel_type.clone()))
                .or_default()
                .push(EdgeRef {
                    edge_row: global_row.clone(),
                    other_label: table.src_label.clone(),
                    other_id: s.clone(),
                });
            edge_row_locations.insert(
                (rel_type.clone(), global_row.clone()),
                EdgeRowLocation {
                    table_index,
                    local_row: row as i64,
                },
            );
        }
        if !self.edge_order.iter().any(|name| name == &table.rel_type) {
            self.edge_order.push(table.rel_type.clone());
        }
        *self.edge_row_counts.entry(rel_type.clone()).or_insert(0) += table.batch.num_rows() as i64;
        self.edges
            .entry(rel_type.clone())
            .or_insert_with(|| table.clone());
        self.edge_keys.entry(rel_type.clone()).or_default().extend(keys);
        self.edge_tables.entry(rel_type).or_default().push(table);
        Ok(())
    }

    pub fn node_table(&self, label: &str) -> CatalogResult<&NodeTable> {
        self.nodes
            .get(label)
            .ok_or_else(|| CatalogError::UnknownLabel(label.to_string()))
    }

    pub fn edge_table(&self, rel_type: &str) -> CatalogResult<&EdgeTable> {
        self.edges
            .get(rel_type)
            .ok_or_else(|| CatalogError::UnknownRelType(rel_type.to_string()))
    }

    /// All physical edge tables for a relationship type, in insertion order.
    ///
    /// Older callers use [`Self::edge_table`] and see the representative
    /// table stored in `edges`. Relational lowerers need every endpoint group
    /// so a single relationship type can span multiple source/destination
    /// label pairs without losing rows.
    pub fn edge_tables(&self, rel_type: &str) -> CatalogResult<&[EdgeTable]> {
        self.edge_tables
            .get(rel_type)
            .map(Vec::as_slice)
            .or_else(|| self.edges.get(rel_type).map(std::slice::from_ref))
            .ok_or_else(|| CatalogError::UnknownRelType(rel_type.to_string()))
    }

    /// All node labels.
    pub fn labels(&self) -> Vec<String> {
        let mut out = self.nodes.keys().cloned().collect::<Vec<_>>();
        for (label, _) in self.overlay.borrow().inserted_nodes.keys() {
            if !out.iter().any(|existing| existing == label) {
                out.push(label.clone());
            }
        }
        out.sort();
        out
    }

    /// All relationship types.
    pub fn rel_types(&self) -> Vec<String> {
        let mut out = self.edge_tables.keys().cloned().collect::<Vec<_>>();
        for rel_type in self.edges.keys() {
            if !out.iter().any(|existing| existing == rel_type) {
                out.push(rel_type.clone());
            }
        }
        for (rel_type, _) in self.overlay.borrow().inserted_edges.keys() {
            if !out.iter().any(|existing| existing == rel_type) {
                out.push(rel_type.clone());
            }
        }
        out.sort();
        out
    }

    /// Yield (rel_type, edge_row, dst_label, dst_id) for outgoing edges of
    /// the given (src_label, src_id) limited to `rel_filter` (if non-empty).
    pub fn out_edges(
        &self,
        src_label: &str,
        src_id: ElementId,
        rel_filter: &[String],
    ) -> Vec<(String, ElementId, String, ElementId)> {
        let mut out = self.source.as_ref().map(|s| s.neighbors(false, src_label, &src_id, rel_filter)).unwrap_or_default();
        let overlay = self.overlay.borrow();
        out.retain(|(rel, id, label, node)| overlay.edge_is_live(rel, id.clone()) && !overlay.deleted_nodes.contains(&(label.clone(), node.clone())));
        if overlay
            .deleted_nodes
            .contains(&(src_label.to_string(), src_id.clone()))
        {
            return out;
        }
        let rels: Vec<&String> = if rel_filter.is_empty() {
            self.edges.keys().collect()
        } else {
            rel_filter.iter().collect()
        };
        for rel in rels {
            if let Some(refs) = self
                .out_adj
                .get(&(src_label.to_string(), src_id.clone(), rel.to_string()))
            {
                for r in refs {
                    if overlay
                        .deleted_nodes
                        .contains(&(r.other_label.clone(), r.other_id.clone()))
                        || !overlay.edge_is_live(rel, r.edge_row.clone())
                    {
                        continue;
                    }
                    out.push((rel.clone(), r.edge_row.clone(), r.other_label.clone(), r.other_id.clone()));
                }
            }
        }
        if let Some(refs) = overlay
            .inserted_out_adj
            .get(&(src_label.to_string(), src_id.clone()))
        {
            for (rel, edge_row) in refs {
                if !rel_filter.is_empty() && !rel_filter.iter().any(|want| want == rel) {
                    continue;
                }
                let Some(edge) = overlay.inserted_edges.get(&(rel.clone(), edge_row.clone())) else {
                    continue;
                };
                if !overlay.edge_is_live(rel, edge_row.clone())
                    || overlay
                        .deleted_nodes
                        .contains(&(edge.dst_label.clone(), edge.dst_id.clone()))
                {
                    continue;
                }
                out.push((rel.clone(), edge_row.clone(), edge.dst_label.clone(), edge.dst_id.clone()));
            }
        }
        out
    }

    pub fn in_edges(
        &self,
        dst_label: &str,
        dst_id: ElementId,
        rel_filter: &[String],
    ) -> Vec<(String, ElementId, String, ElementId)> {
        let mut out = self.source.as_ref().map(|s| s.neighbors(true, dst_label, &dst_id, rel_filter)).unwrap_or_default();
        let overlay = self.overlay.borrow();
        out.retain(|(rel, id, label, node)| overlay.edge_is_live(rel, id.clone()) && !overlay.deleted_nodes.contains(&(label.clone(), node.clone())));
        if overlay
            .deleted_nodes
            .contains(&(dst_label.to_string(), dst_id.clone()))
        {
            return out;
        }
        let rels: Vec<&String> = if rel_filter.is_empty() {
            self.edges.keys().collect()
        } else {
            rel_filter.iter().collect()
        };
        for rel in rels {
            if let Some(refs) = self
                .in_adj
                .get(&(dst_label.to_string(), dst_id.clone(), rel.to_string()))
            {
                for r in refs {
                    if overlay
                        .deleted_nodes
                        .contains(&(r.other_label.clone(), r.other_id.clone()))
                        || !overlay.edge_is_live(rel, r.edge_row.clone())
                    {
                        continue;
                    }
                    out.push((rel.clone(), r.edge_row.clone(), r.other_label.clone(), r.other_id.clone()));
                }
            }
        }
        if let Some(refs) = overlay
            .inserted_in_adj
            .get(&(dst_label.to_string(), dst_id.clone()))
        {
            for (rel, edge_row) in refs {
                if !rel_filter.is_empty() && !rel_filter.iter().any(|want| want == rel) {
                    continue;
                }
                let Some(edge) = overlay.inserted_edges.get(&(rel.clone(), edge_row.clone())) else {
                    continue;
                };
                if !overlay.edge_is_live(rel, edge_row.clone())
                    || overlay
                        .deleted_nodes
                        .contains(&(edge.src_label.clone(), edge.src_id.clone()))
                {
                    continue;
                }
                out.push((rel.clone(), edge_row.clone(), edge.src_label.clone(), edge.src_id.clone()));
            }
        }
        out
    }

    /// Property-key columns exposed for a node label. Excludes the
    /// id/source/destination columns that the catalog reserves.
    pub fn node_property_keys(&self, label: &str) -> Vec<String> {
        self.node_property_keys_inner(label, false)
    }

    /// Like [`Self::node_property_keys`] but keeps a column literally named
    /// `id`. Cypher fixtures use `id` as an ordinary primary-key property
    /// and expect `RETURN n.*` / node printing to show it; Gremlin treats
    /// element ids as separate from properties, so the default hides it.
    pub fn node_property_keys_with_id(&self, label: &str) -> Vec<String> {
        self.node_property_keys_inner(label, true)
    }

    fn node_property_keys_inner(&self, label: &str, keep_id: bool) -> Vec<String> {
        let excluded: &[&str] = if keep_id { &[] } else { &["id"] };
        let mut out = match self.nodes.get(label) {
            Some(table) => table_property_keys(&table.batch, excluded),
            None => Vec::new(),
        };
        let overlay = self.overlay.borrow();
        for key in overlay
            .inserted_node_keys
            .get(label)
            .into_iter()
            .flatten()
            .chain(overlay.override_node_keys.get(label).into_iter().flatten())
        {
            if !out.iter().any(|existing| existing == key) {
                out.push(key.clone());
            }
        }
        out
    }

    /// Property-key columns exposed for an edge rel-type.
    pub fn edge_property_keys(&self, rel_type: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(tables) = self.edge_tables.get(rel_type) {
            for table in tables {
                for key in
                    table_property_keys(&table.batch, &["src", "dst", "id", "__src_id", "__dst_id"])
                {
                    if !out.iter().any(|existing| existing == &key) {
                        out.push(key);
                    }
                }
            }
        } else if let Some(table) = self.edges.get(rel_type) {
            out = table_property_keys(&table.batch, &["src", "dst", "id", "__src_id", "__dst_id"]);
        }
        // Incremental for the same reason as `node_property_keys_inner`:
        // filtering every overlay edge on each call made bulk edge writes
        // quadratic.
        let overlay = self.overlay.borrow();
        for key in overlay
            .inserted_edge_keys
            .get(rel_type)
            .into_iter()
            .flatten()
            .chain(
                overlay
                    .override_edge_keys
                    .get(rel_type)
                    .into_iter()
                    .flatten(),
            )
        {
            if !out.iter().any(|existing| existing == key) {
                out.push(key.clone());
            }
        }
        out
    }

    /// Read a property of a node by id. Returns `Value::Null` when the
    /// property column is missing or the value is null.
    pub fn node_property(&self, label: &str, id: ElementId, key: &str) -> Value {
        let node_key = (label.to_string(), id.clone());
        let overlay = self.overlay.borrow();
        if overlay.deleted_nodes.contains(&node_key) {
            return Value::Null;
        }
        if let Some(props) = overlay.inserted_nodes.get(&node_key) {
            return map_property_value(props, key);
        }
        if let Some(props) = overlay.node_property_overrides.get(&node_key) {
            if let Some(value) = map_property_value_if_present(props, key) {
                return value.clone();
            }
        }
        if overlay.replaced_node_properties.contains(&node_key) {
            return Value::Null;
        }
        drop(overlay);
        if let Some(source) = &self.source { return source.property(false, label, &id, key); }
        let Some(table) = self.nodes.get(label) else {
            return Value::Null;
        };
        self.node_row_locations.get(&(label.to_owned(), id.clone())).map(|row| column_value(&table.batch, key, *row as i64)).unwrap_or(Value::Null)
    }

    /// Read a property of an edge by edge row id.
    pub fn edge_property(&self, rel_type: &str, edge_row: ElementId, key: &str) -> Value {
        let edge_key = (rel_type.to_string(), edge_row.clone());
        {
            let overlay = self.overlay.borrow();
            if overlay.deleted_edges.contains(&edge_key) {
                return Value::Null;
            }
            if let Some(edge) = overlay.inserted_edges.get(&edge_key) {
                return map_property_value(&edge.properties, key);
            }
            if let Some(props) = overlay.edge_property_overrides.get(&edge_key) {
                if let Some(value) = map_property_value_if_present(props, key) {
                    return value.clone();
                }
            }
            if overlay.replaced_edge_properties.contains(&edge_key) {
                return Value::Null;
            }
        }
        if let Some(source) = &self.source { return source.property(true, rel_type, &edge_row, key); }
        if let Some(location) = self
            .edge_row_locations
            .get(&(rel_type.to_string(), edge_row.clone()))
        {
            let Some(table) = self
                .edge_tables
                .get(rel_type)
                .and_then(|tables| tables.get(location.table_index))
            else {
                return Value::Null;
            };
            return column_value(&table.batch, key, location.local_row);
        }
        let Some(table) = self.edges.get(rel_type) else {
            return Value::Null;
        };
        edge_row.as_i64().map(|row| column_value(&table.batch, key, row)).unwrap_or(Value::Null)
    }

    pub fn edge_ids(&self, rel_type: &str) -> Vec<ElementId> {
        let overlay = self.overlay.borrow();
        let mut out = self.source.as_ref().map(|s| s.ids(true, rel_type)).unwrap_or_else(|| self.edge_keys.get(rel_type).cloned().unwrap_or_default());
        out.retain(|id| overlay.edge_is_live(rel_type, id.clone()));
        out.extend(
            overlay
                .inserted_edges
                .keys()
                .filter(|(edge_rel, _)| edge_rel == rel_type)
                .map(|(_, row)| row.clone())
                .filter(|row| overlay.edge_is_live(rel_type, row.clone())),
        );
        out
    }

    /// Iterate node ids of a given label, optionally filtered by a label
    /// expression that the caller can evaluate (`AnyOf` / `AllOf`).
    pub fn node_ids(&self, label: &str) -> CatalogResult<Vec<ElementId>> {
        let mut out = self.source.as_ref().map(|s| s.ids(false, label)).unwrap_or_else(|| self.node_keys.get(label).cloned().unwrap_or_default());
        let overlay = self.overlay.borrow();
        out.retain(|id| !overlay.deleted_nodes.contains(&(label.to_string(), id.clone().into())));
        out.extend(
            overlay
                .inserted_nodes
                .keys()
                .filter_map(|(node_label, id)| (node_label == label).then_some(id.clone()))
                .filter(|id| !overlay.deleted_nodes.contains(&(label.to_string(), id.clone()))),
        );
        let known_overlay_label = overlay
            .inserted_nodes
            .keys()
            .any(|(node_label, _)| node_label == label);
        out.sort();
        if out.is_empty() && !self.nodes.contains_key(label) && !known_overlay_label {
            return Err(CatalogError::UnknownLabel(label.to_string()));
        }
        Ok(out)
    }

    /// Edge endpoint by edge row.
    pub fn edge_endpoints(
        &self,
        rel_type: &str,
        edge_row: ElementId,
    ) -> Option<(String, ElementId, String, ElementId)> {
        if let Some(edge) = self
            .overlay
            .borrow()
            .inserted_edges
            .get(&(rel_type.to_string(), edge_row.clone()))
        {
            return Some((
                edge.src_label.clone(),
                edge.src_id.clone(),
                edge.dst_label.clone(),
                edge.dst_id.clone(),
            ));
        }
        if let Some(source) = &self.source { return source.endpoints(rel_type, &edge_row); }
        let location = self.edge_row_locations.get(&(rel_type.to_owned(), edge_row.clone()))?;
        let table = self.edge_tables.get(rel_type)?.get(location.table_index)?;
        let row = location.local_row as usize;
        Some((table.src_label.clone(), keyed::array_key(table.batch.column(0), row).ok()?,
            table.dst_label.clone(), keyed::array_key(table.batch.column(1), row).ok()?))
    }

    /// Resolve one live edge address through the native row/overlay indexes.
    /// Unlike enumerating `edge_ids`, this does not scan the relationship table.
    pub(crate) fn live_edge_endpoints(
        &self,
        rel_type: &str,
        edge_row: ElementId,
    ) -> Option<(String, ElementId, String, ElementId)> {
        if !self.overlay.borrow().edge_is_live(rel_type, edge_row.clone()) {
            return None;
        }
        self.edge_endpoints(rel_type, edge_row)
    }

    /// Whether `(label, id)` names a live node: present in the base table or
    /// inserted via the overlay, and not deleted.
    pub(crate) fn node_is_live(&self, label: &str, id: ElementId) -> bool {
        let node_key = (label.to_string(), id.clone());
        let overlay = self.overlay.borrow();
        if overlay.deleted_nodes.contains(&node_key) {
            return false;
        }
        if overlay.inserted_nodes.contains_key(&node_key) {
            return true;
        }
        drop(overlay);
        self.source.as_ref().map_or_else(|| self.node_row_locations.contains_key(&(label.to_owned(), id.clone())), |s| s.exists(false, label, &id))
    }

    /// Declared `(src_label, dst_label)` endpoint pairs for a relationship
    /// type registered via `add_edges`. Empty when the type is created on the
    /// fly by `insert_edge` (so no declared schema is enforced).
    fn rel_endpoint_labels(&self, rel_type: &str) -> Vec<(String, String)> {
        if let Some(tables) = self.edge_tables.get(rel_type) {
            return tables
                .iter()
                .map(|t| (t.src_label.clone(), t.dst_label.clone()))
                .collect();
        }
        self.edges
            .get(rel_type)
            .map(|t| vec![(t.src_label.clone(), t.dst_label.clone())])
            .unwrap_or_default()
    }
}

/// Unwrap a value that must be a node to `(label, id)`, for edge endpoints.
fn node_ref(value: &Value, rel_type: &str, role: &str) -> CatalogResult<(String, ElementId)> {
    match value {
        Value::Node { label, id } => Ok((label.clone(), id.clone())),
        other => Err(CatalogError::Schema(format!(
            "relationship `{rel_type}` {role} must be a node, got {}",
            other.type_name()
        ))),
    }
}

fn table_property_keys(batch: &RecordBatch, exclude: &[&str]) -> Vec<String> {
    batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .filter(|name| !exclude.contains(&name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests;

impl PropertyGraph {
    /// Publish or restore an execution-local overlay. Base Arrow tables remain
    /// immutable throughout JVM execution; bridge mutations use only the overlay.
    pub(crate) fn restore_execution_overlay(&self, checkpoint: &Self) {
        self.overlay.share_from(&checkpoint.overlay);
        self.pending.share_from(&checkpoint.pending);
    }
}
