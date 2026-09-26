//! Lossless snapshot serialization for [`PropertyGraph`].
//!
//! Base Arrow tables are encoded with the Arrow IPC stream format, which
//! preserves the full schema (including per-field metadata such as
//! `orchiddb.value_type`) and every physical array type. The session
//! overlay — which lives in Rust-side maps rather than Arrow batches — is
//! encoded with a self-describing binary format in which every value is
//! prefixed by a tag so that each [`Value`] variant round-trips exactly.
//!
//! The wire format is versioned and length-delimited so corrupt or truncated
//! input is rejected rather than silently mis-parsed.

use crate::ir::ElementId;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use arrow::array::RecordBatch;
use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use bigdecimal::BigDecimal;
use num_bigint::BigInt;

use super::PropertyGraph;
use super::{EdgeTable, GraphOverlay, InsertedEdge, NodeTable};
#[cfg(test)]
use super::{EdgeRef,EdgeRowLocation};
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
    (0x10..=0x24).contains(&tag)
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
        let keys = |values: &HashMap<String,Vec<ElementId>>| Value::Map(values.iter().map(|(name,ids)| (name.clone(), Value::List(ids.iter().map(|id|Value::Scalar(id.scalar().clone())).collect()))).collect());
        write_section(&mut out, 7, &encode_value_bytes(&Value::List(vec![keys(&self.node_keys),keys(&self.edge_keys)])));
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
                SEC_OVERLAY => graph.overlay = super::SnapshotCell::new(parse_overlay(payload)?),
                7 => {
                    let Value::List(groups)=decode_value_bytes(payload)? else {return Err("invalid key section".into());};
                    if groups.len()!=2 {return Err("invalid key groups".into());}
                    let parse=|value: &Value| -> Result<HashMap<String,Vec<ElementId>>,String> {
                        let Value::Map(map)=value else{return Err("invalid key map".into());};
                        map.iter().map(|(name,values)| {
                            let Value::List(values)=values else{return Err("invalid key list".into());};
                            Ok((name.clone(),values.iter().map(ElementId::try_from).collect::<Result<Vec<_>,_>>()?))
                        }).collect()
                    };
                    graph.node_keys=parse(&groups[0])?;
                    graph.edge_keys=parse(&groups[1])?;
                }
                _ => return Err(format!("unknown snapshot section {tag}")),
            }
        }
        if !(SEC_NODE_ORDER..=SEC_OVERLAY).all(|tag|seen.contains(&tag)) {
            return Err("missing required graph snapshot section".into());
        }
        graph.rebuild_edge_indices()?;
        Ok(graph)
    }

    /// Rebuild the derived edge caches (`edge_row_locations`,
    /// `edge_row_counts`, `out_adj`, `in_adj`) from the grouped base tables.
    /// Mirrors the indexing performed by `add_edges` so row numbering and
    /// adjacency are identical to the original graph.
    fn rebuild_edge_indices(&mut self) -> Result<(),String> {
        let nodes = self.nodes.values().cloned().collect::<Vec<_>>();
        for table in nodes {
            let ids=self.node_keys.remove(&table.label).unwrap_or_else(||(0..table.batch.num_rows()).map(|r|(r as i64).into()).collect());
            self.add_keyed_nodes(table,ids).map_err(|e|e.to_string())?;
        }
        let edge_keys=std::mem::take(&mut self.edge_keys);
        let tables=std::mem::take(&mut self.edge_tables);
        self.edges.clear(); self.edge_row_counts.clear();
        for (name,group) in tables {
            let mut offset=0;
            for table in group {
                let count=table.batch.num_rows();
                let ids=match edge_keys.get(&name) {
                    Some(ids)=>ids.get(offset..offset+count).ok_or("invalid edge key count")?.to_vec(),
                    None=>(offset..offset+count).map(|r|(r as i64).into()).collect(),
                };
                self.add_keyed_edges(table,ids).map_err(|e|e.to_string())?;
                offset+=count;
            }
            if edge_keys.get(&name).is_some_and(|ids|ids.len()!=offset){return Err("invalid edge key count".into());}
        }
        Ok(())
    }

}

pub(crate) mod binary;
mod sections;

use binary::*;
use sections::*;
pub(super) use binary::{decode_value_bytes, encode_value_bytes};

#[cfg(test)]
mod comparison;
#[cfg(test)]
mod tests;
