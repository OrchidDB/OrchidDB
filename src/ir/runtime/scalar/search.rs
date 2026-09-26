//! The graph-backed TinkerGraph text-search start service.

use std::collections::BTreeMap;

use regex::Regex;

use crate::ir::catalog::PropertyGraph;
use crate::ir::runtime::{RuntimeError, IrResult};
use crate::ir::value::Value;

/// Each argument is a parameter map. Later maps (dynamic parameters and
/// with() options) override earlier maps, just as ServiceCallStep does.
pub(super) fn search(args: &[Value], graph: &PropertyGraph) -> IrResult<Value> {
    let mut parameters = BTreeMap::new();
    for arg in args {
        match arg {
            Value::Map(map) => parameters.extend(map.clone()),
            Value::TypedMap(entries) => {
                for (key, value) in entries {
                    let Value::String(key) = key else {
                        return Err(error("Search parameter keys must be strings"));
                    };
                    parameters.insert(key.clone(), value.clone());
                }
            }
            _ => return Err(error("Search parameters must be a map")),
        }
    }
    let pattern = if let Some(value) = parameters.get("regex") {
        string_parameter("regex", value)?.to_owned()
    } else if let Some(value) = parameters.get("search") {
        format!(".*({}).*", property_text(value))
    } else {
        return Err(error("Missing search/regex parameter"));
    };
    let kind = match parameters.get("type") {
        None | Some(Value::Null) => None,
        Some(value) => Some(string_parameter("type", value)?),
    };
    if kind.is_some_and(|kind| !matches!(kind, "Vertex" | "Edge" | "VertexProperty")) {
        return Err(error("Type must be one of Vertex/Edge/VertexProperty"));
    }
    // Java Matcher.matches() requires the entire string to match, including
    // for the explicit regex parameter. is_match() alone would search substrings.
    let regex = Regex::new(&format!("\\A(?:{pattern})\\z"))
        .map_err(|err| error(&format!("Invalid search regex (native profile excludes lookaround and backreferences): {err}")))?;
    let mut matches = Vec::new();
    if kind != Some("Edge") {
        for label in graph.labels() {
            for id in graph
                .node_ids(&label)
                .map_err(|err| error(&err.to_string()))?
            {
                let vertex = Value::Node {
                    label: label.clone(),
                    id,
                };
                for property in graph.properties(&vertex, &[]) {
                    if kind != Some("VertexProperty") {
                        collect_match(&mut matches, &regex, &property);
                    }
                    if kind != Some("Vertex") {
                        for meta_property in graph.properties(&property, &[]) {
                            collect_match(&mut matches, &regex, &meta_property);
                        }
                    }
                }
            }
        }
    }
    if kind.is_none() || kind == Some("Edge") {
        for rel_type in graph.rel_types() {
            for id in graph.edge_ids(&rel_type) {
                let Some((src_label, src_id, dst_label, dst_id)) =
                    graph.edge_endpoints(&rel_type, id)
                else {
                    continue;
                };
                let edge = Value::Edge {
                    rel_type: rel_type.clone(),
                    id,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    projected_properties: None,
                };
                for property in graph.properties(&edge, &[]) {
                    collect_match(&mut matches, &regex, &property);
                }
            }
        }
    }
    Ok(Value::List(matches))
}

fn collect_match(matches: &mut Vec<Value>, regex: &Regex, property: &Value) {
    let (Value::VertexProperty { value, .. } | Value::Property { value, .. }) = property else {
        return;
    };
    if regex.is_match(&property_text(value)) {
        matches.push(property.clone());
    }
}

fn property_text(value: &Value) -> String {
    match value {
        Value::BigInt(number) => number.to_string(),
        Value::BigDecimal(number) => number.to_string(),
        other => super::strings::display_for_concat(other),
    }
}

fn string_parameter<'a>(name: &str, value: &'a Value) -> IrResult<&'a str> {
    match value {
        Value::String(value) => Ok(value),
        _ => Err(error(&format!("Search parameter {name} must be a string"))),
    }
}

fn error(message: &str) -> RuntimeError {
    RuntimeError::Runtime(message.into())
}
