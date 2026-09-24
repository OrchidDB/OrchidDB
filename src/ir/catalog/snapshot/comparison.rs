//! Test-only deep comparison of graph snapshots.

use super::*;

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
        a.vertex_properties == b.vertex_properties
            && a.next_property_id == b.next_property_id
            && a.public_ids == b.public_ids
            && a.inserted_nodes == b.inserted_nodes
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
