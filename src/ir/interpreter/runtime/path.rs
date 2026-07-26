//! Runtime path helpers — path_or_self, slice, by-key projection.
//!
//! Extracted from `interpreter.rs` lines 2795..2845.

use std::collections::BTreeMap;

use crate::ir::catalog::PropertyGraph;
use crate::ir::value::Value;

pub(crate) fn slice_path_at(items: &[Value], label: &str, from_label: bool) -> Value {
    slice_path_at_value(items, label, None, from_label)
}

pub(crate) fn slice_path_at_value(
    items: &[Value],
    label: &str,
    labelled_value: Option<&Value>,
    from_label: bool,
) -> Value {
    let position = labelled_value
        .and_then(|value| {
            if from_label {
                items.iter().position(|item| item == value)
            } else {
                items.iter().rposition(|item| item == value)
            }
        })
        .or_else(|| {
            items.iter().position(|item| match item {
                Value::Node { label: l, .. } => l == label,
                Value::Edge { rel_type, .. } => rel_type == label,
                _ => false,
            })
        });
    match position {
        Some(idx) if from_label => Value::Path(items[idx..].to_vec()),
        Some(idx) => Value::Path(items[..=idx].to_vec()),
        None => Value::Path(items.to_vec()),
    }
}

pub(crate) fn apply_path_by_keys(
    items: &[Value],
    keys: &[Value],
    graph: &PropertyGraph,
) -> Option<Vec<Value>> {
    apply_path_by_keys_impl(items, keys, graph, true)
}

pub(crate) fn apply_path_by_keys_keep_nulls(
    items: &[Value],
    keys: &[Value],
    graph: &PropertyGraph,
) -> Vec<Value> {
    apply_path_by_keys_impl(items, keys, graph, false).unwrap_or_default()
}

fn apply_path_by_keys_impl(
    items: &[Value],
    keys: &[Value],
    graph: &PropertyGraph,
    drop_nulls: bool,
) -> Option<Vec<Value>> {
    if keys.is_empty() {
        return Some(items.to_vec());
    }
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let projected = match &keys[idx % keys.len()] {
            Value::String(key) => project_value_by_key(item, key, graph),
            Value::Null => item.clone(),
            _ => item.clone(),
        };
        if drop_nulls && matches!(projected, Value::Null) {
            return None;
        }
        out.push(projected);
    }
    Some(out)
}

pub(crate) fn path_pairs(items: &[Value]) -> Value {
    let pairs = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            if idx % 2 == 0 || !matches!(item, Value::Edge { .. }) {
                return None;
            }
            let mut pair = BTreeMap::new();
            pair.insert("r".to_string(), item.clone());
            pair.insert(
                "n".to_string(),
                items.get(idx + 1).cloned().unwrap_or(Value::Null),
            );
            Some(Value::Map(pair))
        })
        .collect();
    Value::List(pairs)
}

/// Pairs of (rel, node) restricted to intermediate nodes: the final
/// pair — whose node is the destination endpoint — is dropped, matching
/// recursive-relationship node predicates that only see interior nodes.
pub(crate) fn path_intermediate_pairs(items: &[Value]) -> Value {
    match path_pairs(items) {
        Value::List(mut pairs) => {
            pairs.pop();
            Value::List(pairs)
        }
        other => other,
    }
}

pub(crate) fn project_path_edges(items: &[Value], rel_keys: &[Value]) -> Value {
    let keys = rel_keys
        .iter()
        .filter_map(|value| match value {
            Value::String(key) => Some(key.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    Value::Path(
        items
            .iter()
            .map(|item| match item {
                Value::Edge {
                    rel_type,
                    id,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    ..
                } => Value::Edge {
                    rel_type: rel_type.clone(),
                    id: *id,
                    src_label: src_label.clone(),
                    src_id: *src_id,
                    dst_label: dst_label.clone(),
                    dst_id: *dst_id,
                    projected_properties: Some(keys.clone()),
                },
                other => other.clone(),
            })
            .collect(),
    )
}

pub(crate) fn project_value_by_key(value: &Value, key: &str, graph: &PropertyGraph) -> Value {
    if let Some(inner) = key.strip_prefix("__upper:") {
        return match project_value_by_key(value, inner, graph) {
            Value::String(s) => Value::String(s.to_uppercase()),
            other => other,
        };
    }
    if let Some(inner) = key.strip_prefix("__lower:") {
        return match project_value_by_key(value, inner, graph) {
            Value::String(s) => Value::String(s.to_lowercase()),
            other => other,
        };
    }
    match value {
        Value::Node { label, .. } if key == "__label" => Value::String(label.clone()),
        Value::Edge { rel_type, .. } if key == "__label" => Value::String(rel_type.clone()),
        Value::Node { id, .. } | Value::Edge { id, .. } if key == "__id" => Value::Int(*id),
        Value::Node { label, id } => graph.node_property(label, *id, key),
        Value::Edge { rel_type, id, .. } => graph.edge_property(rel_type, *id, key),
        Value::Map(map) => map.get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}
