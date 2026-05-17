//! Shared Cypher/Kuzu element-id helpers.

use crate::ir::catalog::PropertyGraph;
use crate::ir::value::Value;

pub(crate) fn node_table_index(graph: &PropertyGraph, label: &str) -> i64 {
    graph
        .node_label_order()
        .iter()
        .position(|candidate| candidate == label)
        .map(|idx| idx as i64)
        .unwrap_or(0)
}

pub(crate) fn edge_table_index(graph: &PropertyGraph, rel_type: &str) -> i64 {
    // Kuzu numbers relationship table IDs in the same namespace as
    // node tables, and reserves a second internal slot per rel type for
    // the reverse adjacency table. User-visible IDs therefore advance
    // by two for each relationship type in schema order.
    let node_count = graph.node_label_order().len() as i64;
    graph
        .edge_rel_order()
        .iter()
        .position(|candidate| candidate == rel_type)
        .map(|idx| node_count + (idx as i64 * 2))
        .unwrap_or(node_count)
}

pub(crate) fn element_internal_id(graph: &PropertyGraph, value: &Value) -> Option<Value> {
    match value {
        Value::Node { label, id } => Some(Value::InternalId {
            table: node_table_index(graph, label),
            offset: *id,
        }),
        Value::Edge { rel_type, id, .. } => Some(Value::InternalId {
            table: edge_table_index(graph, rel_type),
            offset: *id,
        }),
        Value::InternalId { .. } => Some(value.clone()),
        _ => None,
    }
}
