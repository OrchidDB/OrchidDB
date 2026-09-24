//! Graph properties, Gremlin ordering, and shortest paths.

use super::maps::{runtime_list, visible_map_keys};
use super::numeric::value_as_f64;
use super::strings::display_for_concat;
use crate::ir::catalog::PropertyGraph;
use crate::ir::interpreter::element_id::element_internal_id;
use crate::ir::plan::Direction;
use crate::ir::value::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

pub(crate) fn graph_element_property(graph: &PropertyGraph, value: &Value, key: &str) -> Value {
    match (value, key) {
        (Value::VertexProperty {key,..}|Value::Property {key,..}, "key") => Value::String(key.clone()),
        (Value::VertexProperty {value,..}|Value::Property {value,..}, "value") => value.as_ref().clone(),
        (Value::VertexProperty {..}, _) => graph.properties(value,&[key.into()]).first().and_then(|p|if let Value::Property{value,..}=p {Some(value.as_ref().clone())}else{None}).unwrap_or(Value::Null),
        (Value::Node { label, .. }, "_label" | "_LABEL") => Value::String(label.clone()),
        (Value::Edge { rel_type, .. }, "_label" | "_LABEL") => Value::String(rel_type.clone()),
        (Value::Node { .. } | Value::Edge { .. } | Value::InternalId { .. }, "_id" | "_ID") => {
            element_internal_id(graph, value).unwrap_or(Value::Null)
        }
        (
            Value::Edge {
                src_label, src_id, ..
            },
            "_src" | "_SRC",
        ) => element_internal_id(
            graph,
            &Value::Node {
                label: src_label.clone(),
                id: *src_id,
            },
        )
        .unwrap_or(Value::Null),
        (
            Value::Edge {
                dst_label, dst_id, ..
            },
            "_dst" | "_DST",
        ) => element_internal_id(
            graph,
            &Value::Node {
                label: dst_label.clone(),
                id: *dst_id,
            },
        )
        .unwrap_or(Value::Null),
        (Value::Node { label, id }, _) => graph.node_property(label, *id, key),
        (Value::Edge { rel_type, id, .. }, _) => graph.edge_property(rel_type, *id, key),
        (Value::TypedMap(entries), _) => entries
            .iter()
            .find(|(candidate, _)| candidate == &Value::String(key.to_string()))
            .map(|(_, value)| value.clone())
            .unwrap_or(Value::Null),
        (Value::Map(map), _) => map.get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(super) fn gremlin_user_id(graph: &PropertyGraph, value: &Value) -> Value {graph.element_public_id(value)}

pub(super) fn gremlin_scan_order(graph: &PropertyGraph, value: &Value) -> Value {
    // Default identity includes the label, while scan order retains the
    // catalog's row ordinal. Changing identity must not reorder traversers.
    match value {
        Value::Node { label, id } if graph.node_property(label, *id, "id") == Value::Null => {
            return Value::Long(*id);
        }
        Value::Edge { rel_type, id, .. }
            if graph.edge_property(rel_type, *id, "id") == Value::Null =>
        {
            return Value::Long(*id);
        }
        _ => {}
    }
    match gremlin_user_id(graph, value) {
        Value::Int(id) | Value::Long(id) => Value::Long(id),
        Value::String(text) => Value::String(text),
        _ => element_internal_id(graph, value).unwrap_or(Value::Null),
    }
}

pub(super) fn gremlin_order_key(graph: &PropertyGraph, value: &Value) -> Value {
    // TinkerPop orderability: rank the value's type class per the
    // orderability spec, then order within the class. The key is a
    // `[rank, class_key]` pair — list comparison is lexicographic, so
    // cross-class ordering follows the rank and same-class pairs fall
    // through to the class key.
    // Order.asc/desc normalizes a top-level Enum to its name before applying
    // GremlinValueComparator. Nested enums remain typed objects.
    let normalized;
    let value = match value {
        Value::Token(name) | Value::Direction(name) => {
            normalized = Value::String(name.clone()); &normalized
        }
        value => value,
    };
    let (rank, key) = gremlin_orderability_parts(graph, value);
    Value::List(vec![Value::Int(rank), key])
}

/// (class rank, within-class key) per the TinkerPop orderability spec:
/// null < Boolean < Number < Date < String < UUID < Vertex < Edge <
/// VertexProperty < Property < Path < Set < List < Map < unknown.
fn gremlin_orderability_parts(graph: &PropertyGraph, value: &Value) -> (i64, Value) {
    if let Some(items) = crate::ir::value::as_gremlin_set(value) {
        let mut keys = items.iter().map(|item| nested_order_key(graph,item)).collect::<Vec<_>>();
        keys.sort_by(crate::ir::gremlin_semantics::compare_order_keys);
        return (11, Value::List(keys));
    }
    match value {
        Value::TypedMap(entries) => (13, map_order_key(graph, entries.iter().map(|(key,value)| (key.clone(),value.clone())).collect())),
        Value::MapEntry(entry) => (14, Value::List(vec![nested_order_key(graph,&entry.0), nested_order_key(graph,&entry.1)])),
        Value::Token(_) | Value::Direction(_) | Value::CardinalityValue {..} => (14, value.clone()),
        Value::Null => (0, Value::Null),
        Value::Bool(_) => (1, value.clone()),
        Value::Byte(_)
        | Value::UInt8(_)
        | Value::Short(_)
        | Value::UInt16(_)
        | Value::Int(_)
        | Value::UInt32(_)
        | Value::Long(_)
        | Value::UInt64(_)
        | Value::Float32(_)
        | Value::Float(_)
        | Value::BigInt(_)
        | Value::UInt128(_)
        | Value::BigDecimal(_) => (2, value.clone()),
        Value::DateTime(_) => (3, value.clone()),
        Value::String(s) => {
            if s.starts_with("uuid[") && s.ends_with(']') {
                (5, value.clone())
            } else {
                (4, value.clone())
            }
        }
        Value::Node { .. } => (6, graph.element_public_id(value)),
        Value::InternalId { .. } => (6, value.clone()),
        Value::Edge { .. } => (7, graph.element_public_id(value)),
        Value::VertexProperty { .. } => (8, graph.element_public_id(value)),
        Value::Property { key, value, .. } => (9, Value::List(vec![
            nested_order_key(graph, &Value::String(key.clone())),
            nested_order_key(graph, value),
        ])),
        Value::Map(map) => (13, map_order_key(graph, visible_map_keys(map).into_iter()
            .filter_map(|key| map.get(&key).map(|value| (Value::String(key), value.clone()))).collect())),
        Value::Path(items) => (10, Value::List(items.iter().map(|item| nested_order_key(graph, item)).collect())),
        Value::List(items) => (12, Value::List(items.iter().map(|item| nested_order_key(graph, item)).collect())),
        Value::BulkSet(_) => (11, value.clone()),
        Value::Set(_) => (11, value.clone()),
    }
}

fn nested_order_key(graph: &PropertyGraph, value: &Value) -> Value {
    let (rank,key) = gremlin_orderability_parts(graph,value);
    Value::List(vec![Value::Int(rank),key])
}

fn map_order_key(graph: &PropertyGraph, entries: Vec<(Value,Value)>) -> Value {
    let mut entries = entries.into_iter().map(|(key,value)| (nested_order_key(graph,&key),nested_order_key(graph,&value))).collect::<Vec<_>>();
    entries.sort_by(|a,b| crate::ir::gremlin_semantics::compare_order_keys(&a.0,&b.0));
    Value::List(entries.into_iter().map(|(key,value)| Value::List(vec![key,value])).collect())
}

pub(super) fn local_order_by_key(
    graph: &PropertyGraph,
    value: &Value,
    key: &str,
    dir: &str,
) -> Value {
    let desc = dir.eq_ignore_ascii_case("desc");
    if let Some(items) = runtime_list(value) {
        let mut keyed = items
            .into_iter()
            .filter_map(|item| {
                let key_value = local_order_item_key(graph, &item, key);
                if matches!(key_value, Value::Null) {
                    None
                } else {
                    Some((item, key_value))
                }
            })
            .collect::<Vec<_>>();
        keyed.sort_by(|(_, a), (_, b)| crate::ir::gremlin_semantics::compare_order_keys(&gremlin_order_key(graph, a), &gremlin_order_key(graph, b)));
        if desc {
            keyed.reverse();
        }
        return Value::List(keyed.into_iter().map(|(item, _)| item).collect());
    }
    let entries = match value {
        Value::TypedMap(entries) => Some(entries.clone()),
        Value::Map(map) => Some(visible_map_keys(map).into_iter().filter_map(|key| map.get(&key).cloned().map(|value|(Value::String(key),value))).collect()),
        _ => None,
    };
    if let Some(entries) = entries {
        let mut keyed = entries.into_iter().map(|(entry_key, entry_value)| {
            let sort_value = match key { "value" | "values" => entry_value.clone(), _ => entry_key.clone() };
            (Value::MapEntry(Box::new((entry_key,entry_value))),sort_value)
        }).collect::<Vec<_>>();
        keyed.sort_by(|(_,a),(_,b)| crate::ir::gremlin_semantics::compare_order_keys(&gremlin_order_key(graph, a), &gremlin_order_key(graph, b)));
        if desc { keyed.reverse(); }
        return Value::List(keyed.into_iter().map(|(entry,_)|entry).collect());
    }
    value.clone()
}

fn local_order_item_key(graph: &PropertyGraph, item: &Value, key: &str) -> Value {
    match key {
        "id" => gremlin_user_id(graph, item),
        "label" => match item {
            Value::Node { label, .. } => Value::String(label.clone()),
            Value::Edge { rel_type, .. } => Value::String(rel_type.clone()),
            _ => Value::Null,
        },
        "key" | "keys" => match item {
            Value::MapEntry(entry) => entry.0.clone(),
            Value::Map(map) => map.get("key").cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        },
        "value" | "values" => match item {
            Value::MapEntry(entry) => entry.1.clone(),
            Value::Map(map) => map.get("value").cloned().unwrap_or(Value::Null),
            _ => item.clone(),
        },
        property => graph_element_property(graph, item, property),
    }
}

/// `valueMap()` stores its per-key values pre-rendered as `["josh"]`
/// display strings (so the map renders in TinkerPop's form). When a
/// `select(key)` extracts such an entry back onto the traverser, revive
/// it as a real list so downstream rendering shows `l[josh]`, not the
/// raw display text.
pub(super) fn revive_value_map_entry(value: Value) -> Value {
    let Value::String(text) = &value else {
        return value;
    };
    let Some(inner) = text.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return value;
    };
    if inner.is_empty() {
        return Value::List(Vec::new());
    }
    let mut items = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        let item = if let Some(unquoted) = part.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        {
            Value::String(unquoted.to_string())
        } else if let Ok(n) = part.parse::<i64>() {
            Value::Int(n)
        } else if let Ok(f) = part.parse::<f64>() {
            Value::Float(f)
        } else if part == "true" || part == "false" {
            Value::Bool(part == "true")
        } else {
            // Not a rendered value list (e.g. an arbitrary string that
            // happens to be bracketed) — leave untouched.
            return value;
        };
        items.push(item);
    }
    Value::List(items)
}

pub(crate) fn shortest_paths(
    graph: &PropertyGraph,
    start: &Value,
    target: Option<&Value>,
    direction: Direction,
    rel_filter: &[String],
    max_distance: Option<f64>,
    include_edges: bool,
) -> Value {
    let Value::Node { label, id } = start else {
        return Value::List(Vec::new());
    };
    if let Some(Value::Node {
        label: target_label,
        id: target_id,
    }) = target
    {
        return shortest_path_between(
            graph,
            label,
            *id,
            target_label,
            *target_id,
            direction,
            rel_filter,
            max_distance,
            include_edges,
        )
        .map(|path| Value::List(vec![Value::Path(path)]))
        .unwrap_or_else(|| Value::List(Vec::new()));
    }
    let mut targets = Vec::new();
    for target_label in graph.labels() {
        if let Ok(ids) = graph.node_ids(&target_label) {
            targets.extend(
                ids.into_iter()
                    .map(|target_id| (target_label.clone(), target_id)),
            );
        }
    }
    targets.sort_by_key(|(target_label, target_id)| {
        match graph.node_property(target_label, *target_id, "name") {
            Value::String(name) => name,
            _ => format!("{target_label}:{target_id}"),
        }
    });

    let mut out = Vec::new();
    for (target_label, target_id) in targets {
        if let Some(path) = shortest_path_between(
            graph,
            label,
            *id,
            &target_label,
            target_id,
            direction,
            rel_filter,
            max_distance,
            include_edges,
        ) {
            out.push(Value::Path(path));
        }
    }
    Value::List(out)
}

fn shortest_path_between(
    graph: &PropertyGraph,
    start_label: &str,
    start_id: i64,
    target_label: &str,
    target_id: i64,
    direction: Direction,
    rel_filter: &[String],
    max_distance: Option<f64>,
    include_edges: bool,
) -> Option<Vec<Value>> {
    let start_key = (start_label.to_string(), start_id);
    let target_key = (target_label.to_string(), target_id);
    let mut queue = VecDeque::from([start_key.clone()]);
    let mut seen = HashSet::from([start_key.clone()]);
    let mut distance = HashMap::from([(start_key.clone(), 0usize)]);
    let mut parent: HashMap<(String, i64), ((String, i64), Value)> = HashMap::new();

    while let Some((label, id)) = queue.pop_front() {
        if (label.as_str(), id) == (target_label, target_id) {
            break;
        }
        let next_distance = distance.get(&(label.clone(), id)).copied().unwrap_or(0) + 1;
        if max_distance.is_some_and(|max| (next_distance as f64) > max) {
            continue;
        }
        let mut neighbors = match direction {
            Direction::Out => graph
                .out_edges(&label, id, rel_filter)
                .into_iter()
                .map(|(rel_type, edge_row, other_label, other_id)| {
                    (other_label, other_id, rel_type, edge_row)
                })
                .collect::<Vec<_>>(),
            Direction::In => graph
                .in_edges(&label, id, rel_filter)
                .into_iter()
                .map(|(rel_type, edge_row, other_label, other_id)| {
                    (other_label, other_id, rel_type, edge_row)
                })
                .collect::<Vec<_>>(),
            Direction::Both => graph
                .out_edges(&label, id, rel_filter)
                .into_iter()
                .map(|(rel_type, edge_row, other_label, other_id)| {
                    (other_label, other_id, rel_type, edge_row)
                })
                .chain(graph.in_edges(&label, id, rel_filter).into_iter().map(
                    |(rel_type, edge_row, other_label, other_id)| {
                        (other_label, other_id, rel_type, edge_row)
                    },
                ))
                .collect::<Vec<_>>(),
        };
        neighbors.sort();
        for (other_label, other_id, rel_type, edge_row) in neighbors {
            let next = (other_label, other_id);
            if seen.insert(next.clone()) {
                let Some((src_label, src_id, dst_label, dst_id)) =
                    graph.edge_endpoints(&rel_type, edge_row)
                else {
                    continue;
                };
                parent.insert(
                    next.clone(),
                    (
                        (label.clone(), id),
                        Value::Edge {
                            rel_type,
                            id: edge_row,
                            src_label,
                            src_id,
                            dst_label,
                            dst_id,
                            projected_properties: None,
                        },
                    ),
                );
                distance.insert(next.clone(), next_distance);
                queue.push_back(next);
            }
        }
    }
    if !seen.contains(&target_key) {
        return None;
    }

    let mut keys = vec![(target_key.clone(), None)];
    let mut cursor = target_key;
    while cursor != start_key {
        let (next_cursor, edge) = parent.get(&cursor)?.clone();
        cursor = next_cursor;
        keys.push((cursor.clone(), Some(edge)));
    }
    keys.reverse();
    let mut path = Vec::new();
    let mut previous_edge = None;
    for (idx, ((label, id), edge)) in keys.into_iter().enumerate() {
        if idx > 0 && include_edges {
            if let Some(edge) = previous_edge.take() {
                path.push(edge);
            }
        }
        path.push(Value::Node { label, id });
        previous_edge = edge;
    }
    Some(path)
}

pub(super) fn select_binding_by_pop(binding: &Value, history: &Value, pop: &str) -> Value {
    let values = match history {
        Value::List(values) if !values.is_empty() => values.as_slice(),
        _ if !matches!(binding, Value::Null) => return if pop == "all" {
            Value::List(vec![binding.clone()])
        } else { binding.clone() },
        _ => return Value::Null,
    };
    match pop {
        "first" => values.first().cloned().unwrap_or(Value::Null),
        "all" => Value::List(values.to_vec()),
        "mixed" if values.len() == 1 => values[0].clone(),
        "mixed" => Value::List(values.to_vec()),
        _ => values.last().cloned().unwrap_or(Value::Null),
    }
}

pub(super) fn gremlin_within(needle: &Value, candidates: &Value) -> bool {
    if let Some(items) = runtime_list(candidates) {
        return items.iter().any(|item| crate::ir::gremlin_semantics::equals(needle, item));
    }
    crate::ir::gremlin_semantics::equals(needle, candidates)
}

pub(super) fn gremlin_math_bin(op: &str, lhs: &Value, rhs: &Value) -> Value {
    let Some(left) = gremlin_math_scalar(lhs) else {
        return Value::Null;
    };
    let Some(right) = gremlin_math_scalar(rhs) else {
        return Value::Null;
    };
    match op {
        "add" => Value::Float(left + right),
        "sub" => Value::Float(left - right),
        "mul" => Value::Float(left * right),
        "div" => Value::Float(left / right),
        _ => Value::Null,
    }
}

fn gremlin_math_scalar(value: &Value) -> Option<f64> {
    if let Some(items) = runtime_list(value) {
        return items.iter().find_map(value_as_f64);
    }
    value_as_f64(value)
}

pub(super) fn tree_value(value: &Value) -> Value {
    // Build the nested tree map from a folded list of path histories:
    // each path contributes root -> child -> ... chains.
    fn insert_path(map: &mut BTreeMap<String, Value>, items: &[Value]) {
        let Some(head) = items.first() else { return };
        let entry = map
            .entry(display_for_concat(head))
            .or_insert(Value::Map(BTreeMap::new()));
        if let Value::Map(child) = entry {
            insert_path(child, &items[1..]);
        }
    }
    let mut map = BTreeMap::new();
    match value {
        Value::List(items) => {
            for item in items {
                match item {
                    Value::Path(p) | Value::List(p) => insert_path(&mut map, p),
                    other => insert_path(&mut map, std::slice::from_ref(other)),
                }
            }
        }
        Value::Path(p) => insert_path(&mut map, p),
        Value::Null => {}
        other => insert_path(&mut map, std::slice::from_ref(other)),
    }
    Value::Map(map)
}

pub(super) fn path_last_value(value: &Value) -> Option<&Value> {
    match value {
        Value::Path(items) | Value::List(items) => items.last(),
        Value::Null => None,
        other => Some(other),
    }
}

pub(super) fn path_last_label(value: &Value) -> Option<&str> {
    match path_last_value(value)? {
        Value::Node { label, .. } => Some(label.as_str()),
        Value::Edge { rel_type, .. } => Some(rel_type.as_str()),
        _ => None,
    }
}

pub(super) fn format_placeholder(
    current: &Value,
    binding: &Value,
    key: &str,
    graph: &PropertyGraph,
) -> Value {
    let resolved = graph_element_property(graph, current, key);
    if matches!(resolved, Value::Null) {
        binding.clone()
    } else {
        resolved
    }
}

pub(super) fn is_trail_path(items: &[Value]) -> bool {
    let mut seen = HashSet::new();
    for item in items {
        if let Value::Edge { rel_type, id, .. } = item {
            if !seen.insert((rel_type.as_str(), *id)) {
                return false;
            }
        }
    }
    true
}

pub(super) fn is_acyclic_path(items: &[Value]) -> bool {
    let mut seen = HashSet::new();
    for item in items {
        if let Value::Node { label, id } = item {
            if !seen.insert((label.as_str(), *id)) {
                return false;
            }
        }
    }
    true
}

/// Resolve a native reference by the catalog's public element identity. The
/// reference label is descriptive; it does not change vertex identity.
pub(super) fn resolve_gremlin_vertex_reference(graph: &PropertyGraph, id: &Value) -> crate::ir::interpreter::IrResult<Value> {
    for label in graph.labels() {
        for row in graph.node_ids(&label)? {
            let vertex = Value::Node { label: label.clone(), id: row };
            if gremlin_user_id(graph, &vertex).three_valued_eq(id) == Some(true) { return Ok(vertex); }
        }
    }
    Err(crate::ir::interpreter::InterpretError::Runtime(format!("Vertex with id {id:?} does not exist")))
}
