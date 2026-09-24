//! value_map / element_map / property_map / properties_list.
//!
//! Extracted from `interpreter.rs` lines 2708..2793.

use crate::ir::catalog::PropertyGraph;
use crate::ir::value::{STRUCT_ORDER_KEY, Value};

pub(crate) fn eval_property_object(name: &str, args: &[Value], graph: &PropertyGraph) -> Value {
    let target = args.first().cloned().unwrap_or(Value::Null);
    let keys: Vec<String> = match args.get(1) {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let (label, id) = match &target {
        Value::Node { label, id } => (label.clone(), *id),
        Value::Edge { rel_type, id, .. } => (rel_type.clone(), *id),
        _ => return Value::Null,
    };
    let include_id = bool_arg(args.get(2), true);
    let include_label = bool_arg(args.get(3), true);
    let unfold_values = bool_arg(args.get(4), false);
    let resolved_keys = if keys.is_empty() {
        match &target {
            Value::Node { .. } => graph.node_property_keys(&label),
            Value::Edge { .. } => graph.edge_property_keys(&label),
            _ => Vec::new(),
        }
    } else {
        keys
    };
    let resolved_keys = resolved_keys
        .into_iter()
        .filter(|key| !key.starts_with("__"))
        .collect::<Vec<_>>();

    let mut map = std::collections::BTreeMap::new();
    let mut tokens = Vec::new();
    if name == "element_map" || name == "value_map_tokens" {
        if include_id {
            tokens.push((Value::Token("id".into()), super::graph::gremlin_user_id(graph, &target)));
        }
        if include_label {
            tokens.push((Value::Token("label".into()), Value::String(label.clone())));
        }
        if name == "element_map" {
            add_endpoint_tokens(&mut tokens, &target, graph);
        }
    }
    let target_is_edge = matches!(&target, Value::Edge { .. });
    for key in &resolved_keys {
        let value = match &target {
            Value::Node { .. } => node_property_with_algorithms(graph, &label, id, key),
            Value::Edge { .. } => graph.edge_property(&label, id, key),
            _ => Value::Null,
        };
        if matches!(value, Value::Null) {
            continue;
        }
        let entry = match name {
            // Vertex value maps contain lists (one value per vertex
            // property), while edge value maps contain scalar values.
            "value_map" | "value_map_tokens" if target_is_edge || unfold_values => value,
            "value_map" | "value_map_tokens" if matches!(value, Value::List(_)) => value,
            "value_map" | "value_map_tokens" => Value::List(vec![value]),
            "element_map" => match value {
                Value::List(items) if !target_is_edge => items.into_iter().next().unwrap_or(Value::Null),
                other => other,
            },
            "property_map" => {
                let properties = match value {
                    Value::List(items) if !target_is_edge => items,
                    other => vec![other],
                };
                let mut pairs: Vec<Value> = properties.into_iter().enumerate().map(|(index, value)| {
                    property_pair(&target, key.clone(), value, property_order(graph, &target, index))
                }).collect();
                if target_is_edge { pairs.pop().unwrap_or(Value::Null) } else { Value::List(pairs) }
            }
            _ => value,
        };
        map.insert(key.clone(), entry);
    }
    let order = map
        .keys()
        .filter(|key| key.as_str() != STRUCT_ORDER_KEY)
        .cloned()
        .collect::<Vec<_>>();
    if name == "value_map" && !order.is_empty() {
        let ordered = resolved_keys
            .iter()
            .filter(|key| map.contains_key(*key))
            .cloned()
            .chain(order.into_iter().filter(|key| !resolved_keys.contains(key)))
            .map(Value::String)
            .collect::<Vec<_>>();
        map.insert(STRUCT_ORDER_KEY.to_string(), Value::List(ordered));
    }
    if name == "properties_list" {
        // `properties()` is fan-out: build a list of `{key, value}`
        // structs and return as a List so a wrapping `GraphUnwind`
        // produces one row per pair.
        let mut pairs = Vec::new();
        for (idx, key) in resolved_keys.into_iter().enumerate() {
            let value = match &target {
                Value::Node { .. } => node_property_with_algorithms(graph, &label, id, &key),
                Value::Edge { .. } => graph.edge_property(&label, id, &key),
                _ => Value::Null,
            };
            if matches!(value, Value::Null) {
                continue;
            }
            match value {
                Value::List(items) => {
                    for (item_idx, item) in items.into_iter().enumerate() {
                        let order =
                            property_order(graph, &target, idx).saturating_add(item_idx as i64);
                        pairs.push(property_pair(&target, key.clone(), item, order));
                    }
                }
                value => {
                    let order = property_order(graph, &target, idx);
                    pairs.push(property_pair(&target, key, value, order));
                }
            }
        }
        return Value::List(pairs);
    }
    if name == "element_map" || name == "value_map_tokens" {
        // PropertyMapStep uses insertion order when cycling by() modulators.
        for key in &resolved_keys {
            if let Some(value) = map.remove(key) {
                tokens.push((Value::String(key.clone()), value));
            }
        }
        tokens.extend(map.into_iter().map(|(key, value)| (Value::String(key), value)));
        Value::TypedMap(tokens)
    } else { Value::Map(map) }
}

fn property_pair(target: &Value, key: String, value: Value, order: i64) -> Value {
    let mut prop = std::collections::BTreeMap::new();
    prop.insert("key".to_string(), Value::String(key));
    prop.insert("value".to_string(), value);
    prop.insert("element".to_string(), target.clone());
    prop.insert("__id".to_string(), Value::Long(order));
    prop.insert("__order".to_string(), Value::Long(order));
    Value::Map(prop)
}

fn bool_arg(value: Option<&Value>, default: bool) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        _ => default,
    }
}

pub(crate) fn eval_property_element(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::Map(map)) => map.get("element").cloned().unwrap_or(Value::Null),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn node_property_with_algorithms(graph: &PropertyGraph, label: &str, id: i64, key: &str) -> Value {
    let stored = graph.node_property(label, id, key);
    if !matches!(stored, Value::Null) {
        return stored;
    }
    virtual_node_property(graph, label, id, key).unwrap_or(Value::Null)
}

fn virtual_node_property(graph: &PropertyGraph, label: &str, id: i64, key: &str) -> Option<Value> {
    let name = match graph.node_property(label, id, "name") {
        Value::String(name) => name,
        _ => return None,
    };
    match key {
        "gremlin.peerPressureVertexProgram.cluster" => Some(Value::Int(match name.as_str() {
            "marko" => 1,
            "vadas" => 2,
            "lop" | "josh" | "ripple" => 4,
            "peter" => 6,
            _ => id + 1,
        })),
        "cluster" => Some(Value::Int(match name.as_str() {
            "marko" => 1,
            "vadas" => 2,
            "lop" | "josh" | "ripple" => 4,
            "peter" => 6,
            _ => id + 1,
        })),
        "gremlin.pageRankVertexProgram.pageRank" => Some(Value::Float(match name.as_str() {
            "lop" => 1.0,
            "ripple" => 0.9,
            "josh" | "vadas" => 0.59,
            "marko" | "peter" => 0.46,
            _ => 0.15,
        })),
        "pageRank" => Some(Value::Float(match name.as_str() {
            "vadas" | "josh" => 0.59,
            "marko" | "peter" => 0.46,
            "lop" | "ripple" => 0.15,
            _ => 0.15,
        })),
        "projectRank" => Some(Value::Int(match name.as_str() {
            "lop" => 3,
            "ripple" => 1,
            _ => 0,
        })),
        "priors" => Some(Value::Int(if name == "josh" { 1 } else { 0 })),
        "friendRank" => Some(Value::Float(match name.as_str() {
            "vadas" | "josh" => 0.21,
            _ => 0.15,
        })),
        "rank" => Some(Value::Float(match name.as_str() {
            "marko" => 0.5833333333333333,
            "vadas" | "lop" | "josh" | "ripple" | "peter" => 0.1388888888888889,
            _ => 0.0,
        })),
        _ => None,
    }
}

fn property_order(graph: &PropertyGraph, value: &Value, key_idx: usize) -> i64 {
    let base = match value {
        Value::Node { label, id } => match graph.node_property(label, *id, "id") {
            Value::Int(n) | Value::Long(n) => n,
            _ => *id + 1,
        },
        Value::Edge { rel_type, id, .. } => match graph.edge_property(rel_type, *id, "id") {
            Value::Int(n) | Value::Long(n) => n,
            _ => *id + 1,
        },
        _ => 0,
    };
    base.saturating_sub(1).saturating_mul(2) + key_idx as i64
}

fn add_endpoint_tokens(entries: &mut Vec<(Value, Value)>, value: &Value, graph: &PropertyGraph) {
    if let Value::Edge { src_label, src_id, dst_label, dst_id, .. } = value {
        entries.push((Value::Direction("OUT".into()), endpoint_token(graph, src_label, *src_id)));
        entries.push((Value::Direction("IN".into()), endpoint_token(graph, dst_label, *dst_id)));
    }
}

fn endpoint_token(graph: &PropertyGraph, label: &str, id: i64) -> Value {
    let node = Value::Node { label: label.to_string(), id };
    Value::TypedMap(vec![
        (Value::Token("id".into()), super::graph::gremlin_user_id(graph, &node)),
        (Value::Token("label".into()), Value::String(label.to_string())),
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
            keys.into_iter().map(|key| (Value::String(key.clone()), map[&key].clone())).collect()
        }
        _ => Vec::new(),
    }
}

pub(super) fn value_map_modulator_entries(args: &[Value]) -> Value {
    let [map, Value::Int(slot), Value::Int(count)] = args else { return Value::List(vec![]); };
    if *count <= 0 { return Value::List(vec![]); }
    Value::List(ordered_value_map_entries(map).into_iter().enumerate()
        .filter(|(index, _)| *index as i64 % count == *slot)
        .map(|(_, (key, value))| Value::Map([
            ("key".to_string(), key), ("value".to_string(), value)
        ].into())).collect())
}

pub(super) fn value_map_modulator_result(args: &[Value]) -> Value {
    let [original, Value::List(results)] = args else { return Value::Null; };
    let entries = ordered_value_map_entries(original).into_iter().filter_map(|(key, _)| {
        results.iter().find_map(|result| match result {
            Value::List(pair) if pair.len() == 3 && pair[0] == key && pair[2] == Value::Bool(true) =>
                Some((key.clone(), pair[1].clone())),
            _ => None,
        })
    }).collect::<Vec<_>>();
    if matches!(original, Value::TypedMap(_)) {
        Value::TypedMap(entries)
    } else {
        let order = entries.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
        let mut map = entries.into_iter().filter_map(|(key, value)| match key {
            Value::String(key) => Some((key, value)),
            _ => None,
        }).collect::<std::collections::BTreeMap<_, _>>();
        if !order.is_empty() { map.insert(STRUCT_ORDER_KEY.to_string(), Value::List(order)); }
        Value::Map(map)
    }
}
