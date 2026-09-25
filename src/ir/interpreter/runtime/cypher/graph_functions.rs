//! Graph identity, properties, path access, and collection size.
use super::*;

pub(super) fn call(name: &str, args: &[Value], graph: &PropertyGraph) -> IrResult<Option<Value>> {
    match (name, args) {
        ("cypher_merge_valid", [Value::Map(properties)]) => {
            if properties.values().any(|value|matches!(value,Value::Null)) {
                return Err(InterpretError::Diagnosed {
                    code: crate::ir::diagnostics::RuntimeDiagnosis::MergeReadOwnWrites,
                    message: "MERGE cannot match or create a null property".into(),
                });
            }
            Ok(Some(Value::Bool(true)))
        }
        ("cypher_relationship_list", [Value::Path(items) | Value::List(items)]) => Ok(Some(Value::List(
            items.iter().filter(|value|matches!(value,Value::Edge { .. })).cloned().collect()
        ))),
        ("cypher_relationship_list", [Value::Null]) => Ok(Some(Value::Null)),
        ("cypher_path", [Value::List(items)]) => Ok(Some(Value::Path(items.clone()))),
        ("cypher_labels" | "cypher_type", [Value::Null]) => Ok(Some(Value::Null)),
        ("cypher_labels", [value @ Value::Node { .. }]) => call("labels", std::slice::from_ref(value), graph),
        ("cypher_type", [value @ Value::Edge { .. }]) => call("type", std::slice::from_ref(value), graph),
        ("cypher_labels" | "cypher_type", [_]) => Err(InterpretError::Diagnosed {
            code: crate::ir::diagnostics::RuntimeDiagnosis::InvalidValue,
            message: format!("{name} requires {}", if name == "cypher_labels" { "a node" } else { "a relationship" }),
        }),
        // ----- graph-element built-ins -----
        ("cypher_id", [value]) => Ok(Some(graph.cypher_id(value).map(Value::Int).unwrap_or(Value::Null))),
        ("id", [value]) => Ok(Some(match value {
            Value::Node { .. } | Value::Edge { .. } | Value::InternalId { .. } => {
                element_internal_id(graph, value).unwrap_or(Value::Null)
            }
            _ => Value::Null,
        })),
        ("cypher_label", [value]) => Ok(Some(match value {
            Value::Node { label, .. } => Value::String(label.clone()),
            Value::Edge { rel_type, .. } => Value::String(rel_type.clone()),
            _ => Value::Null,
        })),
        ("cypher_has_label", [Value::Node { label, id }, Value::String(wanted)]) =>
            Ok(Some(Value::Bool(graph.node_labels(label, *id).contains(wanted)))),
        ("cypher_has_label", [Value::Edge { rel_type, .. }, Value::String(wanted)]) =>
            Ok(Some(Value::Bool(rel_type == wanted))),
        ("cypher_has_label", [Value::Null, _]) => Ok(Some(Value::Null)),
        ("cypher_has_label", _) => Err(InterpretError::Type("Label predicate requires a node".into())),
        ("labels", [value]) => Ok(Some(match value {
            Value::Node { label, id } => Value::List(graph.node_labels(label, *id).into_iter().map(Value::String).collect()),
            _ => Value::Null,
        })),
        ("type", [value]) => Ok(Some(match value {
            Value::Edge { rel_type, .. } => Value::String(rel_type.clone()),
            _ => Value::Null,
        })),
        (
            "start_node",
            [
                Value::Edge {
                    src_label, src_id, ..
                },
            ],
        ) => Ok(Some(Value::Node {
            label: src_label.clone(),
            id: *src_id,
        })),
        (
            "end_node",
            [
                Value::Edge {
                    dst_label, dst_id, ..
                },
            ],
        ) => Ok(Some(Value::Node {
            label: dst_label.clone(),
            id: *dst_id,
        })),
        ("start_node", [Value::Path(_) | Value::List(_)]) => Err(InterpretError::Runtime(
            "Binder exception: Function START_NODE did not receive correct arguments:\nActual:   (RECURSIVE_REL)\nExpected: (REL)".to_string(),
        )),
        ("end_node", [Value::Path(_) | Value::List(_)]) => Err(InterpretError::Runtime(
            "Binder exception: Function END_NODE did not receive correct arguments:\nActual:   (RECURSIVE_REL)\nExpected: (REL)".to_string(),
        )),
        ("start_node" | "end_node", [Value::Null]) => Ok(Some(Value::Null)),
        // `nodes(path)` — every other element starting from index 0.
        ("nodes", [Value::Path(items)]) => Ok(Some(Value::List(
            items
                .iter()
                .filter(|v| matches!(v, Value::Node { .. }))
                .cloned()
                .collect(),
        ))),
        // `relationships(path)` — every other element starting from index 1.
        ("relationships", [Value::Path(items)]) => Ok(Some(Value::List(
            items
                .iter()
                .filter(|v| matches!(v, Value::Edge { .. }))
                .cloned()
                .collect(),
        ))),
        // `relationships(list_of_edges)` — variable-length expansions
        // bind the relationship variable as a list of edges, so the
        // identity case is just passing it through.
        ("relationships", [Value::List(items)]) => Ok(Some(Value::List(
            items
                .iter()
                .filter(|v| matches!(v, Value::Edge { .. }))
                .cloned()
                .collect(),
        ))),
        ("nodes" | "relationships", [Value::Null]) => Ok(Some(Value::List(Vec::new()))),
        ("length", [Value::Path(items)]) => {
            // Cypher path length = number of relationships = (len-1)/2.
            let edges = items
                .iter()
                .filter(|v| matches!(v, Value::Edge { .. }))
                .count();
            Ok(Some(Value::Int(edges as i64)))
        }
        ("length", [Value::List(items)]) => Ok(Some(Value::Int(items.len() as i64))),
        ("length", [Value::Null]) => Ok(Some(Value::Null)),
        ("size", [Value::Node { .. } | Value::Edge { .. } | Value::Path(_)]) => {
            let actual = format!("({})", value_type_name(&args[0]));
            Err(kuzu_function_arity_error(
                "SIZE",
                &actual,
                "(LIST) -> INT64\n(ARRAY) -> INT64\n(MAP) -> INT64\n(STRING) -> INT64",
            ))
        }
        ("size", [value]) if runtime_list(value).is_some() => Ok(Some(Value::Int(
            runtime_list(value).unwrap_or_default().len() as i64,
        ))),
        ("size", [Value::String(value)]) => Ok(Some(Value::Int(value.chars().count() as i64))),
        ("size", [Value::List(items)]) => Ok(Some(Value::Int(items.len() as i64))),
        ("size", [Value::Map(map)]) => Ok(Some(Value::Int(visible_map_len(map) as i64))),
        ("size", [Value::Null]) => Ok(Some(Value::Null)),
        ("is_trail", [Value::Path(items)]) => Ok(Some(Value::Bool(is_trail_path(items)))),
        ("is_trail", [Value::Null]) => Ok(Some(Value::Null)),
        ("keys", [Value::Node { label, id }]) => Ok(Some(Value::List(
            graph
                .node_property_keys_with_id(label)
                .into_iter()
                .filter(|key| key != STRUCT_ORDER_KEY && key != STRUCT_TYPES_KEY)
                .filter(|key| !matches!(graph.node_property(label, *id, key), Value::Null))
                .map(Value::String)
                .collect(),
        ))),
        ("keys", [Value::Edge { rel_type, id, .. }]) => Ok(Some(Value::List(
            graph
                .edge_property_keys(rel_type)
                .into_iter()
                .filter(|key| key != STRUCT_ORDER_KEY && key != STRUCT_TYPES_KEY)
                .filter(|key| !matches!(graph.edge_property(rel_type, *id, key), Value::Null))
                .map(Value::String)
                .collect(),
        ))),
        ("keys", [Value::Map(map)]) => Ok(Some(Value::List(
            visible_map_keys(map).into_iter().map(Value::String).collect(),
        ))),
        ("keys", [Value::Null]) => Ok(Some(Value::Null)),
        ("isempty", [Value::String(value)]) => Ok(Some(Value::Bool(value.is_empty()))),
        ("isempty", [Value::List(items)]) => Ok(Some(Value::Bool(items.is_empty()))),
        ("isempty", [Value::Map(map)]) => Ok(Some(Value::Bool(visible_map_len(map) == 0))),
        ("isempty", [Value::Null]) => Ok(Some(Value::Null)),
        ("properties", [Value::Node { label, id }]) => {
            let mut map = std::collections::BTreeMap::new();
            for key in graph.node_property_keys_with_id(label) {
                let value = graph.node_property(label, *id, &key);
                if key != STRUCT_ORDER_KEY && key != STRUCT_TYPES_KEY && !matches!(value, Value::Null) {
                    map.insert(key, value);
                }
            }
            Ok(Some(Value::Map(map)))
        }
        ("properties", [Value::Edge { rel_type, id, .. }]) => {
            let mut map = std::collections::BTreeMap::new();
            for key in graph.edge_property_keys(rel_type) {
                let value = graph.edge_property(rel_type, *id, &key);
                if key != STRUCT_ORDER_KEY && key != STRUCT_TYPES_KEY && !matches!(value, Value::Null) {
                    map.insert(key, value);
                }
            }
            Ok(Some(Value::Map(map)))
        }
        ("properties", [Value::Map(map)]) => Ok(Some(Value::Map(map.clone()))),
        ("properties", [Value::Null]) => Ok(Some(Value::Null)),
        // Kuzu-style projection: `properties([elements...], key)` returns
        // the list of values produced by reading `key` off each element.
        ("properties", [Value::List(items), Value::String(key)]) => {
            let projected = items
                .iter()
                .map(|item| graph_element_property(graph, item, key))
                .collect();
            Ok(Some(Value::List(projected)))
        }
        ("properties", [Value::Path(items), Value::String(key)]) => {
            let projected = items
                .iter()
                .map(|item| graph_element_property(graph, item, key))
                .collect();
            Ok(Some(Value::List(projected)))
        }
        ("property", [target, Value::String(key)]) => {
            Ok(Some(graph_element_property(graph, target, key)))
        }
        _ => Ok(None),
    }
}
