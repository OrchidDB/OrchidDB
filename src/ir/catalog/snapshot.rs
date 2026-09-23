//! Lossless snapshot serialization for [`PropertyGraph`].
//!
//! Base Arrow tables are encoded with the Arrow IPC stream format, which
//! preserves the full schema (including per-field metadata such as
//! `new_graph.value_type`) and every physical array type. The session
//! overlay — which lives in Rust-side maps rather than Arrow batches — is
//! encoded with a self-describing binary format in which every value is
//! prefixed by a tag so that each [`Value`] variant round-trips exactly.
//!
//! The wire format is versioned and length-delimited so corrupt or truncated
//! input is rejected rather than silently mis-parsed.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use arrow::array::{Int64Array, RecordBatch};
use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use bigdecimal::BigDecimal;
use num_bigint::BigInt;

use super::PropertyGraph;
use super::{EdgeRef, EdgeRowLocation, EdgeTable, GraphOverlay, InsertedEdge, NodeTable};
use crate::ir::value::Value;

const MAGIC: &[u8] = b"NGSP";
const VERSION: u32 = 1;

// Top-level sections.
const SEC_NODE_ORDER: u8 = 0x01;
const SEC_EDGE_ORDER: u8 = 0x02;
const SEC_NODES: u8 = 0x03;
const SEC_EDGES: u8 = 0x04;
const SEC_EDGE_TABLES: u8 = 0x05;
const SEC_OVERLAY: u8 = 0x06;

// Overlay sub-sections.
const OV_INSERTED_NODES: u8 = 0x10;
const OV_NODE_OVERRIDES: u8 = 0x11;
const OV_DELETED_NODES: u8 = 0x12;
const OV_INSERTED_EDGES: u8 = 0x13;
const OV_EDGE_OVERRIDES: u8 = 0x14;
const OV_DELETED_EDGES: u8 = 0x15;
const OV_NODE_COUNTS: u8 = 0x16;
const OV_EDGE_COUNTS: u8 = 0x17;
const OV_OUT_ADJ: u8 = 0x18;
const OV_IN_ADJ: u8 = 0x19;
const OV_REPLACED_NODES: u8 = 0x1A;
const OV_REPLACED_EDGES: u8 = 0x1B;
const OV_INSERTED_NODE_KEYS: u8 = 0x1C;
const OV_OVERRIDE_NODE_KEYS: u8 = 0x1D;
const OV_INSERTED_EDGE_KEYS: u8 = 0x1E;
const OV_OVERRIDE_EDGE_KEYS: u8 = 0x1F;

fn is_overlay_tag(tag: u8) -> bool {
    (0x10..=0x1F).contains(&tag)
}

// Value tags.
const V_NULL: u8 = 0;
const V_BOOL: u8 = 1;
const V_BYTE: u8 = 2;
const V_UINT8: u8 = 3;
const V_SHORT: u8 = 4;
const V_UINT16: u8 = 5;
const V_INT: u8 = 6;
const V_UINT32: u8 = 7;
const V_LONG: u8 = 8;
const V_UINT64: u8 = 9;
const V_FLOAT32: u8 = 10;
const V_FLOAT: u8 = 11;
const V_BIGINT: u8 = 12;
const V_UINT128: u8 = 13;
const V_BIGDECIMAL: u8 = 14;
const V_DATETIME: u8 = 15;
const V_INTERNAL_ID: u8 = 16;
const V_STRING: u8 = 17;
const V_NODE: u8 = 18;
const V_EDGE: u8 = 19;
const V_LIST: u8 = 20;
const V_MAP: u8 = 21;
const V_PATH: u8 = 22;

impl PropertyGraph {
    /// Serialize the graph (base tables + overlay) into a self-contained byte
    /// vector. Fails only if an underlying Arrow batch cannot be IPC-encoded.
    pub(crate) fn snapshot_encode(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        put_u32(&mut out, VERSION);
        put_u32(&mut out, 0); // flags (reserved)

        write_section(&mut out, SEC_NODE_ORDER, &encode_str_list(&self.node_order));
        write_section(&mut out, SEC_EDGE_ORDER, &encode_str_list(&self.edge_order));
        write_section(&mut out, SEC_NODES, &encode_nodes(&self.nodes)?);
        write_section(&mut out, SEC_EDGES, &encode_edges(&self.edges)?);
        write_section(
            &mut out,
            SEC_EDGE_TABLES,
            &encode_edge_tables(&self.edge_tables)?,
        );
        write_section(
            &mut out,
            SEC_OVERLAY,
            &encode_overlay(&self.overlay.borrow())?,
        );
        Ok(out)
    }

    /// Reconstruct a [`PropertyGraph`] from [`Self::snapshot_encode`] output.
    pub(crate) fn snapshot_decode(data: &[u8]) -> Result<PropertyGraph, String> {
        let mut r = Reader::new(data);
        if r.take(4)? != MAGIC {
            return Err("snapshot magic mismatch".to_string());
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(format!("unsupported snapshot version {version}"));
        }
        if r.u32()? != 0 {
            return Err("unsupported snapshot flags".into());
        }

        let mut graph = PropertyGraph::new();
        let mut seen = std::collections::BTreeSet::new();
        while !r.is_empty() {
            let tag = r.u8()?;
            if !seen.insert(tag) {
                return Err(format!("duplicate snapshot section {tag}"));
            }
            let payload = r.blob()?;
            match tag {
                SEC_NODE_ORDER => graph.node_order = decode_str_list(&mut Reader::new(payload))?,
                SEC_EDGE_ORDER => graph.edge_order = decode_str_list(&mut Reader::new(payload))?,
                SEC_NODES => graph.nodes = parse_nodes(payload)?,
                SEC_EDGES => graph.edges = parse_edges(payload)?,
                SEC_EDGE_TABLES => graph.edge_tables = parse_edge_tables(payload)?,
                SEC_OVERLAY => graph.overlay = RefCell::new(parse_overlay(payload)?),
                _ => return Err(format!("unknown snapshot section {tag}")),
            }
        }
        if seen != (SEC_NODE_ORDER..=SEC_OVERLAY).collect() {
            return Err("missing required graph snapshot section".into());
        }
        graph.rebuild_edge_indices();
        Ok(graph)
    }

    /// Rebuild the derived edge caches (`edge_row_locations`,
    /// `edge_row_counts`, `out_adj`, `in_adj`) from the grouped base tables.
    /// Mirrors the indexing performed by `add_edges` so row numbering and
    /// adjacency are identical to the original graph.
    fn rebuild_edge_indices(&mut self) {
        self.edge_row_locations.clear();
        self.edge_row_counts.clear();
        self.out_adj.clear();
        self.in_adj.clear();

        let mut rel_types: Vec<String> = self.edge_tables.keys().cloned().collect();
        rel_types.sort();
        for rel_type in rel_types {
            let tables = &self.edge_tables[&rel_type];
            let mut base_row: i64 = 0;
            for (table_index, table) in tables.iter().enumerate() {
                let Some(src) = table.batch.column(0).as_any().downcast_ref::<Int64Array>() else {
                    continue;
                };
                let Some(dst) = table.batch.column(1).as_any().downcast_ref::<Int64Array>() else {
                    continue;
                };
                for row in 0..table.batch.num_rows() {
                    let s = src.value(row);
                    let d = dst.value(row);
                    let global_row = base_row + row as i64;
                    self.out_adj
                        .entry((table.src_label.clone(), s, rel_type.clone()))
                        .or_default()
                        .push(EdgeRef {
                            edge_row: global_row,
                            other_label: table.dst_label.clone(),
                            other_id: d,
                        });
                    self.in_adj
                        .entry((table.dst_label.clone(), d, rel_type.clone()))
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
                base_row += table.batch.num_rows() as i64;
            }
            self.edge_row_counts.insert(rel_type.clone(), base_row);
        }
    }
}

// ---------------- primitive writers ----------------

fn put_u8(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_i16(out: &mut Vec<u8>, v: i16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_i64(out: &mut Vec<u8>, v: i64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f64(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u64(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_u64(out, b.len() as u64);
    out.extend_from_slice(b);
}

fn put_str_list(out: &mut Vec<u8>, list: &[String]) {
    put_u64(out, list.len() as u64);
    for s in list {
        put_str(out, s);
    }
}

fn put_map(out: &mut Vec<u8>, map: &BTreeMap<String, Value>) {
    put_u64(out, map.len() as u64);
    for (key, value) in map {
        put_str(out, key);
        encode_value(out, value);
    }
}

fn put_values(out: &mut Vec<u8>, items: &[Value]) {
    put_u64(out, items.len() as u64);
    for item in items {
        encode_value(out, item);
    }
}

fn write_section(out: &mut Vec<u8>, tag: u8, payload: &[u8]) {
    put_u8(out, tag);
    put_u64(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

// ---------------- section builders ----------------

fn encode_nodes(nodes: &HashMap<String, NodeTable>) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    let mut labels: Vec<&String> = nodes.keys().collect();
    labels.sort();
    put_u64(&mut b, labels.len() as u64);
    for label in labels {
        let table = &nodes[label];
        put_str(&mut b, label);
        put_str(&mut b, &table.label);
        put_bytes(&mut b, &encode_batch(&table.batch)?);
    }
    Ok(b)
}

fn encode_edges(edges: &HashMap<String, EdgeTable>) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    let mut rel_types: Vec<&String> = edges.keys().collect();
    rel_types.sort();
    put_u64(&mut b, rel_types.len() as u64);
    for rel_type in rel_types {
        let table = &edges[rel_type];
        put_str(&mut b, rel_type);
        put_str(&mut b, &table.rel_type);
        put_str(&mut b, &table.src_label);
        put_str(&mut b, &table.dst_label);
        put_bytes(&mut b, &encode_batch(&table.batch)?);
    }
    Ok(b)
}

fn encode_edge_tables(edge_tables: &HashMap<String, Vec<EdgeTable>>) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    let mut rel_types: Vec<&String> = edge_tables.keys().collect();
    rel_types.sort();
    put_u64(&mut b, rel_types.len() as u64);
    for rel_type in rel_types {
        let group = &edge_tables[rel_type];
        put_str(&mut b, rel_type);
        put_u64(&mut b, group.len() as u64);
        for table in group {
            put_str(&mut b, &table.rel_type);
            put_str(&mut b, &table.src_label);
            put_str(&mut b, &table.dst_label);
            put_bytes(&mut b, &encode_batch(&table.batch)?);
        }
    }
    Ok(b)
}

fn encode_overlay(ov: &GraphOverlay) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();

    {
        let mut keys: Vec<(String, i64)> = ov.inserted_nodes.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            put_map(&mut b, &ov.inserted_nodes[&(label.clone(), *id)]);
        }
        write_section(&mut out, OV_INSERTED_NODES, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.node_property_overrides.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            put_map(&mut b, &ov.node_property_overrides[&(label.clone(), *id)]);
        }
        write_section(&mut out, OV_NODE_OVERRIDES, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.deleted_nodes.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (label, id) in items {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_DELETED_NODES, &b);
    }
    {
        let mut b = Vec::new();
        put_u64(&mut b, ov.inserted_edges.len() as u64);
        for ((rel_type, id), edge) in &ov.inserted_edges {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
            put_str(&mut b, &edge.src_label);
            put_i64(&mut b, edge.src_id);
            put_str(&mut b, &edge.dst_label);
            put_i64(&mut b, edge.dst_id);
            put_map(&mut b, &edge.properties);
        }
        write_section(&mut out, OV_INSERTED_EDGES, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.edge_property_overrides.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (rel_type, id) in &keys {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
            put_map(
                &mut b,
                &ov.edge_property_overrides[&(rel_type.clone(), *id)],
            );
        }
        write_section(&mut out, OV_EDGE_OVERRIDES, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.deleted_edges.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (rel_type, id) in items {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_DELETED_EDGES, &b);
    }
    {
        let mut keys: Vec<&String> = ov.inserted_node_counts.keys().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for label in keys {
            put_str(&mut b, label);
            put_i64(
                &mut b,
                ov.inserted_node_counts.get(label).copied().unwrap_or(0),
            );
        }
        write_section(&mut out, OV_NODE_COUNTS, &b);
    }
    {
        let mut keys: Vec<&String> = ov.inserted_edge_counts.keys().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for rel_type in keys {
            put_str(&mut b, rel_type);
            put_i64(
                &mut b,
                ov.inserted_edge_counts.get(rel_type).copied().unwrap_or(0),
            );
        }
        write_section(&mut out, OV_EDGE_COUNTS, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.inserted_out_adj.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            let adj = &ov.inserted_out_adj[&(label.clone(), *id)];
            put_u64(&mut b, adj.len() as u64);
            for (rel_type, row) in adj {
                put_str(&mut b, rel_type);
                put_i64(&mut b, *row);
            }
        }
        write_section(&mut out, OV_OUT_ADJ, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.inserted_in_adj.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            let adj = &ov.inserted_in_adj[&(label.clone(), *id)];
            put_u64(&mut b, adj.len() as u64);
            for (rel_type, row) in adj {
                put_str(&mut b, rel_type);
                put_i64(&mut b, *row);
            }
        }
        write_section(&mut out, OV_IN_ADJ, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.replaced_node_properties.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (label, id) in items {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_REPLACED_NODES, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.replaced_edge_properties.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (rel_type, id) in items {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_REPLACED_EDGES, &b);
    }
    for (tag, keys) in [
        (OV_INSERTED_NODE_KEYS, &ov.inserted_node_keys),
        (OV_OVERRIDE_NODE_KEYS, &ov.override_node_keys),
        (OV_INSERTED_EDGE_KEYS, &ov.inserted_edge_keys),
        (OV_OVERRIDE_EDGE_KEYS, &ov.override_edge_keys),
    ] {
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, list) in keys {
            put_str(&mut b, label);
            put_str_list(&mut b, list);
        }
        write_section(&mut out, tag, &b);
    }

    Ok(out)
}

// ---------------- section parsers ----------------

fn parse_nodes(payload: &[u8]) -> Result<HashMap<String, NodeTable>, String> {
    let mut r = Reader::new(payload);
    let n = r.count()?;
    let mut map = HashMap::with_capacity(n);
    for _ in 0..n {
        let key = r.str()?;
        let label = r.str()?;
        let batch = decode_batch(r.blob()?)?;
        map.insert(key, NodeTable { label, batch });
    }
    finish(&r)?;
    Ok(map)
}

fn parse_edges(payload: &[u8]) -> Result<HashMap<String, EdgeTable>, String> {
    let mut r = Reader::new(payload);
    let n = r.count()?;
    let mut map = HashMap::with_capacity(n);
    for _ in 0..n {
        let key = r.str()?;
        let rel_type = r.str()?;
        let src_label = r.str()?;
        let dst_label = r.str()?;
        let batch = decode_batch(r.blob()?)?;
        map.insert(
            key,
            EdgeTable {
                rel_type,
                src_label,
                dst_label,
                batch,
            },
        );
    }
    finish(&r)?;
    Ok(map)
}

fn parse_edge_tables(payload: &[u8]) -> Result<HashMap<String, Vec<EdgeTable>>, String> {
    let mut r = Reader::new(payload);
    let n = r.count()?;
    let mut map = HashMap::with_capacity(n);
    for _ in 0..n {
        let key = r.str()?;
        let m = r.count()?;
        let mut group = Vec::with_capacity(m);
        for _ in 0..m {
            let rel_type = r.str()?;
            let src_label = r.str()?;
            let dst_label = r.str()?;
            let batch = decode_batch(r.blob()?)?;
            group.push(EdgeTable {
                rel_type,
                src_label,
                dst_label,
                batch,
            });
        }
        map.insert(key, group);
    }
    finish(&r)?;
    Ok(map)
}

fn parse_overlay(payload: &[u8]) -> Result<GraphOverlay, String> {
    let mut ov = GraphOverlay::default();
    let mut r = Reader::new(payload);
    while !r.is_empty() {
        let tag = r.u8()?;
        let sub = r.blob()?;
        let mut sr = Reader::new(sub);
        match tag {
            OV_INSERTED_NODES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let label = sr.str()?;
                    let id = sr.i64()?;
                    let props = decode_map(&mut sr)?;
                    ov.inserted_nodes.insert((label, id), props);
                }
            }
            OV_NODE_OVERRIDES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let label = sr.str()?;
                    let id = sr.i64()?;
                    let props = decode_map(&mut sr)?;
                    ov.node_property_overrides.insert((label, id), props);
                }
            }
            OV_DELETED_NODES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.deleted_nodes.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_INSERTED_EDGES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let rel_type = sr.str()?;
                    let id = sr.i64()?;
                    let src_label = sr.str()?;
                    let src_id = sr.i64()?;
                    let dst_label = sr.str()?;
                    let dst_id = sr.i64()?;
                    let properties = decode_map(&mut sr)?;
                    ov.inserted_edges.insert(
                        (rel_type, id),
                        InsertedEdge {
                            src_label,
                            src_id,
                            dst_label,
                            dst_id,
                            properties,
                        },
                    );
                }
            }
            OV_EDGE_OVERRIDES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let rel_type = sr.str()?;
                    let id = sr.i64()?;
                    let props = decode_map(&mut sr)?;
                    ov.edge_property_overrides.insert((rel_type, id), props);
                }
            }
            OV_DELETED_EDGES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.deleted_edges.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_NODE_COUNTS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_node_counts.insert(sr.str()?, sr.i64()?);
                }
            }
            OV_EDGE_COUNTS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_edge_counts.insert(sr.str()?, sr.i64()?);
                }
            }
            OV_OUT_ADJ | OV_IN_ADJ => {
                let n = sr.count()?;
                for _ in 0..n {
                    let label = sr.str()?;
                    let id = sr.i64()?;
                    let m = sr.count()?;
                    let mut adj = Vec::with_capacity(m);
                    for _ in 0..m {
                        adj.push((sr.str()?, sr.i64()?));
                    }
                    if tag == OV_OUT_ADJ {
                        ov.inserted_out_adj.insert((label, id), adj);
                    } else {
                        ov.inserted_in_adj.insert((label, id), adj);
                    }
                }
            }
            OV_REPLACED_NODES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.replaced_node_properties.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_REPLACED_EDGES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.replaced_edge_properties.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_INSERTED_NODE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_node_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            OV_OVERRIDE_NODE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.override_node_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            OV_INSERTED_EDGE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_edge_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            OV_OVERRIDE_EDGE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.override_edge_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            _ => {}
        }
        if is_overlay_tag(tag) {
            finish(&sr)?;
        }
    }
    Ok(ov)
}

// ---------------- Value codec ----------------

fn encode_value(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Null => put_u8(out, V_NULL),
        Value::Bool(b) => {
            put_u8(out, V_BOOL);
            put_u8(out, *b as u8);
        }
        Value::Byte(v) => {
            put_u8(out, V_BYTE);
            put_u8(out, *v as u8);
        }
        Value::UInt8(v) => {
            put_u8(out, V_UINT8);
            put_u8(out, *v);
        }
        Value::Short(v) => {
            put_u8(out, V_SHORT);
            put_i16(out, *v);
        }
        Value::UInt16(v) => {
            put_u8(out, V_UINT16);
            put_u16(out, *v);
        }
        Value::Int(v) => {
            put_u8(out, V_INT);
            put_i64(out, *v);
        }
        Value::UInt32(v) => {
            put_u8(out, V_UINT32);
            put_u32(out, *v);
        }
        Value::Long(v) => {
            put_u8(out, V_LONG);
            put_i64(out, *v);
        }
        Value::UInt64(v) => {
            put_u8(out, V_UINT64);
            put_u64(out, *v);
        }
        Value::Float32(v) => {
            put_u8(out, V_FLOAT32);
            put_f32(out, *v);
        }
        Value::Float(v) => {
            put_u8(out, V_FLOAT);
            put_f64(out, *v);
        }
        Value::BigInt(v) => {
            put_u8(out, V_BIGINT);
            put_str(out, &v.to_string());
        }
        Value::UInt128(v) => {
            put_u8(out, V_UINT128);
            put_str(out, &v.to_string());
        }
        Value::BigDecimal(v) => {
            put_u8(out, V_BIGDECIMAL);
            put_str(out, &v.to_string());
        }
        Value::DateTime(v) => {
            put_u8(out, V_DATETIME);
            put_str(out, v);
        }
        Value::InternalId { table, offset } => {
            put_u8(out, V_INTERNAL_ID);
            put_i64(out, *table);
            put_i64(out, *offset);
        }
        Value::String(v) => {
            put_u8(out, V_STRING);
            put_str(out, v);
        }
        Value::Node { label, id } => {
            put_u8(out, V_NODE);
            put_str(out, label);
            put_i64(out, *id);
        }
        Value::Edge {
            rel_type,
            id,
            src_label,
            src_id,
            dst_label,
            dst_id,
            projected_properties,
        } => {
            put_u8(out, V_EDGE);
            put_str(out, rel_type);
            put_i64(out, *id);
            put_str(out, src_label);
            put_i64(out, *src_id);
            put_str(out, dst_label);
            put_i64(out, *dst_id);
            match projected_properties {
                None => put_u8(out, 0),
                Some(keys) => {
                    put_u8(out, 1);
                    put_str_list(out, keys);
                }
            }
        }
        Value::List(items) => {
            put_u8(out, V_LIST);
            put_values(out, items);
        }
        Value::Map(map) => {
            put_u8(out, V_MAP);
            put_map(out, map);
        }
        Value::Path(items) => {
            put_u8(out, V_PATH);
            put_values(out, items);
        }
    }
}

fn decode_value(r: &mut Reader) -> Result<Value, String> {
    let tag = r.u8()?;
    Ok(match tag {
        V_NULL => Value::Null,
        V_BOOL => Value::Bool(r.u8()? != 0),
        V_BYTE => Value::Byte(r.u8()? as i8),
        V_UINT8 => Value::UInt8(r.u8()?),
        V_SHORT => Value::Short(r.i16()?),
        V_UINT16 => Value::UInt16(r.u16()?),
        V_INT => Value::Int(r.i64()?),
        V_UINT32 => Value::UInt32(r.u32()?),
        V_LONG => Value::Long(r.i64()?),
        V_UINT64 => Value::UInt64(r.u64()?),
        V_FLOAT32 => Value::Float32(r.f32()?),
        V_FLOAT => Value::Float(r.f64()?),
        V_BIGINT => {
            Value::BigInt(BigInt::from_str(&r.str()?).map_err(|e| format!("invalid BigInt: {e}"))?)
        }
        V_UINT128 => Value::UInt128(
            BigInt::from_str(&r.str()?).map_err(|e| format!("invalid UInt128: {e}"))?,
        ),
        V_BIGDECIMAL => Value::BigDecimal(
            BigDecimal::from_str(&r.str()?).map_err(|e| format!("invalid BigDecimal: {e}"))?,
        ),
        V_DATETIME => Value::DateTime(r.str()?),
        V_INTERNAL_ID => Value::InternalId {
            table: r.i64()?,
            offset: r.i64()?,
        },
        V_STRING => Value::String(r.str()?),
        V_NODE => Value::Node {
            label: r.str()?,
            id: r.i64()?,
        },
        V_EDGE => Value::Edge {
            rel_type: r.str()?,
            id: r.i64()?,
            src_label: r.str()?,
            src_id: r.i64()?,
            dst_label: r.str()?,
            dst_id: r.i64()?,
            projected_properties: if r.u8()? != 0 {
                Some(decode_str_list(r)?)
            } else {
                None
            },
        },
        V_LIST => Value::List(decode_values(r)?),
        V_MAP => Value::Map(decode_map(r)?),
        V_PATH => Value::Path(decode_values(r)?),
        other => return Err(format!("unknown value tag {other}")),
    })
}

fn decode_values(r: &mut Reader) -> Result<Vec<Value>, String> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(decode_value(r)?);
    }
    Ok(out)
}

fn decode_map(r: &mut Reader) -> Result<BTreeMap<String, Value>, String> {
    let n = r.count()?;
    let mut map = BTreeMap::new();
    for _ in 0..n {
        map.insert(r.str()?, decode_value(r)?);
    }
    Ok(map)
}

fn decode_str_list(r: &mut Reader) -> Result<Vec<String>, String> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(r.str()?);
    }
    Ok(out)
}

fn encode_str_list(list: &[String]) -> Vec<u8> {
    let mut b = Vec::new();
    put_u64(&mut b, list.len() as u64);
    for s in list {
        put_str(&mut b, s);
    }
    b
}

/// Encode a single [`Value`] into a standalone byte buffer using the
/// snapshot value codec. Exposed to the incremental overlay codec so it can
/// reuse the exact same tag-complete (including NaN bit patterns) encoding.
#[cfg(any(feature = "duckdb", test))]
pub(super) fn encode_value_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    encode_value(&mut out, value);
    out
}

/// Decode exactly one [`Value`] from `data`, rejecting any trailing bytes.
/// Exposed to the incremental overlay codec.
#[cfg(any(feature = "duckdb", test))]
pub(super) fn decode_value_bytes(data: &[u8]) -> Result<Value, String> {
    let mut r = Reader::new(data);
    let value = decode_value(&mut r)?;
    finish(&r)?;
    Ok(value)
}

// ---------------- Arrow IPC ----------------

fn encode_batch(batch: &RecordBatch) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buf, batch.schema().as_ref())
            .map_err(|e| format!("IPC schema write failed: {e}"))?;
        writer
            .write(batch)
            .map_err(|e| format!("IPC batch write failed: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("IPC finish failed: {e}"))?;
    }
    Ok(buf)
}

fn decode_batch(data: &[u8]) -> Result<RecordBatch, String> {
    let reader = StreamReader::try_new(std::io::Cursor::new(data), None)
        .map_err(|e| format!("IPC schema read failed: {e}"))?;
    let mut batches = reader
        .collect::<Result<Vec<RecordBatch>, _>>()
        .map_err(|e| format!("IPC batch read failed: {e}"))?;
    match batches.len() {
        1 => Ok(batches.remove(0)),
        0 => Err("IPC stream contained no record batch".to_string()),
        _ => Err("IPC stream contained multiple record batches".to_string()),
    }
}

// ---------------- reader ----------------

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if n > self.remaining() {
            return Err("unexpected end of snapshot".to_string());
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Result<i16, String> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn str(&mut self) -> Result<String, String> {
        let len = self.u64()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| "invalid UTF-8 in snapshot".to_string())
    }

    fn blob(&mut self) -> Result<&'a [u8], String> {
        let len = self.u64()? as usize;
        self.take(len)
    }

    /// Read a length prefix, rejecting counts that cannot possibly fit in the
    /// remaining bytes. This turns a corrupt length into a clean error instead
    /// of an oversized allocation.
    fn count(&mut self) -> Result<usize, String> {
        let n = self.u64()? as usize;
        if n > self.remaining() {
            return Err("element count exceeds remaining data".to_string());
        }
        Ok(n)
    }
}

fn finish(r: &Reader) -> Result<(), String> {
    if r.is_empty() {
        Ok(())
    } else {
        Err("trailing bytes after section".to_string())
    }
}

// ---------------- test-only deep comparison ----------------

#[cfg(test)]
impl PropertyGraph {
    /// Full-structure equality for tests: base tables, derived caches and the
    /// complete overlay must match exactly.
    pub(crate) fn graph_deep_eq(&self, other: &PropertyGraph) -> bool {
        if self.node_order != other.node_order || self.edge_order != other.edge_order {
            return false;
        }
        if !node_map_eq(&self.nodes, &other.nodes) {
            return false;
        }
        if !edge_map_eq(&self.edges, &other.edges) {
            return false;
        }
        if !edge_tables_eq(&self.edge_tables, &other.edge_tables) {
            return false;
        }
        if self.edge_row_counts != other.edge_row_counts {
            return false;
        }
        if !locs_eq(&self.edge_row_locations, &other.edge_row_locations) {
            return false;
        }
        if !adj_eq(&self.out_adj, &other.out_adj) {
            return false;
        }
        if !adj_eq(&self.in_adj, &other.in_adj) {
            return false;
        }
        let a = self.overlay.borrow();
        let b = other.overlay.borrow();
        a.inserted_nodes == b.inserted_nodes
            && a.node_property_overrides == b.node_property_overrides
            && a.deleted_nodes == b.deleted_nodes
            && inserted_edges_eq(&a.inserted_edges, &b.inserted_edges)
            && a.edge_property_overrides == b.edge_property_overrides
            && a.deleted_edges == b.deleted_edges
            && a.inserted_node_counts == b.inserted_node_counts
            && a.inserted_edge_counts == b.inserted_edge_counts
            && a.inserted_out_adj == b.inserted_out_adj
            && a.inserted_in_adj == b.inserted_in_adj
            && a.replaced_node_properties == b.replaced_node_properties
            && a.replaced_edge_properties == b.replaced_edge_properties
            && a.inserted_node_keys == b.inserted_node_keys
            && a.override_node_keys == b.override_node_keys
            && a.inserted_edge_keys == b.inserted_edge_keys
            && a.override_edge_keys == b.override_edge_keys
    }
}

#[cfg(test)]
fn batch_eq(a: &RecordBatch, b: &RecordBatch) -> bool {
    match (encode_batch(a), encode_batch(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
fn node_map_eq(a: &HashMap<String, NodeTable>, b: &HashMap<String, NodeTable>) -> bool {
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.get(k)
                .is_some_and(|o| v.label == o.label && batch_eq(&v.batch, &o.batch))
        })
}

#[cfg(test)]
fn edge_map_eq(a: &HashMap<String, EdgeTable>, b: &HashMap<String, EdgeTable>) -> bool {
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.get(k).is_some_and(|o| {
                v.rel_type == o.rel_type
                    && v.src_label == o.src_label
                    && v.dst_label == o.dst_label
                    && batch_eq(&v.batch, &o.batch)
            })
        })
}

#[cfg(test)]
fn edge_tables_eq(
    a: &HashMap<String, Vec<EdgeTable>>,
    b: &HashMap<String, Vec<EdgeTable>>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.get(k).is_some_and(|o| {
                v.len() == o.len()
                    && v.iter().zip(o.iter()).all(|(x, y)| {
                        x.rel_type == y.rel_type
                            && x.src_label == y.src_label
                            && x.dst_label == y.dst_label
                            && batch_eq(&x.batch, &y.batch)
                    })
            })
        })
}

#[cfg(test)]
fn locs_eq(
    a: &HashMap<(String, i64), EdgeRowLocation>,
    b: &HashMap<(String, i64), EdgeRowLocation>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.get(k)
                .is_some_and(|o| v.table_index == o.table_index && v.local_row == o.local_row)
        })
}

#[cfg(test)]
fn adj_eq(
    a: &HashMap<(String, i64, String), Vec<EdgeRef>>,
    b: &HashMap<(String, i64, String), Vec<EdgeRef>>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.get(k).is_some_and(|o| {
                v.len() == o.len()
                    && v.iter().zip(o.iter()).all(|(x, y)| {
                        x.edge_row == y.edge_row
                            && x.other_label == y.other_label
                            && x.other_id == y.other_id
                    })
            })
        })
}

#[cfg(test)]
fn inserted_edges_eq(
    a: &BTreeMap<(String, i64), InsertedEdge>,
    b: &BTreeMap<(String, i64), InsertedEdge>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.get(k).is_some_and(|o| {
                v.src_label == o.src_label
                    && v.src_id == o.src_id
                    && v.dst_label == o.dst_label
                    && v.dst_id == o.dst_id
                    && v.properties == o.properties
            })
        })
}

// ---------------- tests ----------------

#[cfg(test)]
mod tests {
    use crate::ir::catalog::{NodeTable, PropertyGraph, nodes_from_columns_with_count};
    use crate::ir::value::Value;
    use crate::ir::{edges_from_columns, nodes_from_columns};
    use arrow::array::{ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use bigdecimal::BigDecimal;
    use num_bigint::BigInt;
    use std::collections::{BTreeMap, HashMap};
    use std::str::FromStr;
    use std::sync::Arc;

    fn roundtrip(g: &PropertyGraph) -> PropertyGraph {
        let bytes = g.snapshot_encode().unwrap();
        PropertyGraph::snapshot_decode(&bytes).unwrap()
    }

    #[test]
    fn roundtrip_full_graph() {
        let mut g = PropertyGraph::new();
        g.add_nodes(nodes_from_columns(
            "Person",
            vec![
                (
                    "name",
                    Arc::new(StringArray::from(vec!["alice", "bob", "carol"])) as ArrayRef,
                ),
                (
                    "age",
                    Arc::new(Int64Array::from(vec![30, 40, 50])) as ArrayRef,
                ),
            ],
        ));
        g.add_nodes(nodes_from_columns_with_count("Lonely", vec![], 2));
        g.add_edges(edges_from_columns(
            "LIKES",
            "Person",
            "Person",
            vec![0, 1],
            vec![1, 2],
            vec![],
        ))
        .unwrap();
        g.add_edges(edges_from_columns(
            "LIKES",
            "Person",
            "Lonely",
            vec![0],
            vec![0],
            vec![("since", Arc::new(Int64Array::from(vec![1999])) as ArrayRef)],
        ))
        .unwrap();
        g.add_edges(edges_from_columns(
            "KNOWS",
            "Person",
            "Person",
            vec![2],
            vec![0],
            vec![],
        ))
        .unwrap();

        let n = g.insert_node(
            "Person",
            BTreeMap::from([("name".to_string(), Value::String("dave".into()))]),
        );
        assert_eq!(
            n,
            Value::Node {
                label: "Person".into(),
                id: 3
            }
        );
        let e = g
            .insert_edge(
                "LIKES",
                &Value::Node {
                    label: "Person".into(),
                    id: 3,
                },
                &Value::Node {
                    label: "Person".into(),
                    id: 0,
                },
                BTreeMap::from([("since".to_string(), Value::Int(2020))]),
            )
            .unwrap();
        assert!(matches!(e, Value::Edge { id: 3, .. }));
        g.set_property(
            &Value::Node {
                label: "Person".into(),
                id: 3,
            },
            "age",
            Value::Int(27),
        )
        .unwrap();
        g.set_properties(
            &Value::Node {
                label: "Person".into(),
                id: 0,
            },
            BTreeMap::from([("extra".to_string(), Value::Bool(true))]),
            false,
        )
        .unwrap();
        g.set_properties(
            &Value::Node {
                label: "Person".into(),
                id: 1,
            },
            BTreeMap::from([("name".to_string(), Value::String("bobby".into()))]),
            true,
        )
        .unwrap();
        g.delete_value(
            &Value::Node {
                label: "Person".into(),
                id: 2,
            },
            true,
        )
        .unwrap();
        g.delete_value(
            &Value::Edge {
                rel_type: "KNOWS".into(),
                id: 0,
                src_label: "Person".into(),
                src_id: 2,
                dst_label: "Person".into(),
                dst_id: 0,
                projected_properties: None,
            },
            false,
        )
        .unwrap();

        let g2 = roundtrip(&g);
        assert!(g.graph_deep_eq(&g2));

        let bytes = g.snapshot_encode().unwrap();
        let bytes2 = g2.snapshot_encode().unwrap();
        assert_eq!(bytes, bytes2);

        assert_eq!(
            g2.node_label_order().to_vec(),
            vec!["Person".to_string(), "Lonely".to_string()]
        );
        assert_eq!(
            g2.edge_rel_order().to_vec(),
            vec!["LIKES".to_string(), "KNOWS".to_string()]
        );
        assert_eq!(g2.node_property("Person", 3, "age"), Value::Int(27));
        assert_eq!(
            g2.node_property("Person", 1, "name"),
            Value::String("bobby".into())
        );
        assert_eq!(g2.node_ids("Person").unwrap(), vec![0i64, 1, 3]);

        // Allocation counters continue without reusing deleted IDs.
        let n2 = g2.insert_node(
            "Person",
            BTreeMap::from([("name".to_string(), Value::String("eve".into()))]),
        );
        assert_eq!(
            n2,
            Value::Node {
                label: "Person".into(),
                id: 4
            }
        );
        let e2 = g2
            .insert_edge(
                "LIKES",
                &Value::Node {
                    label: "Person".into(),
                    id: 4,
                },
                &Value::Node {
                    label: "Person".into(),
                    id: 0,
                },
                BTreeMap::new(),
            )
            .unwrap();
        assert!(matches!(e2, Value::Edge { id: 4, .. }));
    }

    #[test]
    fn roundtrip_preserves_field_metadata() {
        let field = Field::new("born", DataType::Utf8, true).with_metadata(HashMap::from([(
            "new_graph.value_type".to_string(),
            "datetime".to_string(),
        )]));
        let schema = Arc::new(Schema::new(vec![field]));
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(StringArray::from(vec!["1990-01-01"])) as ArrayRef],
        )
        .unwrap();
        let mut g = PropertyGraph::new();
        g.add_nodes(NodeTable {
            label: "Person".to_string(),
            batch,
        });

        let g2 = roundtrip(&g);
        assert!(g.graph_deep_eq(&g2));
        // The metadata is what makes `array_value` read this as a DateTime.
        assert_eq!(
            g2.node_property("Person", 0, "born"),
            Value::DateTime("1990-01-01".into())
        );
    }

    #[test]
    fn roundtrip_all_value_variants() {
        let mut props = BTreeMap::new();
        props.insert("null".to_string(), Value::Null);
        props.insert("bool".to_string(), Value::Bool(true));
        props.insert("byte".to_string(), Value::Byte(-5));
        props.insert("uint8".to_string(), Value::UInt8(200));
        props.insert("short".to_string(), Value::Short(-1234));
        props.insert("uint16".to_string(), Value::UInt16(60_000));
        props.insert("int".to_string(), Value::Int(42));
        props.insert("uint32".to_string(), Value::UInt32(4_000_000_000));
        props.insert("long".to_string(), Value::Long(9_000_000_000_i64));
        props.insert(
            "uint64".to_string(),
            Value::UInt64(18_000_000_000_000_000_000),
        );
        props.insert("float32".to_string(), Value::Float32(1.5));
        props.insert("float".to_string(), Value::Float(2.25));
        props.insert(
            "bigint".to_string(),
            Value::BigInt(BigInt::from(123_456_789_012_345_678_901_234_567_890u128)),
        );
        props.insert(
            "uint128".to_string(),
            Value::UInt128(BigInt::from(
                340_282_366_920_938_463_463_374_607_431_768_211_455u128,
            )),
        );
        props.insert(
            "bigdecimal".to_string(),
            Value::BigDecimal(BigDecimal::from_str("3.14159265358979323846").unwrap()),
        );
        props.insert(
            "datetime".to_string(),
            Value::DateTime("2020-01-01T00:00:00".to_string()),
        );
        props.insert(
            "internalid".to_string(),
            Value::InternalId {
                table: 7,
                offset: 9,
            },
        );
        props.insert("string".to_string(), Value::String("hello".to_string()));
        props.insert(
            "node".to_string(),
            Value::Node {
                label: "X".to_string(),
                id: 3,
            },
        );
        props.insert(
            "edge".to_string(),
            Value::Edge {
                rel_type: "E".to_string(),
                id: 1,
                src_label: "X".to_string(),
                src_id: 0,
                dst_label: "X".to_string(),
                dst_id: 3,
                projected_properties: Some(vec!["p".to_string()]),
            },
        );
        props.insert(
            "list".to_string(),
            Value::List(vec![Value::Int(1), Value::String("a".to_string())]),
        );
        props.insert(
            "map".to_string(),
            Value::Map(BTreeMap::from([("k".to_string(), Value::Int(9))])),
        );
        props.insert(
            "path".to_string(),
            Value::Path(vec![
                Value::Node {
                    label: "X".to_string(),
                    id: 0,
                },
                Value::String("step".to_string()),
            ]),
        );

        let mut g = PropertyGraph::new();
        g.insert_node("Rich", props);

        let g2 = roundtrip(&g);
        assert!(g.graph_deep_eq(&g2));

        let original = g.node_property("Rich", 0, "bigdecimal");
        let restored = g2.node_property("Rich", 0, "bigdecimal");
        assert_eq!(original, restored);
        assert_eq!(
            restored,
            Value::BigDecimal(BigDecimal::from_str("3.14159265358979323846").unwrap())
        );
    }

    #[test]
    fn roundtrip_multitype_edge_groups_preserve_endpoints() {
        let mut g = PropertyGraph::new();
        g.add_nodes(nodes_from_columns(
            "A",
            vec![("x", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef)],
        ));
        g.add_nodes(nodes_from_columns(
            "B",
            vec![("y", Arc::new(Float64Array::from(vec![1.0])) as ArrayRef)],
        ));
        g.add_edges(edges_from_columns("T", "A", "A", vec![0], vec![1], vec![]))
            .unwrap();
        g.add_edges(edges_from_columns("T", "A", "B", vec![1], vec![0], vec![]))
            .unwrap();

        let g2 = roundtrip(&g);
        assert!(g.graph_deep_eq(&g2));
        assert_eq!(
            g2.edge_endpoints("T", 0),
            Some(("A".to_string(), 0, "A".to_string(), 1))
        );
        assert_eq!(
            g2.edge_endpoints("T", 1),
            Some(("A".to_string(), 1, "B".to_string(), 0))
        );
        assert_eq!(g2.edge_ids("T"), vec![0i64, 1]);
    }

    #[test]
    fn corrupt_data_is_rejected() {
        let mut g = PropertyGraph::new();
        g.add_nodes(nodes_from_columns(
            "P",
            vec![("x", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
        ));
        g.insert_node("P", BTreeMap::from([("k".to_string(), Value::Int(1))]));
        let bytes = g.snapshot_encode().unwrap();

        assert!(PropertyGraph::snapshot_decode(&[]).is_err());
        assert!(PropertyGraph::snapshot_decode(&[0u8; 32]).is_err());

        let mut bad_magic = bytes.clone();
        bad_magic[0] ^= 0xFF;
        assert!(PropertyGraph::snapshot_decode(&bad_magic).is_err());

        let mut bad_version = bytes.clone();
        bad_version[4] = 99;
        assert!(PropertyGraph::snapshot_decode(&bad_version).is_err());

        // Truncation within the trailing (overlay) section framing.
        for cut in 1..=8 {
            assert!(
                PropertyGraph::snapshot_decode(&bytes[..bytes.len() - cut]).is_err(),
                "cut {cut} bytes should fail"
            );
        }

        // A corrupt length prefix in a section should be rejected.
        let mut bad_len = bytes.clone();
        // The first section length starts after the 12-byte header and tag.
        // Flipping arbitrary payload bytes could simply produce a valid value.
        bad_len[13..21].fill(0xFF);
        assert!(PropertyGraph::snapshot_decode(&bad_len).is_err());
        assert!(PropertyGraph::snapshot_decode(&bytes[..12]).is_err());
    }
}
