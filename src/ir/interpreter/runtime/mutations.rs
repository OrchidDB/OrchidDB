//! Internal write procedures. The caller is an explicit GraphProcedureCall
//! with Write mode, so calls cannot be folded or duplicated as scalar expressions.
use crate::ir::catalog::PropertyGraph;
use crate::ir::interpreter::{InterpretError, IrResult};
use crate::ir::value::Value;
use std::collections::BTreeMap;

fn error(message: impl Into<String>) -> InterpretError {
    InterpretError::Runtime(message.into())
}
fn label(value: &Value) -> IrResult<String> {
    let Value::String(label) = value else {
        return Err(error("Label must be a string"));
    };
    if label.is_empty() {
        return Err(error("Label can not be empty"));
    }
    if label.starts_with('~') {
        return Err(error(format!("Label can not be a hidden key: {label}")));
    }
    Ok(label.clone())
}
fn properties(value: &Value) -> IrResult<BTreeMap<String, Value>> {
    match value {
        Value::Map(map) => Ok(map.clone()),
        Value::Null => Ok(BTreeMap::new()),
        _ => Err(error("Expected property map")),
    }
}

pub(crate) fn call(name: &str, args: &[Value], graph: &PropertyGraph) -> IrResult<Value> {
    match (name, args) {
        (
            "gremlin.mutation.merge_create",
            [
                criteria,
                create,
                Value::Bool(edge),
                out,
                input,
                partition,
                Value::Map(cardinalities),
            ],
        ) => merge_create(
            criteria,
            create,
            *edge,
            out,
            input,
            partition,
            cardinalities,
            graph,
        ),
        ("gremlin.mutation.merge_validate", [current, criteria, updates, Value::Bool(edge)]) => {
            validate(&entries(criteria)?, *edge, false)?;
            validate(&entries(updates)?, *edge, true)?;
            Ok(current.clone())
        }
        ("gremlin.mutation.merge_guard", [current, Value::Bool(edge)]) => {
            if matches!(
                current,
                Value::Node { .. } | Value::Edge { .. } | Value::VertexProperty { .. }
            ) {
                return Err(error(format!(
                    "The incoming traverser for {} cannot be an Element",
                    if *edge {
                        "MergeEdgeStep"
                    } else {
                        "MergeVertexStep"
                    }
                )));
            }
            Ok(current.clone())
        }
        (
            "gremlin.mutation.merge_update",
            [
                element,
                updates,
                Value::Map(cardinalities),
                Value::String(default_cardinality),
            ],
        ) => {
            let map = entries(updates)?;
            validate(&map, matches!(element, Value::Edge { .. }), true)?;
            for (key, value) in map {
                let Value::String(key) = key else {
                    unreachable!()
                };
                if matches!(element, Value::Node { .. }) {
                    let cardinality = match cardinalities.get(&key) {
                        Some(Value::String(cardinality)) => cardinality.as_str(),
                        None => default_cardinality.as_str(),
                        _ => return Err(error("Invalid vertex property cardinality")),
                    };
                    graph.set_vertex_property(
                        element,
                        &key,
                        value,
                        parse_cardinality(cardinality)?,
                        BTreeMap::new(),
                    )?;
                } else {
                    graph.set_gremlin_property(element, &key, value)?;
                }
            }
            Ok(element.clone())
        }
        ("gremlin.mutation.add_vertex", [name, props]) => {
            let props = properties(props)?;
            let element = graph.insert_node(label(name)?, props.clone());
            for (key, value) in props { graph.set_gremlin_property(&element, &key, value)?; }
            graph.assign_generated_public_id(&element)?;
            Ok(element)
        }
        ("gremlin.mutation.add_edge", [name, src, dst, props]) => {
            let props = properties(props)?;
            let element = graph.insert_edge(label(name)?, src, dst, props.clone())?;
            for (key, value) in props { graph.set_gremlin_property(&element, &key, value)?; }
            graph.assign_generated_public_id(&element)?;
            Ok(element)
        }
        (
            "gremlin.mutation.property_native",
            [
                target,
                key,
                value,
                Value::String(cardinality),
                Value::Map(meta),
            ],
        ) => {
            if !meta.is_empty() && !matches!(target, Value::Node { .. }) {
                return Err(error("Meta-properties require a vertex property"));
            }
            for key in meta.keys() {
                public_key(key)?;
            }
            match key {
                Value::Token(token) if token == "id" => {
                    if !meta.is_empty() {
                        return Err(error("Meta-properties require a vertex property"));
                    }
                    graph.set_element_public_id(target, value.clone())?;
                }
                Value::String(key) => {
                    public_key(key)?;
                    let cardinality = parse_cardinality(cardinality)?;
                    if matches!(target, Value::Node { .. }) {
                        graph.set_vertex_property(
                            target,
                            key,
                            value.clone(),
                            cardinality,
                            meta.clone(),
                        )?;
                    } else {
                        graph.set_gremlin_property(target, key, value.clone())?;
                    }
                }
                _ => return Err(error("Property key must be a String or T.id")),
            }
            Ok(target.clone())
        }
        ("gremlin.mutation.property", [target, Value::String(key), value]) => {
            if key.is_empty() {
                return Err(error("Property key can not be empty"));
            }
            if key.starts_with('~') {
                return Err(error(format!(
                    "Property key can not be a hidden key: {key}"
                )));
            }
            graph.set_gremlin_property(target, key, value.clone())?;
            Ok(target.clone())
        }
        _ => Err(error(format!("Invalid mutation procedure: {name}"))),
    }
}

fn entries(value: &Value) -> IrResult<Vec<(Value, Value)>> {
    match value {
        Value::Null => Ok(vec![]),
        Value::Map(map) => Ok(map
            .iter()
            .filter(|(k, _)| !k.starts_with("__new_graph_"))
            .map(|(k, v)| (Value::String(k.clone()), v.clone()))
            .collect()),
        Value::TypedMap(entries) => Ok(entries.clone()),
        _ => Err(error(format!(
            "merge argument must be a Map, got {value:?}"
        ))),
    }
}
fn lookup<'a>(map: &'a [(Value, Value)], key: &Value) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}
fn public_key(key: &str) -> IrResult<()> {
    if key.is_empty() {
        return Err(error("Property key can not be empty"));
    }
    if key.starts_with('~') {
        return Err(error(format!(
            "Property key can not be a hidden key: {key}"
        )));
    }
    Ok(())
}
fn validate(map: &[(Value, Value)], edge: bool, updates: bool) -> IrResult<()> {
    for (key, value) in map {
        match key {
            Value::String(key) => public_key(key)?,
            _ if updates => {
                return Err(error("option(onMatch) expects keys in Map to be of String"));
            }
            Value::Token(token) if token == "label" => {
                label(value)?;
            }
            Value::Token(token) if token == "id" => {
                if matches!(value, Value::Null) {
                    return Err(error("merge() does not allow null Map values"));
                }
                if matches!(value, Value::List(_) | Value::Map(_)) {
                    return Err(error("Invalid element id"));
                }
            }
            Value::Direction(direction) if edge && matches!(direction.as_str(), "IN" | "OUT") => {
                if matches!(value, Value::Null) {
                    return Err(error("mergeE() does not allow null Map values"));
                }
            }
            _ => return Err(error("merge map contains an unsupported key")),
        }
    }
    Ok(())
}
fn endpoint(value: &Value, option: &Value, graph: &PropertyGraph) -> IrResult<Value> {
    let value = if matches!(value,Value::Token(token) if token=="Merge.outV" || token=="Merge.inV")
    {
        option
    } else {
        value
    };
    match value {
        Value::Node { .. } => Ok(value.clone()),
        Value::Map(_) | Value::TypedMap(_) => {
            for label in graph.labels() {
                for id in graph.node_ids(&label)? {
                    let node = Value::Node {
                        label: label.clone(),
                        id,
                    };
                    if matches(&node, value, &Value::Null, &Value::Null, graph)? {
                        return Ok(node);
                    }
                }
            }
            Err(error("Vertex does not exist for merge endpoint"))
        }
        Value::Null => Err(error("Merge endpoint option did not resolve a vertex")),
        value => super::graph::resolve_gremlin_vertex_reference(graph, value).map_err(|_| {
            error(format!(
                "Vertex id could not be resolved from mergeE: {value:?}"
            ))
        }),
    }
}
pub(crate) fn matches(
    element: &Value,
    criteria: &Value,
    out: &Value,
    input: &Value,
    graph: &PropertyGraph,
) -> IrResult<bool> {
    let map = entries(criteria)?;
    let edge = matches!(element, Value::Edge { .. });
    validate(&map, edge, false)?;
    for (key, expected) in map {
        let actual = match key {
            Value::String(key) => {
                let found = graph.properties(element, &[key]).iter().any(|property| {
                    matches!(property, Value::VertexProperty { value, .. } | Value::Property { value, .. }
                        if (value.as_ref() == &Value::Null && expected == Value::Null)
                            || value.three_valued_eq(&expected) == Some(true))
                });
                if !found {
                    return Ok(false);
                }
                continue;
            }
            Value::Token(key) if key == "id" => super::graph::gremlin_user_id(graph, element),
            Value::Token(key) if key == "label" => match element {
                Value::Node { label, .. } => Value::String(label.clone()),
                Value::Edge { rel_type, .. } => Value::String(rel_type.clone()),
                _ => Value::Null,
            },
            Value::Direction(direction) => {
                let resolved = endpoint(
                    &expected,
                    if direction == "OUT" { out } else { input },
                    graph,
                )?;
                let Value::Edge {
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    ..
                } = element
                else {
                    return Ok(false);
                };
                let actual = if direction == "OUT" {
                    Value::Node {
                        label: src_label.clone(),
                        id: *src_id,
                    }
                } else {
                    Value::Node {
                        label: dst_label.clone(),
                        id: *dst_id,
                    }
                };
                if actual != resolved {
                    return Ok(false);
                }
                continue;
            }
            _ => return Ok(false),
        };
        if actual.three_valued_eq(&expected) != Some(true) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn merge_create(
    criteria: &Value,
    create: &Value,
    edge: bool,
    out: &Value,
    input: &Value,
    partition: &Value,
    cardinalities: &BTreeMap<String, Value>,
    graph: &PropertyGraph,
) -> IrResult<Value> {
    let mut map = entries(criteria)?;
    let additions = entries(create)?;
    validate(&map, edge, false)?;
    validate(&additions, edge, false)?;
    for (key, value) in additions {
        if let Some(previous) = lookup(&map, &key) {
            if previous != &value
                || matches!(&key, Value::String(key) if cardinalities.contains_key(key))
            {
                return Err(error(
                    "option(onCreate) cannot override values from merge() argument",
                ));
            }
        } else {
            map.push((key, value));
        }
    }
    let public_id = lookup(&map, &Value::Token("id".into())).cloned();
    if let Some(id) = &public_id {
        if graph.find_element_by_public_id(id, edge).is_some() {
            return Err(error(format!("Element with id {id:?} already exists")));
        }
    }
    let name = lookup(&map, &Value::Token("label".into()))
        .map(label)
        .transpose()?
        .unwrap_or_else(|| if edge { "edge" } else { "vertex" }.into());
    let mut props: BTreeMap<_, _> = map
        .iter()
        .filter_map(|(k, v)| {
            if let Value::String(k) = k {
                Some((k.clone(), v.clone()))
            } else {
                None
            }
        })
        .collect();
    props.extend(properties(partition)?);
    let element = if edge {
        let src = lookup(&map, &Value::Direction("OUT".into()))
            .ok_or_else(|| error("Out Vertex not specified"))?;
        let dst = lookup(&map, &Value::Direction("IN".into()))
            .ok_or_else(|| error("In Vertex not specified"))?;
        graph.insert_edge(
            name,
            &endpoint(src, out, graph)?,
            &endpoint(dst, input, graph)?,
            props.clone(),
        )?
    } else {
        graph.insert_node(name, props.clone())
    };
    for (key, value) in props { graph.set_gremlin_property(&element, &key, value)?; }
    graph.assign_generated_public_id(&element)?;
    if let Some(id) = public_id {
        graph.set_element_public_id(&element, id)?;
    }
    Ok(element)
}

fn parse_cardinality(cardinality: &str) -> IrResult<crate::ir::catalog::Cardinality> {
    use crate::ir::catalog::Cardinality;
    match cardinality {
        "single" => Ok(Cardinality::Single),
        "list" => Ok(Cardinality::List),
        "set" => Ok(Cardinality::Set),
        _ => Err(error("Unknown vertex property cardinality")),
    }
}
