//! Durable, lossless serialization for the in-memory property graph.
//!
//! `encode_graph` / `decode_graph` produce and consume a self-contained byte
//! blob that captures the full graph state — base Arrow tables (including
//! grouped edge tables and per-field metadata) and the session overlay
//! (inserted elements, property overrides, deletions, and allocation
//! counters) — so a decoded graph behaves identically to the original and
//! continues allocating element ids without reuse.

use crate::ir::catalog::PropertyGraph;
use sha2::{Digest, Sha256};

/// Serialize `graph` into a self-contained, versioned byte vector.
pub fn encode_graph(graph: &PropertyGraph) -> Result<Vec<u8>, String> {
    let mut payload = graph.snapshot_encode()?;
    let checksum = Sha256::digest(&payload);
    payload.extend_from_slice(&checksum);
    Ok(payload)
}

/// Reconstruct a [`PropertyGraph`] from [`encode_graph`] output.
pub fn decode_graph(data: &[u8]) -> Result<PropertyGraph, String> {
    if data.len() < 32 {
        return Err("truncated graph snapshot".into());
    }
    let (payload, checksum) = data.split_at(data.len() - 32);
    if Sha256::digest(payload).as_slice() != checksum {
        return Err("graph snapshot checksum mismatch".into());
    }
    PropertyGraph::snapshot_decode(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::value::Value;
    use crate::ir::{edges_from_columns, nodes_from_columns};
    use arrow::array::{ArrayRef, Int64Array, StringArray};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[test]
    fn roundtrip_and_corruption_via_public_api() {
        let mut g = PropertyGraph::new();
        g.add_nodes(nodes_from_columns(
            "Person",
            vec![(
                "name",
                Arc::new(StringArray::from(vec!["a", "b"])) as ArrayRef,
            )],
        ));
        g.add_edges(edges_from_columns(
            "LIKES",
            "Person",
            "Person",
            vec![0],
            vec![1],
            vec![("w", Arc::new(Int64Array::from(vec![5])) as ArrayRef)],
        ))
        .unwrap();
        g.insert_node(
            "Person",
            BTreeMap::from([("name".to_string(), Value::String("c".into()))]),
        );
        g.delete_value(
            &Value::Node {
                label: "Person".into(),
                id: 1,
            },
            true,
        )
        .unwrap();

        let bytes = encode_graph(&g).unwrap();
        let g2 = decode_graph(&bytes).unwrap();
        assert!(g.graph_deep_eq(&g2));

        assert!(decode_graph(&[]).is_err());
        assert!(decode_graph(&[0u8; 16]).is_err());
        assert!(decode_graph(&bytes[..bytes.len() - 1]).is_err());
        let mut corrupted = bytes.clone();
        let middle = corrupted.len() / 2;
        corrupted[middle] ^= 1;
        assert!(decode_graph(&corrupted).is_err());
    }
}
