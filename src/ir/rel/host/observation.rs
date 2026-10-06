//! Exact graph state observation shared by every execution host.
use crate::ir::runtime::output::gremlin_typed_value;
use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY};
use crate::ir::{catalog::PropertyGraph, value::Value};
use serde::Serialize;
use std::collections::BTreeSet;
/// The same versioned, typed rows exposed by Cypher result transport. Consumers
/// compare sets of rows; properties include their value, so replacement counts
/// as one removed and one added property. This observes state, not write counts.
#[derive(Debug, Clone, Serialize)]
pub struct CypherStateSnapshot {
    pub nodes: Vec<Vec<serde_json::Value>>,
    pub relationships: Vec<Vec<serde_json::Value>>,
    pub labels: Vec<Vec<serde_json::Value>>,
    pub node_properties: Vec<Vec<serde_json::Value>>,
    pub edge_properties: Vec<Vec<serde_json::Value>>,
}
pub type QualifiedIdentities =
    std::collections::BTreeMap<(bool, String, crate::ir::ElementId), Value>;
pub fn observe(
    graph: &PropertyGraph,
    qualified: Option<&QualifiedIdentities>,
) -> Result<CypherStateSnapshot, String> {
    let mut result = CypherStateSnapshot {
        nodes: vec![],
        relationships: vec![],
        labels: vec![],
        node_properties: vec![],
        edge_properties: vec![],
    };
    let encode = |value: &Value| gremlin_typed_value(value, graph);
    let identity = |element: &Value| {
        graph.source_identity(element).unwrap_or_else(|| {
            graph
                .cypher_id(element)
                .map(Value::Int)
                .unwrap_or(Value::Null)
        })
    };
    let mut labels = BTreeSet::new();
    for label in graph.labels() {
        let ids = graph.node_ids(&label).map_err(|e| e.to_string())?;
        let nodes = ids
            .iter()
            .map(|id| Value::Node {
                label: label.clone(),
                id: id.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(source) = &graph.source {
            source.prefetch(&mut nodes.iter());
        }
        let keys = graph.node_property_keys_with_id(&label);
        for node in nodes {
            let Value::Node { id, .. } = &node else {
                unreachable!()
            };
            let identity = encode(&identity(&node));
            let element_identity = qualified
                .and_then(|ids| ids.get(&(false, label.clone(), id.clone())))
                .map(&encode)
                .unwrap_or_else(|| identity.clone());
            result.nodes.push(vec![element_identity]);
            labels.extend(graph.node_labels(&label, id.clone()));
            for key in &keys {
                if matches!(key.as_str(), STRUCT_ORDER_KEY | STRUCT_TYPES_KEY) {
                    continue;
                }
                let value = graph.node_property(&label, id.clone(), key);
                if value != Value::Null {
                    result.node_properties.push(vec![
                        identity.clone(),
                        encode(&Value::String(key.clone())),
                        encode(&value),
                    ]);
                }
            }
        }
    }
    result.labels = labels
        .into_iter()
        .map(|label| vec![encode(&Value::String(label))])
        .collect();
    for rel_type in graph.rel_types() {
        let ids = graph.edge_ids(&rel_type);
        let keys = graph.edge_property_keys(&rel_type);
        for id in ids {
            let Some((src_label, src_id, dst_label, dst_id)) =
                graph.edge_endpoints(&rel_type, id.clone())
            else {
                continue;
            };
            // MATCH ()-[r]->() only observes relationships with live endpoints.
            if !graph.node_is_live(&src_label, src_id.clone())
                || !graph.node_is_live(&dst_label, dst_id.clone())
            {
                continue;
            }
            let edge = Value::Edge {
                rel_type: rel_type.clone(),
                id: id.clone(),
                src_label,
                src_id,
                dst_label,
                dst_id,
                projected_properties: None,
            };
            let identity = encode(&identity(&edge));
            let element_identity = qualified
                .and_then(|ids| ids.get(&(true, rel_type.clone(), id.clone())))
                .map(&encode)
                .unwrap_or_else(|| identity.clone());
            result.relationships.push(vec![element_identity]);
            for key in &keys {
                if matches!(key.as_str(), STRUCT_ORDER_KEY | STRUCT_TYPES_KEY) {
                    continue;
                }
                let value = graph.edge_property(&rel_type, id.clone(), key);
                if value != Value::Null {
                    result.edge_properties.push(vec![
                        identity.clone(),
                        encode(&Value::String(key.clone())),
                        encode(&value),
                    ]);
                }
            }
        }
    }
    graph.check_source()?;
    Ok(result)
}
