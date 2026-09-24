//! value_map / element_map / property_map / properties_list.
//!
//! Extracted from `interpreter.rs` lines 2708..2793.

use crate::ir::catalog::PropertyGraph;
use crate::ir::value::{STRUCT_ORDER_KEY, Value};

pub(crate) fn eval_property_object(name: &str, args: &[Value], graph: &PropertyGraph) -> Value {
    let target = args.first().cloned().unwrap_or(Value::Null);
    let mut keys = Vec::new();
    if let Some(Value::List(requested)) = args.get(1) {
        for key in requested {
            if let Value::String(key) = key {
                if !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
        }
    }
    let properties = graph.properties(&target, &keys);
    if name == "properties_list" {
        return Value::List(properties);
    }
    if !matches!(
        target,
        Value::Node { .. } | Value::Edge { .. } | Value::VertexProperty { .. }
    ) {
        return Value::Null;
    }
    let single = !matches!(target, Value::Node { .. })
        || name == "element_map"
        || bool_arg(args.get(4), false);
    let mut map = std::collections::BTreeMap::new();
    let mut observed_keys = Vec::new();
    for property in properties {
        let (key, value) = match &property {
            Value::VertexProperty { key, value, .. } | Value::Property { key, value, .. } => {
                (key.clone(), value.as_ref().clone())
            }
            _ => continue,
        };
        if !observed_keys.contains(&key) {
            observed_keys.push(key.clone());
        }
        let value = if name == "property_map" {
            property
        } else {
            value
        };
        // Cardinality comes from native record count. A list-valued record is
        // one value and remains nested inside a vertex valueMap's outer list.
        if single {
            map.entry(key).or_insert(value);
        } else {
            let entry = map.entry(key).or_insert_with(|| Value::List(vec![]));
            if let Value::List(items) = entry {
                items.push(value);
            }
        }
    }
    // Preserve requested order even for providers whose properties iterator
    // is key-sorted (e.g. meta-properties), and insertion order when unfiltered.
    let mut ordered_keys = keys
        .into_iter()
        .filter(|key| map.contains_key(key))
        .collect::<Vec<_>>();
    for key in observed_keys {
        if !ordered_keys.contains(&key) {
            ordered_keys.push(key);
        }
    }
    if name == "element_map" || name == "value_map_tokens" {
        let mut entries = Vec::new();
        if bool_arg(args.get(2), true) {
            entries.push((Value::Token("id".into()), graph.element_public_id(&target)));
        }
        if name == "element_map" {
            if let Value::VertexProperty { key, value, .. } = &target {
                entries.push((Value::Token("key".into()), Value::String(key.clone())));
                entries.push((Value::Token("value".into()), value.as_ref().clone()));
            } else if bool_arg(args.get(3), true) {
                let label = match &target {
                    Value::Node { label, .. } => label,
                    Value::Edge { rel_type, .. } => rel_type,
                    _ => unreachable!(),
                };
                entries.push((Value::Token("label".into()), Value::String(label.clone())));
            }
            add_endpoint_tokens(&mut entries, &target, graph);
        } else if bool_arg(args.get(3), true) {
            let label = match &target {
                Value::Node { label, .. } => label,
                Value::Edge { rel_type, .. } => rel_type,
                Value::VertexProperty { key, .. } => key,
                _ => unreachable!(),
            };
            entries.push((Value::Token("label".into()), Value::String(label.clone())));
        }
        for key in ordered_keys {
            if let Some(value) = map.remove(&key) {
                entries.push((Value::String(key), value));
            }
        }
        Value::TypedMap(entries)
    } else {
        if name == "value_map" && !ordered_keys.is_empty() {
            map.insert(
                STRUCT_ORDER_KEY.into(),
                Value::List(ordered_keys.into_iter().map(Value::String).collect()),
            );
        }
        Value::Map(map)
    }
}

pub(crate) fn requested_property_values(
    target: &Value,
    keys: &[String],
    graph: &PropertyGraph,
) -> Value {
    if matches!(target, Value::Map(_) | Value::TypedMap(_)) {
        let entries = ordered_value_map_entries(target);
        return Value::List(if keys.is_empty() {
            entries.into_iter().map(|(_, value)| value).collect()
        } else {
            keys.iter()
                .filter_map(|key| {
                    entries
                        .iter()
                        .find(|(candidate, _)| candidate == &Value::String(key.clone()))
                        .map(|(_, value)| value.clone())
                })
                .collect()
        });
    }
    Value::List(
        graph
            .properties(target, keys)
            .into_iter()
            .filter_map(|property| match property {
                Value::VertexProperty { value, .. } | Value::Property { value, .. } => Some(*value),
                _ => None,
            })
            .collect(),
    )
}

fn bool_arg(value: Option<&Value>, default: bool) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        _ => default,
    }
}

pub(crate) fn eval_property_element(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::VertexProperty { owner, .. } | Value::Property { owner, .. }) => {
            owner.as_ref().clone()
        }
        _ => Value::Null,
    }
}

fn add_endpoint_tokens(entries: &mut Vec<(Value, Value)>, value: &Value, graph: &PropertyGraph) {
    if let Value::Edge {
        src_label,
        src_id,
        dst_label,
        dst_id,
        ..
    } = value
    {
        entries.push((
            Value::Direction("OUT".into()),
            endpoint_token(graph, src_label, *src_id),
        ));
        entries.push((
            Value::Direction("IN".into()),
            endpoint_token(graph, dst_label, *dst_id),
        ));
    }
}

fn endpoint_token(graph: &PropertyGraph, label: &str, id: i64) -> Value {
    let node = Value::Node {
        label: label.to_string(),
        id,
    };
    Value::TypedMap(vec![
        (
            Value::Token("id".into()),
            super::graph::gremlin_user_id(graph, &node),
        ),
        (
            Value::Token("label".into()),
            Value::String(label.to_string()),
        ),
    ])
}

/// Enumerate the map in traversal-ring order without exposing order metadata.
fn ordered_value_map_entries(value: &Value) -> Vec<(Value, Value)> {
    match value {
        Value::TypedMap(entries) => entries.clone(),
        Value::Map(map) => {
            let mut keys = Vec::new();
            if let Some(Value::List(order)) = map.get(STRUCT_ORDER_KEY) {
                for key in order {
                    if let Value::String(key) = key {
                        if map.contains_key(key) && !keys.contains(key) {
                            keys.push(key.clone());
                        }
                    }
                }
            }
            for key in map.keys() {
                if key != STRUCT_ORDER_KEY && !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
            keys.into_iter()
                .map(|key| (Value::String(key.clone()), map[&key].clone()))
                .collect()
        }
        _ => Vec::new(),
    }
}

pub(super) fn value_map_modulator_entries(args: &[Value]) -> Value {
    let [map, Value::Int(slot), Value::Int(count)] = args else {
        return Value::List(vec![]);
    };
    if *count <= 0 {
        return Value::List(vec![]);
    }
    Value::List(
        ordered_value_map_entries(map)
            .into_iter()
            .enumerate()
            .filter(|(index, _)| *index as i64 % count == *slot)
            .map(|(_, (key, value))| {
                Value::Map([("key".to_string(), key), ("value".to_string(), value)].into())
            })
            .collect(),
    )
}

pub(super) fn value_map_modulator_result(args: &[Value]) -> Value {
    let [original, Value::List(results)] = args else {
        return Value::Null;
    };
    let entries = ordered_value_map_entries(original)
        .into_iter()
        .filter_map(|(key, _)| {
            results.iter().find_map(|result| match result {
                Value::List(pair)
                    if pair.len() == 3 && pair[0] == key && pair[2] == Value::Bool(true) =>
                {
                    Some((key.clone(), pair[1].clone()))
                }
                _ => None,
            })
        })
        .collect::<Vec<_>>();
    if matches!(original, Value::TypedMap(_)) {
        Value::TypedMap(entries)
    } else {
        let order = entries
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut map = entries
            .into_iter()
            .filter_map(|(key, value)| match key {
                Value::String(key) => Some((key, value)),
                _ => None,
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        if !order.is_empty() {
            map.insert(STRUCT_ORDER_KEY.to_string(), Value::List(order));
        }
        Value::Map(map)
    }
}
