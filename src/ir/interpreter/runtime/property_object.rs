//! value_map / element_map / property_map / properties_list.
//!
//! Extracted from `interpreter.rs` lines 2708..2793.

use crate::ir::catalog::PropertyGraph;
use crate::ir::value::Value;

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
    for key in &resolved_keys {
        let value = match &target {
            Value::Node { .. } => graph.node_property(&label, id, key),
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
    if name == "properties_list" {
        // `properties()` is fan-out: build a list of `{key, value}`
        // structs and return as a List so a wrapping `GraphUnwind`
        // produces one row per pair.
        let pairs = resolved_keys
            .into_iter()
            .filter_map(|key| {
                let value = match &target {
                    Value::Node { .. } => graph.node_property(&label, id, &key),
                    Value::Edge { .. } => graph.edge_property(&label, id, &key),
                    _ => Value::Null,
                };
                if matches!(value, Value::Null) {
                    return None;
                }
                let mut prop = std::collections::BTreeMap::new();
                prop.insert("key".to_string(), Value::String(key));
                prop.insert("value".to_string(), value);
                prop.insert("element".to_string(), target.clone());
                Some(Value::Map(prop))
            })
            .collect();
        return Value::List(pairs);
    }
    Value::Map(map)
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
