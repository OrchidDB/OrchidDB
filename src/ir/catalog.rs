//! In-memory property-graph store backed by Apache Arrow.
//!
//! The interpreter drives a `PropertyGraph`; each label has an Arrow
//! `RecordBatch` of node properties, and each relationship type has a
//! `RecordBatch` of edge rows whose first two columns are `__src_id` and
//! `__dst_id` (interpreted as logical row ids into the corresponding label
//! tables). This keeps the on-disk story Arrow-native while letting the
//! interpreter work in plain `Value`s for clarity.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};

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
    /// Outgoing adjacency cache: `(src_label, src_id, rel_type)` →
    /// list of (edge_row, dst_label, dst_id).
    out_adj: HashMap<(String, i64, String), Vec<EdgeRef>>,
    /// Incoming adjacency cache.
    in_adj: HashMap<(String, i64, String), Vec<EdgeRef>>,
}

#[derive(Debug, Clone, Copy)]
struct EdgeRef {
    edge_row: i64,
    other_id: i64,
}

impl PropertyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_nodes(&mut self, table: NodeTable) {
        self.nodes.insert(table.label.clone(), table);
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
        for row in 0..table.batch.num_rows() {
            let s = src.value(row);
            let d = dst.value(row);
            self.out_adj
                .entry((table.src_label.clone(), s, table.rel_type.clone()))
                .or_default()
                .push(EdgeRef {
                    edge_row: row as i64,
                    other_id: d,
                });
            self.in_adj
                .entry((table.dst_label.clone(), d, table.rel_type.clone()))
                .or_default()
                .push(EdgeRef {
                    edge_row: row as i64,
                    other_id: s,
                });
        }
        self.edges.insert(table.rel_type.clone(), table);
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

    /// All node labels.
    pub fn labels(&self) -> Vec<String> {
        let mut out = self.nodes.keys().cloned().collect::<Vec<_>>();
        out.sort();
        out
    }

    /// All relationship types.
    pub fn rel_types(&self) -> Vec<String> {
        let mut out = self.edges.keys().cloned().collect::<Vec<_>>();
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
                let edge = match self.edges.get(rel) {
                    Some(edge) => edge,
                    None => continue,
                };
                for r in refs {
                    out.push((rel.clone(), r.edge_row, edge.dst_label.clone(), r.other_id));
                }
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
                let edge = match self.edges.get(rel) {
                    Some(edge) => edge,
                    None => continue,
                };
                for r in refs {
                    out.push((rel.clone(), r.edge_row, edge.src_label.clone(), r.other_id));
                }
            }
        }
        out
    }

    /// Property-key columns exposed for a node label. Excludes the
    /// id/source/destination columns that the catalog reserves.
    pub fn node_property_keys(&self, label: &str) -> Vec<String> {
        match self.nodes.get(label) {
            Some(table) => table_property_keys(&table.batch, &["id"]),
            None => Vec::new(),
        }
    }

    /// Property-key columns exposed for an edge rel-type.
    pub fn edge_property_keys(&self, rel_type: &str) -> Vec<String> {
        match self.edges.get(rel_type) {
            Some(table) => table_property_keys(&table.batch, &["src", "dst", "id"]),
            None => Vec::new(),
        }
    }

    /// Read a property of a node by id. Returns `Value::Null` when the
    /// property column is missing or the value is null.
    pub fn node_property(&self, label: &str, id: i64, key: &str) -> Value {
        let Some(table) = self.nodes.get(label) else {
            return Value::Null;
        };
        column_value(&table.batch, key, id)
    }

    /// Read a property of an edge by edge row id.
    pub fn edge_property(&self, rel_type: &str, edge_row: i64, key: &str) -> Value {
        let Some(table) = self.edges.get(rel_type) else {
            return Value::Null;
        };
        column_value(&table.batch, key, edge_row)
    }

    /// Iterate node ids of a given label, optionally filtered by a label
    /// expression that the caller can evaluate (`AnyOf` / `AllOf`).
    pub fn node_ids(&self, label: &str) -> CatalogResult<Vec<i64>> {
        let table = self.node_table(label)?;
        Ok((0..table.batch.num_rows()).map(|i| i as i64).collect())
    }

    /// Edge endpoint by edge row.
    pub fn edge_endpoints(
        &self,
        rel_type: &str,
        edge_row: i64,
    ) -> Option<(String, i64, String, i64)> {
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
    let Ok(idx) = batch.schema().index_of(name) else {
        return Value::Null;
    };
    let field = batch.schema().field(idx).clone();
    array_value(batch.column(idx).as_ref(), row, Some(field.as_ref()))
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
                Some(kind) if kind == "map" => {
                    return parse_debug_value(&value).unwrap_or(Value::Null);
                }
                _ => {}
            }
            value
        }),
        _ => Value::Null,
    }
}

fn parse_debug_value(input: &str) -> Option<Value> {
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
        .strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return Some(Value::String(inner.to_string()));
    }
    if let Some(inner) = input
        .strip_prefix("DateTime(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return Some(Value::DateTime(inner.to_string()));
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
    let key = key.trim().strip_prefix('"')?.strip_suffix('"')?.to_string();
    Some((key, value))
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
    let fields: Vec<Field> = columns
        .iter()
        .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
        .collect();
    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let arrays: Vec<ArrayRef> = columns.into_iter().map(|(_, a)| a).collect();
    let batch = RecordBatch::try_new(schema, arrays).expect("node batch");
    NodeTable { label, batch }
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
