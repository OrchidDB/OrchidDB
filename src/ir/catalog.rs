//! In-memory property-graph store backed by Apache Arrow.
//!
//! The interpreter drives a `PropertyGraph`; each label has an Arrow
//! `RecordBatch` of node properties, and each relationship type has a
//! `RecordBatch` of edge rows whose first two columns are `__src_id` and
//! `__dst_id` (interpreted as logical row ids into the corresponding label
//! tables). This keeps the on-disk story Arrow-native while letting the
//! interpreter work in plain `Value`s for clarity.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use bigdecimal::BigDecimal;
use num_bigint::BigInt;

use crate::ir::value::Value;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CatalogError {
    #[error("unknown node label `{0}`")]
    UnknownLabel(String),
    #[error("unknown relationship type `{0}`")]
    UnknownRelType(String),
    #[error("schema mismatch: {0}")]
    Schema(String),
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

#[derive(Debug, Clone, Default)]
pub struct PropertyGraph {
    pub nodes: HashMap<String, NodeTable>,
    /// Multiple relationship types are allowed. They are stored under the
    /// rel_type key.
    pub edges: HashMap<String, EdgeTable>,
    /// All physical edge tables for a relationship type. Cypher fixtures
    /// can model relationship groups such as `LIKES(FROM A TO B, FROM B TO C)`;
    /// the public `edges` map keeps a representative table for older callers,
    /// while scans/expands use this grouped storage.
    edge_tables: HashMap<String, Vec<EdgeTable>>,
    edge_row_locations: HashMap<(String, i64), EdgeRowLocation>,
    edge_row_counts: HashMap<String, i64>,
    /// Insertion order of `add_nodes` calls. Cypher conformance output
    /// uses this index as the high half of node `_ID` printers, so we
    /// expose it alongside the underlying hash-keyed storage.
    node_order: Vec<String>,
    /// Insertion order of `add_edges` calls; shares a numbering space
    /// with `node_order` (edges are numbered after all nodes).
    edge_order: Vec<String>,
    /// Outgoing adjacency cache: `(src_label, src_id, rel_type)` →
    /// list of (edge_row, dst_label, dst_id).
    out_adj: HashMap<(String, i64, String), Vec<EdgeRef>>,
    /// Incoming adjacency cache.
    in_adj: HashMap<(String, i64, String), Vec<EdgeRef>>,
    /// Session-local graph mutations layered above immutable Arrow
    /// fixture tables. This keeps Graph IR mutation semantics visible to
    /// normal scans/property reads without rebuilding Arrow batches per row.
    overlay: RefCell<GraphOverlay>,
}

#[derive(Debug, Clone)]
struct EdgeRef {
    edge_row: i64,
    other_label: String,
    other_id: i64,
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
    src_id: i64,
    dst_label: String,
    dst_id: i64,
    properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
struct GraphOverlay {
    inserted_nodes: HashMap<(String, i64), BTreeMap<String, Value>>,
    node_property_overrides: HashMap<(String, i64), BTreeMap<String, Value>>,
    deleted_nodes: HashSet<(String, i64)>,
    /// Keyed by (rel_type, edge_row); `BTreeMap` so iteration is id-ordered.
    inserted_edges: BTreeMap<(String, i64), InsertedEdge>,
    edge_property_overrides: HashMap<(String, i64), BTreeMap<String, Value>>,
    deleted_edges: HashSet<(String, i64)>,
    /// Per-label / per-rel-type insert counters. Kept alongside the maps so
    /// allocating the next id stays O(1) — a bulk `CREATE` loop would
    /// otherwise be quadratic in the number of rows it writes.
    inserted_node_counts: HashMap<String, i64>,
    inserted_edge_counts: HashMap<String, i64>,
    /// Adjacency for overlay edges, so expanding a node does not scan
    /// every edge written so far.
    inserted_out_adj: HashMap<(String, i64), Vec<(String, i64)>>,
    inserted_in_adj: HashMap<(String, i64), Vec<(String, i64)>>,
    /// Elements whose base-table property bag was wholesale replaced by
    /// `SET n = {…}`. Any key not present in the overrides reads as null.
    replaced_node_properties: HashSet<(String, i64)>,
    replaced_edge_properties: HashSet<(String, i64)>,
}

impl GraphOverlay {
    fn edge_is_live(&self, rel_type: &str, edge_row: i64) -> bool {
        !self.deleted_edges.contains(&(rel_type.to_string(), edge_row))
    }
}

impl PropertyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_nodes(&mut self, table: NodeTable) {
        if !self.node_order.iter().any(|name| name == &table.label) {
            self.node_order.push(table.label.clone());
        }
        self.nodes.insert(table.label.clone(), table);
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
        let src = table
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or_else(|| CatalogError::Schema("edge __src_id must be Int64".into()))?;
        let dst = table
            .batch
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or_else(|| CatalogError::Schema("edge __dst_id must be Int64".into()))?;
        let rel_type = table.rel_type.clone();
        let table_index = self
            .edge_tables
            .get(&rel_type)
            .map(|tables| tables.len())
            .unwrap_or(0);
        let base_row = self.edge_row_counts.get(&rel_type).copied().unwrap_or(0);
        for row in 0..table.batch.num_rows() {
            let s = src.value(row);
            let d = dst.value(row);
            let global_row = base_row + row as i64;
            self.out_adj
                .entry((table.src_label.clone(), s, table.rel_type.clone()))
                .or_default()
                .push(EdgeRef {
                    edge_row: global_row,
                    other_label: table.dst_label.clone(),
                    other_id: d,
                });
            self.in_adj
                .entry((table.dst_label.clone(), d, table.rel_type.clone()))
                .or_default()
                .push(EdgeRef {
                    edge_row: global_row,
                    other_label: table.src_label.clone(),
                    other_id: s,
                });
            self.edge_row_locations.insert(
                (rel_type.clone(), global_row),
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
        src_id: i64,
        rel_filter: &[String],
    ) -> Vec<(String, i64, String, i64)> {
        let mut out = Vec::new();
        let overlay = self.overlay.borrow();
        if overlay
            .deleted_nodes
            .contains(&(src_label.to_string(), src_id))
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
                .get(&(src_label.to_string(), src_id, rel.to_string()))
            {
                for r in refs {
                    if overlay
                        .deleted_nodes
                        .contains(&(r.other_label.clone(), r.other_id))
                        || !overlay.edge_is_live(rel, r.edge_row)
                    {
                        continue;
                    }
                    out.push((rel.clone(), r.edge_row, r.other_label.clone(), r.other_id));
                }
            }
        }
        if let Some(refs) = overlay
            .inserted_out_adj
            .get(&(src_label.to_string(), src_id))
        {
            for (rel, edge_row) in refs {
                if !rel_filter.is_empty() && !rel_filter.iter().any(|want| want == rel) {
                    continue;
                }
                let Some(edge) = overlay.inserted_edges.get(&(rel.clone(), *edge_row)) else {
                    continue;
                };
                if !overlay.edge_is_live(rel, *edge_row)
                    || overlay
                        .deleted_nodes
                        .contains(&(edge.dst_label.clone(), edge.dst_id))
                {
                    continue;
                }
                out.push((rel.clone(), *edge_row, edge.dst_label.clone(), edge.dst_id));
            }
        }
        out
    }

    pub fn in_edges(
        &self,
        dst_label: &str,
        dst_id: i64,
        rel_filter: &[String],
    ) -> Vec<(String, i64, String, i64)> {
        let mut out = Vec::new();
        let overlay = self.overlay.borrow();
        if overlay
            .deleted_nodes
            .contains(&(dst_label.to_string(), dst_id))
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
                .get(&(dst_label.to_string(), dst_id, rel.to_string()))
            {
                for r in refs {
                    if overlay
                        .deleted_nodes
                        .contains(&(r.other_label.clone(), r.other_id))
                        || !overlay.edge_is_live(rel, r.edge_row)
                    {
                        continue;
                    }
                    out.push((rel.clone(), r.edge_row, r.other_label.clone(), r.other_id));
                }
            }
        }
        if let Some(refs) = overlay.inserted_in_adj.get(&(dst_label.to_string(), dst_id)) {
            for (rel, edge_row) in refs {
                if !rel_filter.is_empty() && !rel_filter.iter().any(|want| want == rel) {
                    continue;
                }
                let Some(edge) = overlay.inserted_edges.get(&(rel.clone(), *edge_row)) else {
                    continue;
                };
                if !overlay.edge_is_live(rel, *edge_row)
                    || overlay
                        .deleted_nodes
                        .contains(&(edge.src_label.clone(), edge.src_id))
                {
                    continue;
                }
                out.push((rel.clone(), *edge_row, edge.src_label.clone(), edge.src_id));
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
        for ((node_label, _), props) in overlay
            .inserted_nodes
            .iter()
            .chain(overlay.node_property_overrides.iter())
        {
            if node_label == label {
                for key in props.keys() {
                    if !out.iter().any(|existing| existing == key) {
                        out.push(key.clone());
                    }
                }
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
        let overlay = self.overlay.borrow();
        let inserted = overlay
            .inserted_edges
            .iter()
            .filter(|((edge_rel, _), _)| edge_rel == rel_type)
            .map(|(_, edge)| &edge.properties)
            .chain(
                overlay
                    .edge_property_overrides
                    .iter()
                    .filter(|((edge_rel, _), _)| edge_rel == rel_type)
                    .map(|(_, props)| props),
            );
        for props in inserted {
            for key in props.keys() {
                if !out.iter().any(|existing| existing == key) {
                    out.push(key.clone());
                }
            }
        }
        out
    }

    /// Read a property of a node by id. Returns `Value::Null` when the
    /// property column is missing or the value is null.
    pub fn node_property(&self, label: &str, id: i64, key: &str) -> Value {
        let node_key = (label.to_string(), id);
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
        let Some(table) = self.nodes.get(label) else {
            return Value::Null;
        };
        column_value(&table.batch, key, id)
    }

    /// Read a property of an edge by edge row id.
    pub fn edge_property(&self, rel_type: &str, edge_row: i64, key: &str) -> Value {
        let edge_key = (rel_type.to_string(), edge_row);
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
        if let Some(location) = self
            .edge_row_locations
            .get(&(rel_type.to_string(), edge_row))
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
        column_value(&table.batch, key, edge_row)
    }

    pub fn edge_ids(&self, rel_type: &str) -> Vec<i64> {
        let count = self
            .edge_row_counts
            .get(rel_type)
            .copied()
            .unwrap_or_else(|| {
                self.edges
                    .get(rel_type)
                    .map(|table| table.batch.num_rows() as i64)
                    .unwrap_or(0)
            });
        let overlay = self.overlay.borrow();
        let mut out: Vec<i64> = (0..count)
            .filter(|row| overlay.edge_is_live(rel_type, *row))
            .collect();
        out.extend(
            overlay
                .inserted_edges
                .keys()
                .filter(|(edge_rel, _)| edge_rel == rel_type)
                .map(|(_, row)| *row)
                .filter(|row| overlay.edge_is_live(rel_type, *row)),
        );
        out
    }

    /// Append an edge between two node values. Returns the new edge value.
    pub fn insert_edge(
        &self,
        rel_type: impl Into<String>,
        src: &Value,
        dst: &Value,
        properties: BTreeMap<String, Value>,
    ) -> CatalogResult<Value> {
        let rel_type = rel_type.into();
        let (src_label, src_id) = node_ref(src, &rel_type, "source")?;
        let (dst_label, dst_id) = node_ref(dst, &rel_type, "destination")?;
        let base = self
            .edge_row_counts
            .get(&rel_type)
            .copied()
            .unwrap_or_else(|| {
                self.edges
                    .get(&rel_type)
                    .map(|table| table.batch.num_rows() as i64)
                    .unwrap_or(0)
            });
        let mut overlay = self.overlay.borrow_mut();
        let counter = overlay
            .inserted_edge_counts
            .entry(rel_type.clone())
            .or_insert(0);
        let id = base + *counter;
        *counter += 1;
        overlay
            .inserted_out_adj
            .entry((src_label.clone(), src_id))
            .or_default()
            .push((rel_type.clone(), id));
        overlay
            .inserted_in_adj
            .entry((dst_label.clone(), dst_id))
            .or_default()
            .push((rel_type.clone(), id));
        overlay.inserted_edges.insert(
            (rel_type.clone(), id),
            InsertedEdge {
                src_label: src_label.clone(),
                src_id,
                dst_label: dst_label.clone(),
                dst_id,
                properties,
            },
        );
        Ok(Value::Edge {
            rel_type,
            id,
            src_label,
            src_id,
            dst_label,
            dst_id,
            projected_properties: None,
        })
    }

    /// Iterate node ids of a given label, optionally filtered by a label
    /// expression that the caller can evaluate (`AnyOf` / `AllOf`).
    pub fn node_ids(&self, label: &str) -> CatalogResult<Vec<i64>> {
        let mut out = match self.nodes.get(label) {
            Some(table) => (0..table.batch.num_rows())
                .map(|i| i as i64)
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };
        let overlay = self.overlay.borrow();
        out.retain(|id| !overlay.deleted_nodes.contains(&(label.to_string(), *id)));
        out.extend(
            overlay
                .inserted_nodes
                .keys()
                .filter_map(|(node_label, id)| (node_label == label).then_some(*id))
                .filter(|id| !overlay.deleted_nodes.contains(&(label.to_string(), *id))),
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

    pub fn insert_node(
        &self,
        label: impl Into<String>,
        properties: BTreeMap<String, Value>,
    ) -> Value {
        let label = label.into();
        let base_rows = self
            .nodes
            .get(&label)
            .map(|table| table.batch.num_rows() as i64)
            .unwrap_or(0);
        let mut overlay = self.overlay.borrow_mut();
        let counter = overlay.inserted_node_counts.entry(label.clone()).or_insert(0);
        let id = base_rows + *counter;
        *counter += 1;
        overlay
            .inserted_nodes
            .insert((label.clone(), id), properties);
        Value::Node { label, id }
    }

    pub fn set_property(
        &self,
        target: &Value,
        key: impl Into<String>,
        value: Value,
    ) -> CatalogResult<()> {
        let key = key.into();
        match target {
            Value::Node { label, id } => {
                let node_key = (label.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_nodes.contains(&node_key) {
                    return Ok(());
                }
                if let Some(props) = overlay.inserted_nodes.get_mut(&node_key) {
                    props.insert(key, value);
                } else {
                    overlay
                        .node_property_overrides
                        .entry(node_key)
                        .or_default()
                        .insert(key, value);
                }
                Ok(())
            }
            Value::Edge { rel_type, id, .. } => {
                let edge_key = (rel_type.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_edges.contains(&edge_key) {
                    return Ok(());
                }
                if let Some(edge) = overlay.inserted_edges.get_mut(&edge_key) {
                    edge.properties.insert(key, value);
                } else {
                    overlay
                        .edge_property_overrides
                        .entry(edge_key)
                        .or_default()
                        .insert(key, value);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Apply a whole property map to an element. `replace` discards every
    /// property not named in `properties` (Cypher `n = {…}`); otherwise the
    /// map is merged over the existing bag (`n += {…}`).
    pub fn set_properties(
        &self,
        target: &Value,
        properties: BTreeMap<String, Value>,
        replace: bool,
    ) -> CatalogResult<()> {
        match target {
            Value::Node { label, id } => {
                let node_key = (label.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_nodes.contains(&node_key) {
                    return Ok(());
                }
                if let Some(props) = overlay.inserted_nodes.get_mut(&node_key) {
                    if replace {
                        *props = properties;
                    } else {
                        props.extend(properties);
                    }
                    return Ok(());
                }
                if replace {
                    overlay.replaced_node_properties.insert(node_key.clone());
                    overlay.node_property_overrides.insert(node_key, properties);
                } else {
                    overlay
                        .node_property_overrides
                        .entry(node_key)
                        .or_default()
                        .extend(properties);
                }
                Ok(())
            }
            Value::Edge { rel_type, id, .. } => {
                let edge_key = (rel_type.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_edges.contains(&edge_key) {
                    return Ok(());
                }
                if let Some(edge) = overlay.inserted_edges.get_mut(&edge_key) {
                    if replace {
                        edge.properties = properties;
                    } else {
                        edge.properties.extend(properties);
                    }
                    return Ok(());
                }
                if replace {
                    overlay.replaced_edge_properties.insert(edge_key.clone());
                    overlay.edge_property_overrides.insert(edge_key, properties);
                } else {
                    overlay
                        .edge_property_overrides
                        .entry(edge_key)
                        .or_default()
                        .extend(properties);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn delete_value(&self, target: &Value, detach: bool) -> CatalogResult<()> {
        match target {
            Value::Node { label, id } => {
                if detach {
                    for (rel_type, edge_row, _, _) in self.out_edges(label, *id, &[]) {
                        self.overlay
                            .borrow_mut()
                            .deleted_edges
                            .insert((rel_type, edge_row));
                    }
                    for (rel_type, edge_row, _, _) in self.in_edges(label, *id, &[]) {
                        self.overlay
                            .borrow_mut()
                            .deleted_edges
                            .insert((rel_type, edge_row));
                    }
                }
                self.overlay
                    .borrow_mut()
                    .deleted_nodes
                    .insert((label.clone(), *id));
                Ok(())
            }
            Value::Edge { rel_type, id, .. } => {
                self.overlay
                    .borrow_mut()
                    .deleted_edges
                    .insert((rel_type.clone(), *id));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Edge endpoint by edge row.
    pub fn edge_endpoints(
        &self,
        rel_type: &str,
        edge_row: i64,
    ) -> Option<(String, i64, String, i64)> {
        if let Some(edge) = self
            .overlay
            .borrow()
            .inserted_edges
            .get(&(rel_type.to_string(), edge_row))
        {
            return Some((
                edge.src_label.clone(),
                edge.src_id,
                edge.dst_label.clone(),
                edge.dst_id,
            ));
        }
        if let Some(location) = self
            .edge_row_locations
            .get(&(rel_type.to_string(), edge_row))
        {
            let table = self
                .edge_tables
                .get(rel_type)
                .and_then(|tables| tables.get(location.table_index))?;
            let src = table
                .batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()?;
            let dst = table
                .batch
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()?;
            let row = location.local_row as usize;
            if row >= table.batch.num_rows() {
                return None;
            }
            return Some((
                table.src_label.clone(),
                src.value(row),
                table.dst_label.clone(),
                dst.value(row),
            ));
        }
        let table = self.edges.get(rel_type)?;
        let src = table
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()?;
        let dst = table
            .batch
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()?;
        let row = edge_row as usize;
        if row >= table.batch.num_rows() {
            return None;
        }
        Some((
            table.src_label.clone(),
            src.value(row),
            table.dst_label.clone(),
            dst.value(row),
        ))
    }
}

/// Unwrap a value that must be a node to `(label, id)`, for edge endpoints.
fn node_ref(value: &Value, rel_type: &str, role: &str) -> CatalogResult<(String, i64)> {
    match value {
        Value::Node { label, id } => Ok((label.clone(), *id)),
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

fn column_value(batch: &RecordBatch, name: &str, row_id: i64) -> Value {
    let row = row_id as usize;
    let Some(idx) = schema_index(batch.schema().as_ref(), name) else {
        return Value::Null;
    };
    let field = batch.schema().field(idx).clone();
    array_value(batch.column(idx).as_ref(), row, Some(field.as_ref()))
}

fn schema_index(schema: &Schema, name: &str) -> Option<usize> {
    if let Ok(idx) = schema.index_of(name) {
        return Some(idx);
    }
    let mut matches = schema
        .fields()
        .iter()
        .enumerate()
        .filter_map(|(idx, field)| field.name().eq_ignore_ascii_case(name).then_some(idx));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn map_property_value(props: &BTreeMap<String, Value>, key: &str) -> Value {
    map_property_value_if_present(props, key)
        .cloned()
        .unwrap_or(Value::Null)
}

fn map_property_value_if_present<'a>(
    props: &'a BTreeMap<String, Value>,
    key: &str,
) -> Option<&'a Value> {
    if let Some(value) = props.get(key) {
        return Some(value);
    }
    let mut matches = props
        .iter()
        .filter_map(|(name, value)| name.eq_ignore_ascii_case(key).then_some(value));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn array_value(array: &dyn Array, row: usize, field: Option<&Field>) -> Value {
    if row >= array.len() || array.is_null(row) {
        return Value::Null;
    }
    match array.data_type() {
        DataType::Int64 => Value::Int(
            array
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(row),
        ),
        DataType::Int32 => Value::Int(
            array
                .as_any()
                .downcast_ref::<arrow::array::Int32Array>()
                .unwrap()
                .value(row) as i64,
        ),
        DataType::Float64 => Value::Float(
            array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(row),
        ),
        DataType::Boolean => Value::Bool(
            array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .unwrap()
                .value(row),
        ),
        DataType::Utf8 => Value::String({
            let value = array
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(row)
                .to_string();
            match field.and_then(|field| field.metadata().get("new_graph.value_type")) {
                Some(kind) if kind == "datetime" => return Value::DateTime(value),
                Some(kind) if kind == "map" || kind == "value" => {
                    return parse_debug_value(&value).unwrap_or(Value::Null);
                }
                _ => {}
            }
            value
        }),
        _ => Value::Null,
    }
}

pub(crate) fn parse_debug_value(input: &str) -> Option<Value> {
    let input = input.trim();
    if input == "Null" {
        return Some(Value::Null);
    }
    if input == "Bool(true)" {
        return Some(Value::Bool(true));
    }
    if input == "Bool(false)" {
        return Some(Value::Bool(false));
    }
    if let Some(inner) = input.strip_prefix("Int(").and_then(|s| s.strip_suffix(')')) {
        return inner.parse::<i64>().ok().map(Value::Int);
    }
    if let Some(inner) = input
        .strip_prefix("Byte(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<i8>().ok().map(Value::Byte);
    }
    if let Some(inner) = input
        .strip_prefix("Short(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<i16>().ok().map(Value::Short);
    }
    if let Some(inner) = input
        .strip_prefix("Long(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<i64>().ok().map(Value::Long);
    }
    if let Some(inner) = input
        .strip_prefix("Float(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<f64>().ok().map(Value::Float);
    }
    if let Some(inner) = input
        .strip_prefix("Float32(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.parse::<f32>().ok().map(Value::Float32);
    }
    if let Some(inner) = input
        .strip_prefix("BigInt(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return BigInt::from_str(inner).ok().map(Value::BigInt);
    }
    if let Some(inner) = input
        .strip_prefix("BigDecimal(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return BigDecimal::from_str(inner).ok().map(Value::BigDecimal);
    }
    if let Some(inner) = input
        .strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return Some(Value::String(unescape_debug_string(inner)));
    }
    if let Some(inner) = input
        .strip_prefix("DateTime(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return Some(Value::DateTime(unescape_debug_string(inner)));
    }
    if let Some(inner) = input
        .strip_prefix("List([")
        .and_then(|s| s.strip_suffix("])"))
    {
        if inner.trim().is_empty() {
            return Some(Value::List(Vec::new()));
        }
        return split_debug_items(inner)
            .into_iter()
            .map(|item| parse_debug_value(&item))
            .collect::<Option<Vec<_>>>()
            .map(Value::List);
    }
    if let Some(inner) = input
        .strip_prefix("Map({")
        .and_then(|s| s.strip_suffix("})"))
    {
        let mut map = std::collections::BTreeMap::new();
        if inner.trim().is_empty() {
            return Some(Value::Map(map));
        }
        for entry in split_debug_items(inner) {
            let (key, value) = split_debug_map_entry(&entry)?;
            map.insert(key, parse_debug_value(value.trim())?);
        }
        return Some(Value::Map(map));
    }
    None
}

fn split_debug_map_entry(entry: &str) -> Option<(String, &str)> {
    let (key, value) = split_top_level_once(entry, ':')?;
    let key = key
        .trim()
        .strip_prefix('"')?
        .strip_suffix('"')
        .map(unescape_debug_string)?;
    Some((key, value))
}

fn unescape_debug_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('0') => out.push('\0'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn split_debug_items(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut quote = false;
    let mut escape = false;
    let mut depth = 0usize;
    for ch in input.chars() {
        if quote {
            buf.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                quote = false;
            }
            continue;
        }
        match ch {
            '"' => {
                quote = true;
                buf.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                buf.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                buf.push(ch);
            }
            ',' if depth == 0 => {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn split_top_level_once(input: &str, needle: char) -> Option<(&str, &str)> {
    let mut quote = false;
    let mut escape = false;
    let mut depth = 0usize;
    for (idx, ch) in input.char_indices() {
        if quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                quote = false;
            }
            continue;
        }
        match ch {
            '"' => quote = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ch if ch == needle && depth == 0 => {
                return Some((&input[..idx], &input[idx + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

// ---------------- builder helpers ----------------

/// Build a node table from columnar Rust data.
pub fn nodes_from_columns(label: impl Into<String>, columns: Vec<(&str, ArrayRef)>) -> NodeTable {
    let label = label.into();
    if columns.is_empty() {
        // Arrow cannot build a batch with zero columns and an unknown
        // row count. Callers with property-less nodes should pass a
        // hidden column carrying the row count; degrade to an empty
        // table instead of panicking when they don't.
        let schema: SchemaRef = Arc::new(Schema::empty());
        let batch = RecordBatch::try_new_with_options(
            schema,
            Vec::new(),
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(0)),
        )
        .expect("empty node batch");
        return NodeTable { label, batch };
    }
    let fields: Vec<Field> = columns
        .iter()
        .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
        .collect();
    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let arrays: Vec<ArrayRef> = columns.into_iter().map(|(_, a)| a).collect();
    let batch = RecordBatch::try_new(schema, arrays).expect("node batch");
    NodeTable { label, batch }
}

/// Build a node table from columnar Rust data with an explicit row
/// count. Unlike [`nodes_from_columns`], property-less node tables keep
/// their rows (Arrow needs the count when there are zero columns), so
/// scenario initializers can declare `node a:Label` without properties.
pub fn nodes_from_columns_with_count(
    label: impl Into<String>,
    columns: Vec<(&str, ArrayRef)>,
    row_count: usize,
) -> NodeTable {
    let label = label.into();
    if columns.is_empty() {
        let schema: SchemaRef = Arc::new(Schema::empty());
        let batch = RecordBatch::try_new_with_options(
            schema,
            Vec::new(),
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(row_count)),
        )
        .expect("empty node batch with count");
        return NodeTable { label, batch };
    }
    nodes_from_columns(label, columns)
}

/// Build an edge table from `__src_id, __dst_id` plus property columns.
pub fn edges_from_columns(
    rel_type: impl Into<String>,
    src_label: impl Into<String>,
    dst_label: impl Into<String>,
    src: Vec<i64>,
    dst: Vec<i64>,
    extra: Vec<(&str, ArrayRef)>,
) -> EdgeTable {
    assert_eq!(src.len(), dst.len(), "src/dst length mismatch");
    let mut fields = vec![
        Field::new("__src_id", DataType::Int64, false),
        Field::new("__dst_id", DataType::Int64, false),
    ];
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(src)),
        Arc::new(Int64Array::from(dst)),
    ];
    for (name, array) in extra {
        fields.push(Field::new(name, array.data_type().clone(), true));
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema, arrays).expect("edge batch");
    EdgeTable {
        rel_type: rel_type.into(),
        src_label: src_label.into(),
        dst_label: dst_label.into(),
        batch,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::Int64Array;

    use super::*;

    #[test]
    fn property_lookup_falls_back_to_unique_case_insensitive_column() {
        let mut graph = PropertyGraph::new();
        graph.add_nodes(nodes_from_columns(
            "person",
            vec![("ID", Arc::new(Int64Array::from(vec![7])) as ArrayRef)],
        ));

        assert_eq!(graph.node_property("person", 0, "id"), Value::Int(7));
        assert_eq!(graph.node_property("person", 0, "ID"), Value::Int(7));
    }

    #[test]
    fn property_lookup_keeps_ambiguous_case_misses_null() {
        let mut graph = PropertyGraph::new();
        graph.add_nodes(nodes_from_columns(
            "person",
            vec![
                ("ID", Arc::new(Int64Array::from(vec![7])) as ArrayRef),
                ("id", Arc::new(Int64Array::from(vec![8])) as ArrayRef),
            ],
        ));

        assert_eq!(graph.node_property("person", 0, "Id"), Value::Null);
    }
}
