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
use std::sync::Arc;

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
    (0x10..=0x22).contains(&tag)
}

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
        let mut edge_row_locations = HashMap::new();
        self.edge_row_counts.clear();
        let mut out_adj: HashMap<_, Vec<EdgeRef>> = HashMap::new();
        let mut in_adj: HashMap<_, Vec<EdgeRef>> = HashMap::new();

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
                    out_adj
                        .entry((table.src_label.clone(), s, rel_type.clone()))
                        .or_default()
                        .push(EdgeRef {
                            edge_row: global_row,
                            other_label: table.dst_label.clone(),
                            other_id: d,
                        });
                    in_adj
                        .entry((table.dst_label.clone(), d, rel_type.clone()))
                        .or_default()
                        .push(EdgeRef {
                            edge_row: global_row,
                            other_label: table.src_label.clone(),
                            other_id: s,
                        });
                    edge_row_locations.insert(
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
        self.edge_row_locations = Arc::new(edge_row_locations);
        self.out_adj = Arc::new(out_adj);
        self.in_adj = Arc::new(in_adj);
    }
}

mod binary;
mod sections;

use binary::*;
use sections::*;
pub(super) use binary::{decode_value_bytes, encode_value_bytes};

#[cfg(test)]
mod comparison;
#[cfg(test)]
mod tests;
