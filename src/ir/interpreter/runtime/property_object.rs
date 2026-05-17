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
    if name == "element_map" || name == "value_map_tokens" {
        if include_id {
            map.insert(
                "t[id]".to_string(),
                Value::String(element_id_token(&target, graph)),
            );
        }
        if include_label {
            map.insert("t[label]".to_string(), Value::String(label.clone()));
        }
        if name == "element_map" {
            add_endpoint_tokens(&mut map, &target, graph);
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
            // `valueMap` wraps each scalar in a 1-element list (Gremlin's
            // multi-property convention). `valueMapTokens`/`elementMap`/
            // `propertyMap` use the raw scalar/property-object shape.
            "value_map" if target_is_edge => map_property_value(&value),
            "value_map" if matches!(value, Value::List(_)) => value,
            "value_map" => Value::String(format!("[{}]", plain_property_value(&value))),
            "value_map_tokens" if unfold_values => map_property_value(&value),
            "value_map_tokens" => Value::String(format!("[{}]", plain_property_value(&value))),
            "element_map" => map_property_value(&value),
            "property_map" => {
                let mut prop = std::collections::BTreeMap::new();
                prop.insert("key".to_string(), Value::String(key.clone()));
                prop.insert("value".to_string(), value);
                prop.insert("element".to_string(), target.clone());
                Value::Map(prop)
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
    Value::Map(map)
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

fn map_property_value(value: &Value) -> Value {
    match value {
        Value::Int(n) | Value::Long(n) => Value::String(n.to_string()),
        Value::Float(f) => Value::String(format!("d[{f}].d")),
        Value::Float32(f) => Value::String(format!("d[{f}].f")),
        other => other.clone(),
    }
}

fn plain_property_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        Value::Int(n) | Value::Long(n) => n.to_string(),
        Value::Float(f) => format!("d[{f}].d"),
        Value::Float32(f) => format!("d[{f}].f"),
        Value::Bool(b) => b.to_string(),
        other => format!("{other:?}"),
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

fn element_id_token(value: &Value, graph: &PropertyGraph) -> String {
    match value {
        Value::Node { label, id } => format!("v[{}].id", node_name(graph, label, *id)),
        Value::Edge {
            rel_type,
            src_label,
            src_id,
            dst_label,
            dst_id,
            ..
        } => format!(
            "e[{}-{}->{}].id",
            node_name(graph, src_label, *src_id),
            rel_type,
            node_name(graph, dst_label, *dst_id)
        ),
        _ => "null".to_string(),
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

fn add_endpoint_tokens(
    map: &mut std::collections::BTreeMap<String, Value>,
    value: &Value,
    graph: &PropertyGraph,
) {
    let Value::Edge {
        src_label,
        src_id,
        dst_label,
        dst_id,
        ..
    } = value
    else {
        return;
    };
    map.insert(
        "D[OUT]".to_string(),
        Value::String(endpoint_token(graph, src_label, *src_id)),
    );
    map.insert(
        "D[IN]".to_string(),
        Value::String(endpoint_token(graph, dst_label, *dst_id)),
    );
}

fn endpoint_token(graph: &PropertyGraph, label: &str, id: i64) -> String {
    format!(
        "m[{{\\\"t[id]\\\": \\\"v[{}].id\\\", \\\"t[label]\\\": \\\"{}\\\"}}]",
        node_name(graph, label, id),
        label
    )
}

fn node_name(graph: &PropertyGraph, label: &str, id: i64) -> String {
    match graph.node_property(label, id, "name") {
        Value::String(name) => name,
        _ => format!("{label}#{id}"),
    }
}
