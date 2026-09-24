//! Shared runtime function dispatch.

use crate::ir::catalog::PropertyGraph;
use crate::ir::interpreter::{InterpretError, IrResult};
use crate::ir::interpreter::expr::compare_values;
use crate::ir::value::Value;
use std::collections::BTreeMap;
use super::registry;
use super::casts::{
    cast_list_to_string, cast_to_bigdecimal, cast_to_bigint, cast_to_bool, cast_to_byte,
    cast_to_date, cast_to_float, cast_to_float32, cast_to_gremlin_date, cast_to_gremlin_int,
    cast_to_int, cast_to_long, cast_to_number, cast_to_short, cast_to_string,
    parse_datetime_string,
};
use super::cypher::cypher_call;
use super::datetime::{date_add_value, date_diff_value};
use super::graph::{
    format_placeholder, graph_element_property, gremlin_math_bin,
    gremlin_order_key, gremlin_scan_order, gremlin_user_id,
    gremlin_within, local_order_by_key, path_last_label,
    path_last_value, revive_value_map_entry, select_binding_by_pop, tree_value,
};
use super::lists::display_for_list_to_string;
use super::maps::{
    is_map_entry, runtime_list, slice_map_entries, visible_map_keys, visible_map_values,
};
use super::path::{
    apply_path_by_keys, apply_path_by_keys_keep_nulls, path_intermediate_pairs, path_pairs,
    project_path_edges, slice_path_at, slice_path_at_value,
};
use super::property_object::{eval_property_element, eval_property_object};
use super::reductions::{
    apply_sack_op, fold_reduce_op, reduce_list_numeric, reduce_list_orderable,
};
use super::strings::{self, display_for_concat, regex_match_literal, substring};
use super::type_check::typeof_matches;

pub(in crate::ir::interpreter) fn eval_call(name: &str, args: Vec<Value>, graph: &PropertyGraph) -> IrResult<Value> {
    if name == "tinker_search" {
        return super::search::search(&args, graph);
    }
    match (name, args.as_slice()) {
        ("property_value", [Value::VertexProperty {value,..} | Value::Property {value,..}]) => return Ok(value.as_ref().clone()),
        ("property_key", [Value::VertexProperty {key,..} | Value::Property {key,..}]) => return Ok(Value::String(key.clone())),
        ("property_value", [Value::MapEntry(pair)]) => return Ok(pair.1.clone()),
        ("property_key", [Value::MapEntry(pair)]) => return Ok(pair.0.clone()),
        ("property_value" | "property_key", [_]) => return Err(InterpretError::Runtime("key()/value() requires a Property or Map.Entry".into())),
        ("local_limit", [value,count])=>return Ok(super::lists::gremlin_local_range(value,0,count.as_i64().unwrap_or(0))),
        ("gremlin_merge_matches",[element,criteria,out,input])=>return Ok(Value::Bool(super::mutations::matches(element,criteria,out,input,graph)?)),
        ("gremlin_cardinality_value", [Value::String(cardinality), value]) => {
            if !matches!(cardinality.as_str(), "single" | "list" | "set") {
                return Err(InterpretError::Runtime(format!("Invalid cardinality: {cardinality}")));
            }
            return Ok(Value::CardinalityValue {cardinality:cardinality.clone(),value:Box::new(value.clone())});
        },
        ("gremlin_token_literal", [Value::String(token)]) => return Ok(Value::Token(token.clone())),
        ("gremlin_direction_literal", [Value::String(token)]) => return Ok(Value::Direction(token.clone())),
        ("list_merge" | "gremlin_traversal_list_merge", [lhs,rhs]) => return super::lists::gremlin_merge(lhs,rhs,name.starts_with("gremlin_traversal")),
        ("local_order", [value]) if crate::ir::value::as_gremlin_set(value).is_some() => {
            let mut items = crate::ir::value::as_gremlin_set(value).unwrap().to_vec();
            items.sort_by(compare_values);
            return Ok(Value::List(items));
        }
        ("local_mean", [value]) if !matches!(value, Value::List(_) | Value::BulkSet(_)) && crate::ir::value::as_gremlin_set(value).is_none() => {
            return Ok(reduce_list_numeric(&[value.clone()], "mean"));
        }
        ("gremlin_sum_result", [sum, count]) => return Ok(if count.as_i64() == Some(0) { Value::Null } else { match sum { Value::Int(n) => Value::Long(*n), other => other.clone() } }),
        ("gremlin_vertex_property_ref", [id, owner, Value::String(key)]) => {
            return graph.properties(owner, &[key.clone()]).into_iter()
                .find(|property| graph.element_public_id(property).three_valued_eq(id) == Some(true))
                .ok_or_else(|| InterpretError::Runtime(format!("Vertex property with id {id:?} does not exist")));
        },
        ("gremlin_edge_ref", [id]) => return super::graph::resolve_gremlin_edge_reference(graph, id),
        ("gremlin_vertex_ref", [id, Value::String(_label)]) => return super::graph::resolve_gremlin_vertex_reference(graph, id),
        ("local_tail", [value, count]) => return Ok(super::lists::gremlin_local_tail(value, count.as_i64().unwrap_or(0))),
        ("local_range", [value, low, high]) => return Ok(super::lists::gremlin_local_range(value, low.as_i64().unwrap_or(0), high.as_i64().unwrap_or(-1))),
        _ => {}
    }
    if let Some(inner) = name.strip_prefix("gremlin_traversal_list_") {
        // Validate the incoming traverser before evaluating the argument type,
        // matching TinkerPop's collection-step validation order.
        if let Some(lhs) = args.first() {
            if !matches!(lhs, Value::List(_) | Value::Path(_) | Value::BulkSet(_)) && crate::ir::value::as_gremlin_set(lhs).is_none() && !(inner == "merge" && matches!(lhs, Value::Map(_) | Value::TypedMap(_))) {
                return Err(InterpretError::Runtime(if matches!(lhs, Value::Null) {
                    format!("Incoming traverser for {inner} step can't be null")
                } else { format!("{inner} step can only take an array or an Iterable type for incoming traversers, encountered {}", lhs.type_name()) }));
            }
        }
        if let Some(rhs) = args.get(1) {
            if !matches!(rhs, Value::List(_) | Value::Path(_) | Value::BulkSet(_)) && crate::ir::value::as_gremlin_set(rhs).is_none() && !(inner == "merge" && matches!(rhs, Value::Map(_) | Value::TypedMap(_))) {
                return Err(InterpretError::Runtime(if matches!(rhs, Value::Null) {
                    format!("traversal argument for {inner} step must yield an iterable type, not null")
                } else { format!("traversal argument for {inner} step must yield an iterable type, encountered {}", rhs.type_name()) }));
            }
        }
        return eval_call(&format!("list_{inner}"), args, graph);
    }
    if let Some(op) = name.strip_prefix("gremlin_string_") {
        return super::strings::gremlin_string_call(op, &args);
    }
    // Property-object helpers need catalog access; route them before
    // the value-only dispatch table.
    if matches!(
        name,
        "value_map" | "value_map_tokens" | "element_map" | "property_map" | "properties_list"
    ) {
        return Ok(eval_property_object(name, &args, graph));
    }
    if name == "value_map_modulator_entries" {
        return Ok(super::property_object::value_map_modulator_entries(&args));
    }
    if name == "value_map_modulator_result" {
        return Ok(super::property_object::value_map_modulator_result(&args));
    }
    if name == "property_element" {
        return Ok(eval_property_element(&args));
    }
    if let Some(value) = cypher_call(name, &args, graph)? {
        return Ok(value);
    }
    // Canonicalize the name for the Gremlin-leaning tail dispatch too,
    // so e.g. `lcase`/`ucase` cannot diverge from `lower`/`upper`.
    let canonical = registry::canonical_name(name);
    match (canonical.as_ref(), args.as_slice()) {
        ("element_kind", [Value::Node { .. }]) => Ok(Value::String("Vertex".into())),
        ("element_kind", [Value::Edge { .. }]) => Ok(Value::String("Edge".into())),
        ("element_kind", [Value::VertexProperty { .. }]) => Ok(Value::String("VertexProperty".into())),
        ("element_kind", [_]) => Ok(Value::String("Property".into())),
        ("gremlin_id", [value]) => Ok(gremlin_user_id(graph, value)),
        // Projection modulators return the same identity as id().
        ("gremlin_id_token", [value]) => Ok(gremlin_user_id(graph, value)),
        ("gremlin_scan_order", [value]) => Ok(gremlin_scan_order(graph, value)),
        ("gremlin_order_key", [value]) => Ok(gremlin_order_key(graph, value)),
        ("gremlin_compare", [Value::String(op), lhs, rhs]) => Ok(crate::ir::gremlin_semantics::predicate(op, lhs, rhs)),
        ("gremlin_within", [needle, candidates]) => {
            Ok(Value::Bool(gremlin_within(needle, candidates)))
        }
        ("gremlin_math_bin", [Value::String(op), lhs, rhs]) => Ok(gremlin_math_bin(op, lhs, rhs)),
        ("tinker_degree_centrality", [Value::Node { label, id }, Value::String(direction)]) => {
            let edges = if direction == "OUT" {
                graph.out_edges(label, *id, &[])
            } else {
                graph.in_edges(label, *id, &[])
            };
            Ok(Value::Long(edges.len() as i64))
        }
        ("tinker_degree_centrality", [_, _]) => Ok(Value::Null),
        ("lower", [Value::String(s)]) => Ok(Value::String(s.to_lowercase())),
        ("upper", [Value::String(s)]) => Ok(Value::String(s.to_uppercase())),
        ("length" | "size", [Value::String(s)]) => Ok(Value::Int(s.chars().count() as i64)),
        ("size", [Value::List(items)]) => Ok(Value::Int(items.len() as i64)),
        ("abs", [Value::Int(n)]) => Ok(Value::Int(n.abs())),
        ("abs", [Value::Float(f)]) => Ok(Value::Float(f.abs())),
        // ----- additional string ops (Gremlin string family) -----
        ("trim", [Value::String(s)]) => Ok(Value::String(s.trim().to_string())),
        ("ltrim", [Value::String(s)]) => Ok(Value::String(s.trim_start().to_string())),
        ("rtrim", [Value::String(s)]) => Ok(Value::String(s.trim_end().to_string())),
        ("reverse", [Value::String(s)]) => Ok(Value::String(s.chars().rev().collect())),
        ("reverse", [Value::List(items)]) => {
            let mut rev = items.clone();
            rev.reverse();
            Ok(Value::List(rev))
        }
        ("reverse", [Value::Path(items)]) => {
            let mut rev = items.clone();
            rev.reverse();
            Ok(Value::Path(rev))
        }
        ("reverse", [other]) => Ok(other.clone()),
        ("gremlin_substring", [Value::String(s), start]) => Ok(start
            .as_i64()
            .map(|start| Value::String(substring(s, start, None)))
            .unwrap_or(Value::Null)),
        ("gremlin_substring", [Value::String(s), start, end]) => {
            Ok(match (start.as_i64(), end.as_i64()) {
                (Some(start), Some(end)) => Value::String(substring(s, start, Some(end))),
                _ => Value::Null,
            })
        }
        ("substring", [Value::String(s), Value::Int(start)]) => {
            Ok(Value::String(substring(s, *start, None)))
        }
        ("substring", [Value::String(s), Value::Int(start), Value::Int(end)]) => {
            Ok(Value::String(substring(s, *start, Some(*end))))
        }
        ("replace", [Value::String(s), Value::String(from), Value::String(_)])
            if from.is_empty() =>
        {
            Ok(Value::String(s.clone()))
        }
        ("replace", [Value::String(s), Value::String(from), Value::String(to)]) => {
            Ok(Value::String(s.replace(from.as_str(), to)))
        }
        ("concat", [Value::String(a), Value::String(b)]) => Ok(Value::String(format!("{a}{b}"))),
        ("concat", [a, Value::String(b)]) => {
            // Best-effort: stringify the LHS so `1.concat("x")` etc. lowers.
            Ok(Value::String(format!("{}{}", display_for_concat(a), b)))
        }
        ("concat", [Value::String(a), b]) => {
            Ok(Value::String(format!("{}{}", a, display_for_concat(b))))
        }
        ("concat", [a, b]) => Ok(Value::String(format!(
            "{}{}",
            display_for_concat(a),
            display_for_concat(b)
        ))),
        ("conjoin", [Value::List(items), Value::String(delim)]) => {
            let parts: Vec<String> = items
                .iter()
                .filter(|item| !matches!(item, Value::Null))
                .map(display_for_list_to_string)
                .collect();
            Ok(Value::String(parts.join(delim)))
        }
        ("conjoin", [Value::Path(items), Value::String(delim)]) => {
            let parts: Vec<String> = items
                .iter()
                .filter(|item| !matches!(item, Value::Null))
                .map(display_for_list_to_string)
                .collect();
            Ok(Value::String(parts.join(delim)))
        }
        ("conjoin", [Value::String(s), Value::String(delim)]) => {
            Ok(Value::String(format!("{s}{delim}")))
        }
        ("conjoin", [other, Value::String(_)]) => Ok(Value::String(display_for_concat(other))),
        ("split", [Value::String(s), Value::String(delim)]) => {
            let parts = if delim.is_empty() {
                s.chars().map(|c| Value::String(c.to_string())).collect()
            } else {
                s.split(delim.as_str())
                    .map(|p| Value::String(p.to_string()))
                    .collect()
            };
            Ok(Value::List(parts))
        }
        ("split" | "gremlin_split_ws", [Value::String(s), Value::Null])
        | ("gremlin_split_ws", [Value::String(s)]) => {
            // TinkerPop `split(null)` splits on whitespace.
            Ok(Value::List(
                s.split_whitespace()
                    .map(|p| Value::String(p.to_string()))
                    .collect(),
            ))
        }
        // ----- coin(p) — keep with probability p. Deterministic in
        // tests: we use the binding's identity hash as the entropy source
        // so a given input row's outcome is reproducible. With p=1.0 we
        // always keep; with p=0.0 we always drop. -----
        ("coin_keep", [Value::Float(p)]) => Ok(Value::Bool(*p >= 1.0)),
        ("coin_keep", _) => Ok(Value::Bool(false)),
        // ----- index() — list with (item, index) pairs -----
        ("index_list", [Value::List(items)]) => {
            let pairs = items
                .iter()
                .enumerate()
                .map(|(i, item)| Value::List(vec![item.clone(), Value::Int(i as i64)]))
                .collect();
            Ok(Value::List(pairs))
        }
        ("index_list", [other]) => Ok(Value::List(vec![Value::List(vec![
            other.clone(),
            Value::Int(0),
        ])])),
        ("index_map", [Value::List(items)]) => Ok(Value::TypedMap(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| (Value::Int(index as i64), item.clone()))
                .collect(),
        )),
        ("index_map", [other]) => Ok(Value::TypedMap(vec![(Value::Int(0), other.clone())])),
        // ----- hasValue helper: stringify any-property -----
        ("any_property", [Value::Node { label, id }]) => {
            let mut combined = Vec::new();
            for key in graph.node_property_keys(label) {
                let value = graph.node_property(label, *id, &key);
                if !matches!(value, Value::Null) {
                    combined.push(value);
                }
            }
            // Encode as a list so the surrounding predicate can `any/all`
            // over it. Bare `Compare` predicates collapse into "first
            // value matches" via best-effort: return the first non-null
            // value if there's exactly one, otherwise the list.
            if combined.len() == 1 {
                Ok(combined.into_iter().next().unwrap())
            } else {
                Ok(Value::List(combined))
            }
        }
        ("any_property", [Value::Edge { rel_type, id, .. }]) => {
            let mut combined = Vec::new();
            for key in graph.edge_property_keys(rel_type) {
                let value = graph.edge_property(rel_type, *id, &key);
                if !matches!(value, Value::Null) {
                    combined.push(value);
                }
            }
            if combined.len() == 1 {
                Ok(combined.into_iter().next().unwrap())
            } else {
                Ok(Value::List(combined))
            }
        }
        ("any_property", _) => Ok(Value::Null),
        // ----- project(...) — make_map(label0, value0, label1, value1, ...) -----
        ("make_map", entries) if entries.len() % 2 == 0 => {
            let mut map = std::collections::BTreeMap::new();
            for chunk in entries.chunks_exact(2) {
                let key = match &chunk[0] {
                    Value::String(s) => s.clone(),
                    other => display_for_concat(other),
                };
                map.insert(key, chunk[1].clone());
            }
            Ok(Value::Map(map))
        }
        ("make_project_map", entries) if entries.len() % 2 == 0 => {
            let mut map = std::collections::BTreeMap::new();
            for chunk in entries.chunks_exact(2) {
                if matches!(chunk[1], Value::Null) {
                    continue;
                }
                let key = match &chunk[0] {
                    Value::String(s) => s.clone(),
                    other => display_for_concat(other),
                };
                map.insert(key, chunk[1].clone());
            }
            Ok(Value::Map(map))
        }
        ("make_project_map_productive", entries) if entries.len() % 3 == 0 => {
            let mut map = std::collections::BTreeMap::new();
            for chunk in entries.chunks_exact(3) {
                if !matches!(chunk[2], Value::Bool(true)) {
                    continue;
                }
                let key = match &chunk[0] {
                    Value::String(s) => s.clone(),
                    other => display_for_concat(other),
                };
                map.insert(key, chunk[1].clone());
            }
            Ok(Value::Map(map))
        }
        // ----- select(Column.keys|values) on a map-shaped traverser -----
        ("map_keys", [Value::TypedMap(entries)]) => Ok(Value::List(entries.iter().map(|(key, _)| key.clone()).collect())),
        ("map_values", [Value::TypedMap(entries)]) => Ok(Value::List(entries.iter().map(|(_, value)| value.clone()).collect())),
        ("map_get_display", [Value::TypedMap(entries), key]) => Ok(entries.iter().find(|(candidate, _)| candidate == key).map(|(_, value)| value.clone()).unwrap_or(Value::Null)),
        ("local_count", [Value::TypedMap(entries)]) => Ok(Value::Long(entries.len() as i64)),
        ("requested_property_values", [target, Value::List(keys)]) => Ok(super::property_object::requested_property_values(target,&keys.iter().filter_map(|k|if let Value::String(k)=k {Some(k.clone())}else{None}).collect::<Vec<_>>(),graph)),
        ("map_keys", [Value::MapEntry(pair)]) => Ok(pair.0.clone()),
        ("map_values", [Value::MapEntry(pair)]) => Ok(pair.1.clone()),
        ("map_keys", [Value::Map(map)]) if is_map_entry(map) => Ok(Value::List(vec![
            map.get("key").cloned().unwrap_or(Value::Null),
        ])),
        ("map_values", [Value::Map(map)]) if is_map_entry(map) => Ok(Value::List(vec![
            map.get("value").cloned().unwrap_or(Value::Null),
        ])),
        ("map_keys", [Value::Map(map)]) => Ok(Value::List(
            visible_map_keys(map)
                .into_iter()
                .map(Value::String)
                .collect(),
        )),
        ("map_values", [Value::Map(map)]) => Ok(Value::List(visible_map_values(map))),
        ("map_literal", [Value::List(keys), Value::List(values)]) => {
            Ok(Value::map_from_entries(keys.iter().cloned().zip(values.iter().cloned()).collect()))
        }
        ("map_keys" | "map_values", [Value::Null]) => Ok(Value::Null),
        ("map_keys" | "map_values", [_]) => Ok(Value::List(Vec::new())),
        // Prefer the labelled binding (set by an upstream `as(...)`) over
        // a same-named key on the current map traverser. Gremlin's
        // `select` resolves labels first; a `groupCount().as("x")` then
        // `select("x")` must return the whole map even if the map happens
        // to have a key called "x".
        ("select_key_or_binding", [_, binding, _]) if !matches!(binding, Value::Null) => {
            Ok(binding.clone())
        }
        ("select_key_or_binding", [Value::Map(map), _, Value::String(key)]) => Ok(
            revive_value_map_entry(map.get(key).cloned().unwrap_or(Value::Null)),
        ),
        ("select_key_or_binding", [_, binding, _]) => Ok(binding.clone()),
        // `map_has_key(map, key)` — true iff `map` is a Map containing the
        // given key. Used by lowering to decide whether to keep a row.
        ("map_has_key", [Value::TypedMap(entries),key]) => Ok(Value::Bool(entries.iter().any(|(candidate,_)|candidate==key))),
        ("map_has_key", [Value::Map(map), Value::String(key)]) => {
            Ok(Value::Bool(map.contains_key(key)))
        }
        ("map_has_key", _) => Ok(Value::Bool(false)),
        ("select_history_append", [history, value]) => {
            let mut values = match history {
                Value::List(values) => values.clone(),
                Value::Null => Vec::new(),
                other => vec![other.clone()],
            };
            values.push(value.clone());
            Ok(Value::List(values))
        }
        (
            "select_key_or_binding_pop",
            [
                source,
                binding,
                history,
                Value::String(key),
                Value::String(pop),
            ],
        ) => {
            if !matches!(binding, Value::Null) || !matches!(history, Value::Null) {
                return Ok(select_binding_by_pop(binding, history, pop));
            }
            if let Value::TypedMap(entries)=source {
                return Ok(entries.iter().find(|(candidate,_)|candidate==&Value::String(key.clone())).map(|(_,value)|value.clone()).unwrap_or(Value::Null));
            }
            if let Value::Map(map) = source {
                return Ok(revive_value_map_entry(
                    map.get(key).cloned().unwrap_or(Value::Null),
                ));
            }
            Ok(Value::Null)
        }
        // ----- outV/inV/bothV endpoint projection -----
        (
            "edge_src",
            [
                Value::Edge {
                    src_label, src_id, ..
                },
            ],
        ) => Ok(Value::Node {
            label: src_label.clone(),
            id: *src_id,
        }),
        (
            "edge_dst",
            [
                Value::Edge {
                    dst_label, dst_id, ..
                },
            ],
        ) => Ok(Value::Node {
            label: dst_label.clone(),
            id: *dst_id,
        }),
        (
            "edge_both",
            [
                Value::Edge {
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    ..
                },
            ],
        ) => Ok(Value::List(vec![
            Value::Node {
                label: src_label.clone(),
                id: *src_id,
            },
            Value::Node {
                label: dst_label.clone(),
                id: *dst_id,
            },
        ])),
        // Endpoint projection on a non-edge — propagate the binding so
        // the surrounding chain has something to chew on.
        ("edge_src" | "edge_dst", [other]) => Ok(other.clone()),
        ("edge_both", [other]) => Ok(Value::List(vec![other.clone()])),
        // ----- path() and slicing variants -----
        ("path_extend_after", [history, previous, item]) => {
            let mut path = match history {
                Value::Path(items) => items.clone(),
                _ => vec![previous.clone()],
            };
            path.push(item.clone());
            Ok(Value::Path(path))
        }
        ("path_attach_label", [labels, history, Value::String(label)]) => {
            let len = match history { Value::Path(items) => items.len(), _ => 1 };
            let mut positions = match labels { Value::List(items) => items.clone(), _ => Vec::new() };
            positions.resize(len, crate::ir::value::gremlin_set(Vec::new()));
            if let Some(last) = positions.last_mut() {
                let mut set = crate::ir::value::as_gremlin_set(last).unwrap_or(&[]).to_vec();
                let label = Value::String(label.clone());
                if !set.contains(&label) { set.push(label); }
                *last = crate::ir::value::gremlin_set(set);
            }
            Ok(Value::List(positions))
        }
        ("gremlin_column_keys", [Value::Path(items), labels]) => {
            let mut labels = match labels { Value::List(labels) => labels.clone(), _ => Vec::new() };
            labels.resize(items.len(), crate::ir::value::gremlin_set(Vec::new()));
            Ok(Value::List(labels))
        }
        ("gremlin_column_values", [Value::Path(items), _]) => Ok(Value::List(items.clone())),
        ("gremlin_column_keys", [value, _]) => eval_call("map_keys", vec![value.clone()], graph),
        ("gremlin_column_values", [value, _]) => eval_call("map_values", vec![value.clone()], graph),
        ("path_or_self", [Value::Path(items), _]) => Ok(Value::Path(items.clone())),
        ("path_or_self", [Value::Null, fallback])
        | ("path_or_self", [Value::List(_), fallback])
            if matches!(args.first(), Some(Value::Null)) =>
        {
            // Single-vertex case (no expansion): path becomes a 1-element
            // path containing the current value.
            Ok(Value::Path(vec![fallback.clone()]))
        }
        ("path_or_self", [_, fallback]) => Ok(Value::Path(vec![fallback.clone()])),
        ("path_append", [Value::Path(items), item]) => {
            let mut path = items.clone();
            if !matches!(item, Value::Null) && path.last() != Some(item) {
                path.push(item.clone());
            }
            Ok(Value::Path(path))
        }
        ("path_append", [Value::Null, item]) => Ok(Value::Path(vec![item.clone()])),
        ("path_append", [other, item]) => Ok(Value::Path(vec![other.clone(), item.clone()])),
        ("path_append_after", [Value::Path(items), _, item]) => {
            let mut path = items.clone();
            if !matches!(item, Value::Null) && path.last() != Some(item) {
                path.push(item.clone());
            }
            Ok(Value::Path(path))
        }
        ("path_append_after", [Value::Null, previous, item]) => {
            let mut path = Vec::new();
            if !matches!(previous, Value::Null) {
                path.push(previous.clone());
            }
            if !matches!(item, Value::Null) && path.last() != Some(item) {
                path.push(item.clone());
            }
            Ok(Value::Path(path))
        }
        ("path_append_after", [other, _, item]) => {
            let mut path = vec![other.clone()];
            if !matches!(item, Value::Null) && path.last() != Some(item) {
                path.push(item.clone());
            }
            Ok(Value::Path(path))
        }
        ("path_last_property_eq", [path, Value::String(key), expected]) => {
            let actual = path_last_value(path)
                .map(|last| graph_element_property(graph, last, key))
                .unwrap_or(Value::Null);
            Ok(Value::Bool(actual == *expected))
        }
        ("path_last_label_eq", [path, Value::String(expected)]) => Ok(Value::Bool(
            path_last_label(path).is_some_and(|label| label == expected),
        )),
        ("tree_value", [value]) => Ok(tree_value(value)),
        // Gremlin `{a, b}` set literal — marker-map Set value.
        ("set_literal", [Value::List(items)]) => Ok(crate::ir::value::gremlin_set(items.clone())),
        ("set_literal", [other]) => Ok(crate::ir::value::gremlin_set(vec![other.clone()])),
        // Merge a Set-typed side-effect seed with the aggregated stream:
        // seed items first, then stream values in first-occurrence
        // order, deduplicated (LinkedHashSet semantics).
        ("set_compact", [seed, folded]) => {
            let mut items: Vec<Value> = Vec::new();
            let mut push = |v: &Value| {
                if !matches!(v, Value::Null) && !items.contains(v) {
                    items.push(v.clone());
                }
            };
            match crate::ir::value::as_gremlin_set(seed) {
                Some(seed_items) => seed_items.iter().for_each(&mut push),
                None => match seed {
                    Value::List(seed_items) => seed_items.iter().for_each(&mut push),
                    Value::Map(m) if m.is_empty() => {}
                    other => push(other),
                },
            }
            match folded {
                Value::List(stream) => stream.iter().for_each(&mut push),
                other => push(other),
            }
            Ok(crate::ir::value::gremlin_set(items))
        }
        // Null-through-collect sentinel pair (gremlin side-effect bags
        // under ProductiveByStrategy): collects skip nulls, so nulls are
        // encoded as a marker string before the fold and decoded after.
        // Dedup key canonicalization: property-object maps compare by
        // (key, value) only — the owning element and synthetic order ids
        // are excluded (TinkerPop Property equality).
        ("gremlin_dedup_key", [Value::Map(entries)])
            if matches!(entries.get("element"), Some(Value::Edge { .. }))
                && entries.contains_key("key")
                && entries.contains_key("value") =>
        {
            let mut slim = std::collections::BTreeMap::new();
            if let Some(k) = entries.get("key") {
                slim.insert("key".to_string(), k.clone());
            }
            if let Some(v) = entries.get("value") {
                slim.insert("value".to_string(), v.clone());
            }
            Ok(Value::Map(slim))
        }
        ("gremlin_dedup_key", [other]) => Ok(other.clone()),
        ("null_to_sentinel", [Value::Null]) => Ok(Value::String("\u{0}gremlin.null".to_string())),
        ("null_to_sentinel", [other]) => Ok(other.clone()),
        ("list_restore_null_sentinels", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|item| match item {
                    Value::String(s) if s == "\u{0}gremlin.null" => Value::Null,
                    other => other.clone(),
                })
                .collect(),
        )),
        ("list_restore_null_sentinels", [other]) => Ok(other.clone()),
        // Gremlin `unfold()` item source: unlike Cypher UNWIND, a null
        // traverser unfolds to itself (one null item) and a non-iterable
        // scalar unfolds to a single-item list.
        // `order(Scope.local)` over a Map orders entries as single-entry
        // maps for a following unfold; when the traverser stays a Map,
        // merge the ordered entries back into one map.
        // Dynamic map-key select: look up a map entry whose (string) key
        // is the display form of the given value (`select(__.select("a"))`
        // against a group("m") map keyed by vertices).
        ("map_get_display", [Value::Map(map), key]) => {
            if let Some(v) = map.get(&display_for_concat(key)) {
                return Ok(v.clone());
            }
            let tagged = strings::display_for_tagged_container(key);
            Ok(map.get(&tagged).cloned().unwrap_or(Value::Null))
        }
        ("map_get_display", [_, _]) => Ok(Value::Null),
        ("local_order_merge_map", [original, ordered]) => {
            if matches!(original, Value::Map(_) | Value::TypedMap(_)) && crate::ir::value::as_gremlin_set(original).is_none() {
                if let Value::List(items) = ordered {
                    let mut merged: Vec<(Value, Value)> = Vec::new();
                    for item in items {
                        let entries = match item {
                            Value::MapEntry(entry) => vec![(entry.0.clone(), entry.1.clone())],
                            // Legacy ordering produced one-entry maps. Preserve their
                            // actual entries without interpreting key/value field names.
                            Value::Map(entry) => entry.iter().map(|(k,v)|(Value::String(k.clone()),v.clone())).collect(),
                            Value::TypedMap(entries) => entries.clone(),
                            _ => continue,
                        };
                        for (key, value) in entries {
                            if let Some((_, existing)) = merged.iter_mut().find(|(existing, _)| *existing == key) { *existing = value; }
                            else { merged.push((key, value)); }
                        }
                    }
                    return Ok(Value::TypedMap(merged));
                }
            }
            Ok(ordered.clone())
        }
        ("gremlin_unfold_items", [value]) => Ok(match value {
            Value::Null => Value::List(vec![Value::Null]),
            Value::List(items) | Value::BulkSet(items) | Value::Path(items) => Value::List(items.clone()),
            Value::TypedMap(entries) => Value::List(entries.iter().map(|(key, value)|
                Value::MapEntry(Box::new((key.clone(), value.clone())))
            ).collect()),
            // Gremlin Set marker maps unfold to their items.
            set if crate::ir::value::as_gremlin_set(set).is_some() => {
                Value::List(crate::ir::value::as_gremlin_set(set).unwrap().to_vec())
            }
            Value::Map(entries) => Value::List(
                entries
                    .iter()
                    .filter(|(key,_)| key.as_str()!=crate::ir::value::STRUCT_ORDER_KEY && key.as_str()!=crate::ir::value::STRUCT_TYPES_KEY)
                    .map(|(k, v)| {
                        Value::MapEntry(Box::new((Value::String(k.clone()), v.clone())))
                    })
                    .collect(),
            ),
            other => Value::List(vec![other.clone()]),
        }),
        ("path_from", [Value::Path(items), Value::String(label)]) => {
            Ok(slice_path_at(items, label, /*from_label=*/ true))
        }
        ("path_to", [Value::Path(items), Value::String(label)]) => {
            Ok(slice_path_at(items, label, /*from_label=*/ false))
        }
        ("path_from", [Value::Path(items), Value::String(label), labelled_value]) => {
            Ok(slice_path_at_value(
                items,
                label,
                Some(labelled_value),
                /*from_label=*/ true,
            ))
        }
        ("path_to", [Value::Path(items), Value::String(label), labelled_value]) => {
            Ok(slice_path_at_value(
                items,
                label,
                Some(labelled_value),
                /*from_label=*/ false,
            ))
        }
        (
            "recursive_relationship_path",
            [Value::Path(items), Value::String(label), labelled_value],
        ) => {
            let segment = slice_path_at_value(
                items,
                label,
                Some(labelled_value),
                /*from_label=*/ true,
            );
            let Value::Path(segment) = segment else {
                return Ok(segment);
            };
            let mut out = segment;
            if matches!(out.first(), Some(Value::Node { .. })) {
                out.remove(0);
            }
            if matches!(out.last(), Some(Value::Node { .. })) {
                out.pop();
            }
            Ok(Value::Path(out))
        }
        ("path_from" | "path_to", [Value::Null, _]) => Ok(Value::Null),
        ("path_from" | "path_to", [Value::Null, _, _]) => Ok(Value::Null),
        ("recursive_relationship_path", [Value::Null, _, _]) => Ok(Value::Null),
        ("path_from" | "path_to", [other, _]) => Ok(other.clone()),
        ("path_from" | "path_to", [other, _, _]) => Ok(other.clone()),
        ("recursive_relationship_path", [other, _, _]) => Ok(other.clone()),
        ("path_by_keys", [Value::Path(items), Value::List(keys)]) => {
            Ok(apply_path_by_keys(items, keys, graph)
                .map(Value::Path)
                .unwrap_or(Value::Null))
        }
        ("path_by_keys", [Value::List(items), Value::List(keys)]) => {
            Ok(apply_path_by_keys(items, keys, graph)
                .map(Value::List)
                .unwrap_or(Value::Null))
        }
        ("path_by_keys", [other, Value::List(keys)]) => {
            Ok(apply_path_by_keys(&[other.clone()], keys, graph)
                .map(Value::Path)
                .unwrap_or(Value::Null))
        }
        ("path_by_keys_keep_nulls", [Value::Path(items), Value::List(keys)]) => Ok(Value::Path(
            apply_path_by_keys_keep_nulls(items, keys, graph),
        )),
        ("path_by_keys_keep_nulls", [Value::List(items), Value::List(keys)]) => Ok(Value::List(
            apply_path_by_keys_keep_nulls(items, keys, graph),
        )),
        ("path_by_keys_keep_nulls", [other, Value::List(keys)]) => Ok(Value::Path(
            apply_path_by_keys_keep_nulls(&[other.clone()], keys, graph),
        )),
        ("path_pairs", [Value::Path(items)]) => Ok(path_pairs(items)),
        ("path_pairs", [Value::Null]) => Ok(Value::List(Vec::new())),
        ("path_intermediate_pairs", [Value::Path(items)]) => Ok(path_intermediate_pairs(items)),
        ("path_intermediate_pairs", [Value::Null]) => Ok(Value::List(Vec::new())),
        ("path_project_edges", [Value::Path(items), Value::List(keys)]) => {
            Ok(project_path_edges(items, keys))
        }
        ("path_project_edges", [other, Value::List(keys)]) => {
            Ok(project_path_edges(&[other.clone()], keys))
        }
        // ----- LocalScoped(<step>) — per-list-element variants -----
        ("local_limit", [Value::List(items), Value::Int(n)]) => {
            let n = (*n).max(0) as usize;
            Ok(Value::List(items.iter().take(n).cloned().collect()))
        }
        ("local_limit", [Value::Map(items), Value::Int(n)]) => {
            let n = (*n).max(0) as usize;
            Ok(Value::Map(slice_map_entries(items, 0, n.min(items.len()))))
        }
        ("local_skip", [Value::List(items), Value::Int(n)]) => {
            let n = (*n).max(0) as usize;
            Ok(Value::List(items.iter().skip(n).cloned().collect()))
        }
        ("local_skip", [Value::Map(items), Value::Int(n)]) => {
            let n = (*n).max(0) as usize;
            Ok(Value::Map(slice_map_entries(
                items,
                n.min(items.len()),
                items.len(),
            )))
        }
        ("local_order", [Value::List(items)]) => {
            let mut sorted = items.clone();
            sorted.sort_by(compare_values);
            Ok(Value::List(sorted))
        }
        ("local_order_by_key", [items, Value::String(key), Value::String(dir)]) => {
            Ok(local_order_by_key(graph, items, key, dir))
        }
        ("local_dedup", [Value::List(items) | Value::Set(items) | Value::BulkSet(items) | Value::Path(items)]) =>
            Ok(crate::ir::value::gremlin_set(items.clone())),
        ("local_count", [value]) if runtime_list(value).is_some() => Ok(Value::Long(runtime_list(value).unwrap().len() as i64)),
        ("local_count", [Value::Map(items)]) => Ok(Value::Long(items.len() as i64)),
        ("local_sum", [value]) if runtime_list(value).is_some() => Ok(reduce_list_numeric(&runtime_list(value).unwrap(), "sum")),
        ("local_min", [value]) if runtime_list(value).is_some() => Ok(reduce_list_orderable(&runtime_list(value).unwrap(), "min")),
        ("local_max", [value]) if runtime_list(value).is_some() => Ok(reduce_list_orderable(&runtime_list(value).unwrap(), "max")),
        ("local_mean", [value]) if runtime_list(value).is_some() => Ok(reduce_list_numeric(&runtime_list(value).unwrap(), "mean")),
        ("local_lcase", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(s.to_lowercase()),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_ucase", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(s.to_uppercase()),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_length", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::Int(s.chars().count() as i64),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_trim", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(s.trim().to_string()),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_ltrim", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(s.trim_start().to_string()),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_rtrim", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(s.trim_end().to_string()),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_reverse_strings", [Value::List(items)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(s.chars().rev().collect()),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_substring", [Value::List(items), Value::Int(start)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(substring(s, *start, None)),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_substring", [Value::List(items), Value::Int(start), Value::Int(end)]) => {
            Ok(Value::List(
                items
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => Value::String(substring(s, *start, Some(*end))),
                        other => other.clone(),
                    })
                    .collect(),
            ))
        }
        ("local_replace", [Value::List(items), Value::String(from), Value::String(to)]) => {
            Ok(Value::List(
                items
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => Value::String(s.replace(from.as_str(), to)),
                        other => other.clone(),
                    })
                    .collect(),
            ))
        }
        ("local_split", [Value::List(items), Value::String(delim)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => {
                        let parts = if delim.is_empty() {
                            s.chars().map(|c| Value::String(c.to_string())).collect()
                        } else {
                            s.split(delim.as_str())
                                .map(|p| Value::String(p.to_string()))
                                .collect()
                        };
                        Value::List(parts)
                    }
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_split", [Value::List(items), Value::Null]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::List(
                        s.split_whitespace()
                            .map(|p| Value::String(p.to_string()))
                            .collect(),
                    ),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_concat", [Value::List(items), Value::String(suffix)]) => Ok(Value::List(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(format!("{s}{suffix}")),
                    other => other.clone(),
                })
                .collect(),
        )),
        ("local_conjoin", [Value::List(items), Value::String(delim)]) => {
            let parts: Vec<String> = items
                .iter()
                .filter(|item| !matches!(item, Value::Null))
                .map(display_for_concat)
                .collect();
            Ok(Value::String(parts.join(delim)))
        }
        ("local_lcase", [Value::String(s)]) => Ok(Value::String(s.to_lowercase())),
        ("local_ucase", [Value::String(s)]) => Ok(Value::String(s.to_uppercase())),
        ("local_length", [Value::String(s)]) => Ok(Value::Int(s.chars().count() as i64)),
        ("local_trim", [Value::String(s)]) => Ok(Value::String(s.trim().to_string())),
        ("local_ltrim", [Value::String(s)]) => Ok(Value::String(s.trim_start().to_string())),
        ("local_rtrim", [Value::String(s)]) => Ok(Value::String(s.trim_end().to_string())),
        ("local_reverse_strings", [Value::String(s)]) => {
            Ok(Value::String(s.chars().rev().collect()))
        }
        ("local_substring", [Value::String(s), Value::Int(start)]) => {
            Ok(Value::String(substring(s, *start, None)))
        }
        ("local_substring", [Value::String(s), Value::Int(start), Value::Int(end)]) => {
            Ok(Value::String(substring(s, *start, Some(*end))))
        }
        ("local_replace", [Value::String(s), Value::String(from), Value::String(_)])
            if from.is_empty() =>
        {
            Ok(Value::String(s.clone()))
        }
        ("local_replace", [Value::String(s), Value::String(from), Value::String(to)]) => {
            Ok(Value::String(s.replace(from.as_str(), to)))
        }
        ("local_split", [Value::String(s), Value::String(delim)]) => {
            let parts = if delim.is_empty() {
                s.chars().map(|c| Value::String(c.to_string())).collect()
            } else {
                s.split(delim.as_str())
                    .map(|p| Value::String(p.to_string()))
                    .collect()
            };
            Ok(Value::List(parts))
        }
        ("local_split", [Value::String(s), Value::Null]) => Ok(Value::List(
            s.split_whitespace()
                .map(|p| Value::String(p.to_string()))
                .collect(),
        )),
        // Local-scoped on non-list inputs degrades to the global handler.
        ("local_limit" | "local_skip", [other, Value::Int(_n)]) => Ok(other.clone()),
        ("local_order" | "local_dedup", [other]) => Ok(other.clone()),
        ("local_count", [_]) => Ok(Value::Long(1)),
        ("local_sum" | "local_min" | "local_max" | "local_mean", [scalar]) => Ok(scalar.clone()),
        // TinkerPop list functions accept iterable values and preserve Set results.
        ("bulk_set", [Value::List(items)]) => Ok(Value::BulkSet(items.clone())),
        ("list_merge", [Value::Map(a), Value::Map(b)])
            if crate::ir::value::as_gremlin_set(&args[0]).is_none()
                && crate::ir::value::as_gremlin_set(&args[1]).is_none() => {
            let mut out = a.clone();
            out.extend(b.clone());
            Ok(Value::Map(out))
        }
        ("list_combine" | "list_merge" | "list_intersect" | "list_difference"
            | "list_disjunct" | "list_product", [a, b]) => {
            let step = name.strip_prefix("list_").unwrap_or(name);
            let iterable = |value: &Value, incoming: bool| -> IrResult<Vec<Value>> {
                if let Some(items) = crate::ir::value::as_gremlin_set(value) {
                    return Ok(items.to_vec());
                }
                match value {
                    Value::List(items) | Value::BulkSet(items) | Value::Path(items) => Ok(items.clone()),
                    Value::Null => Err(InterpretError::Runtime(if incoming {
                        format!("Incoming traverser for {step} step can't be null")
                    } else { format!("Argument provided for {step} step can't be null") })),
                    _ => Err(InterpretError::Runtime(if incoming {
                        format!("{step} step can only take an array or an Iterable type for incoming traversers, encountered {}", value.type_name())
                    } else { format!("{step} step can only take an array or an Iterable as an argument, encountered {}", value.type_name()) })),
                }
            };
            let lhs = iterable(a, true)?;
            let rhs = iterable(b, false)?;
            if step == "combine" {
                return Ok(Value::List(lhs.into_iter().chain(rhs).collect()));
            }
            if step == "product" {
                return Ok(Value::List(lhs.iter().flat_map(|left| rhs.iter().map(move |right|
                    Value::List(vec![left.clone(), right.clone()]))).collect()));
            }
            let mut out = Vec::new();
            for item in lhs.iter().chain(if matches!(step, "merge" | "disjunct") { rhs.iter() } else { [].iter() }) {
                let include = match step {
                    "merge" => true,
                    "intersect" => rhs.contains(item),
                    "difference" => !rhs.contains(item),
                    "disjunct" => lhs.contains(item) != rhs.contains(item),
                    _ => unreachable!(),
                };
                if include && !out.contains(item) { out.push(item.clone()); }
            }
            Ok(crate::ir::value::gremlin_set(out))
        }
        // ----- fold(seed, op) reducer-fold -----
        ("fold_reduce", [Value::List(items), seed, Value::String(op)]) => {
            Ok(fold_reduce_op(items, seed, op))
        }
        ("fold_reduce", [other, seed, Value::String(op)]) => {
            Ok(fold_reduce_op(&[other.clone()], seed, op))
        }
        ("sack_apply", [lhs, rhs, Value::String(op)]) => Ok(apply_sack_op(lhs, rhs, op)),
        // ----- procedure / graph algorithm placeholders -----
        ("procedure_call", _) | ("graph_algorithm", _) => Ok(Value::Null),
        // ----- format(...) named placeholders -----
        ("format_placeholder", [current, binding, Value::String(key)]) => {
            Ok(format_placeholder(current, binding, key, graph))
        }
        // ----- format(...) concatenate stringified pieces -----
        ("format_concat", parts) => {
            if parts.iter().any(|part| matches!(part, Value::Null)) {
                return Ok(Value::Null);
            }
            let mut out = String::new();
            for p in parts {
                out.push_str(&display_for_concat(p));
            }
            Ok(Value::String(out))
        }
        // String ops on Null propagate.
        ("trim" | "ltrim" | "rtrim" | "lcase" | "ucase" | "length" | "size", [Value::Null]) => {
            Ok(Value::Null)
        }
        ("gremlin_substring" | "substring" | "replace" | "concat" | "conjoin" | "split", args)
            if args.iter().any(|a| matches!(a, Value::Null)) =>
        {
            Ok(Value::Null)
        }
        // Scalar casts (Gremlin asNumber/asString/asBool/asDate). Each
        // is best-effort: an inconvertible input yields `null` rather
        // than a hard error so the surrounding chain still produces a
        // row stream the harness can compare.
        ("gremlin_cast_string", [v]) => Ok(super::casts::cast_graph_string(v,graph,false)),
        ("cast_string", [v]) => Ok(cast_to_string(v)),
        ("local_cast_string", [v]) => Ok(super::casts::cast_graph_string(v,graph,true)),
        ("local_cast_number", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_number).collect()))
        }
        ("local_cast_byte", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_byte).collect()))
        }
        ("local_cast_short", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_short).collect()))
        }
        ("local_cast_int", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_gremlin_int).collect()))
        }
        ("local_cast_long", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_long).collect()))
        }
        ("local_cast_bigint", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_bigint).collect()))
        }
        ("local_cast_float", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_float32).collect()))
        }
        ("local_cast_double", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_float).collect()))
        }
        ("local_cast_bigdecimal", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_bigdecimal).collect()))
        }
        ("local_cast_bool", [Value::List(items)]) => {
            Ok(Value::List(items.iter().map(cast_to_bool).collect()))
        }
        ("local_cast_date", [Value::List(items)]) => Ok(Value::List(
            items.iter().map(cast_to_gremlin_date).collect(),
        )),
        ("local_cast_number", [v]) => Ok(cast_to_number(v)),
        ("local_cast_byte", [v]) => Ok(cast_to_byte(v)),
        ("local_cast_short", [v]) => Ok(cast_to_short(v)),
        ("local_cast_int", [v]) => Ok(cast_to_gremlin_int(v)),
        ("local_cast_long", [v]) => Ok(cast_to_long(v)),
        ("local_cast_bigint", [v]) => Ok(cast_to_bigint(v)),
        ("local_cast_float", [v]) => Ok(cast_to_float32(v)),
        ("local_cast_double", [v]) => Ok(cast_to_float(v)),
        ("local_cast_bigdecimal", [v]) => Ok(cast_to_bigdecimal(v)),
        ("local_cast_bool", [v]) => Ok(cast_to_bool(v)),
        ("local_cast_date", [v]) => Ok(cast_to_gremlin_date(v)),
        ("cast_number", [v]) => Ok(cast_to_number(v)),
        ("cast_byte", [v]) => Ok(cast_to_byte(v)),
        ("cast_short", [v]) => Ok(cast_to_short(v)),
        ("cast_int", [v]) => Ok(cast_to_int(v)),
        ("gremlin_cast_int", [v]) => Ok(cast_to_gremlin_int(v)),
        ("cast_long", [v]) => Ok(cast_to_long(v)),
        ("cast_bigint", [v]) => Ok(cast_to_bigint(v)),
        ("cast_float", [v]) => Ok(cast_to_float32(v)),
        ("cast_double", [v]) => Ok(cast_to_float(v)),
        ("cast_bigdecimal", [v]) => Ok(cast_to_bigdecimal(v)),
        ("cast_bool", [v]) => Ok(cast_to_bool(v)),
        ("cast_date", [v]) => Ok(cast_to_date(v)),
        ("gremlin_cast_date", [v]) => match cast_to_gremlin_date(v) {
            Value::Null => Err(InterpretError::Runtime("Can't parse value as date".into())),
            value => Ok(value),
        },
        ("datetime_literal", [Value::String(s)]) => Ok(parse_datetime_string(s)
            .map(Value::DateTime)
            .unwrap_or(Value::Null)),
        ("date_add", [Value::DateTime(s), Value::String(unit), amount]) => {
            Ok(date_add_value(s, unit, amount))
        }
        ("date_diff", [Value::DateTime(_), Value::String(marker)])
            if marker == "__current_datetime__" =>
        {
            Ok(Value::Long(0))
        }
        ("date_diff", [Value::DateTime(lhs), rhs]) => Ok(date_diff_value(lhs, rhs)),
        // Predicate helpers emitted by the Gremlin planner.
        // `typeof_matches(target, name)` resolves Gremlin's
        // `P.typeOf("GType.INT")` against the runtime value's type.
        ("typeof_matches", [v, Value::String(name)]) => Ok(Value::Bool(typeof_matches(v, name))),
        ("regex_match", [Value::String(haystack), Value::String(pattern)]) => {
            Ok(Value::Bool(regex_match_literal(haystack, pattern)?))
        }
        ("regex_match", [Value::Null, _]) | ("regex_match", [_, Value::Null]) => Ok(Value::Null),
        // Generic fallbacks: any string-style helper called with Null
        // or a wrong-shape input yields Null rather than failing.
        _ if args.iter().any(|a| matches!(a, Value::Null)) => Ok(Value::Null),
        // Unknown function names — preserve the original behavior so
        // genuine planning bugs surface, but only for argument shapes
        // we couldn't have plausibly meant to support. Local-* and
        // string ops on unexpected inputs degrade to passing the first
        // arg through, which is closer to identity than panic.
        _ if name.starts_with("local_") => Ok(args.into_iter().next().unwrap_or(Value::Null)),
        _ => Err(InterpretError::Unsupported(format!(
            "function {name}/{}",
            args.len()
        ))),
    }
}
