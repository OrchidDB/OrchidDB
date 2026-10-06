//! Typed property-graph import shared by the host API and original test provider.
use super::PropertyGraph;
use crate::ir::value::Value as GValue;
use serde_json::Value;
use std::collections::BTreeMap;
pub fn parameter(v: &Value) -> GValue {
    match v {
        Value::Null => GValue::Null,
        Value::Bool(x) => GValue::Bool(*x),
        Value::Number(x) => {
            if let Some(i) = x.as_i64() {
                GValue::Int(i)
            } else {
                GValue::Float(x.as_f64().unwrap())
            }
        }
        Value::String(s) => GValue::String(s.clone()),
        Value::Array(a) => GValue::List(a.iter().map(parameter).collect()),
        Value::Object(m) => GValue::Map(m.iter().map(|(k, v)| (k.clone(), parameter(v))).collect()),
    }
}
pub fn typed_property(value: &Value, declared: Option<&str>) -> Result<GValue, String> {
    let invalid = || format!("Fixture value {value} does not match declared type {declared:?}");
    match declared {
        None => Ok(parameter(value)),
        Some("Integer") => value
            .as_i64()
            .filter(|v| i32::try_from(*v).is_ok())
            .map(GValue::Int)
            .ok_or_else(invalid),
        Some("Long") => value.as_i64().map(GValue::Long).ok_or_else(invalid),
        Some("Byte") => value
            .as_i64()
            .and_then(|v| i8::try_from(v).ok())
            .map(GValue::Byte)
            .ok_or_else(invalid),
        Some("Short") => value
            .as_i64()
            .and_then(|v| i16::try_from(v).ok())
            .map(GValue::Short)
            .ok_or_else(invalid),
        Some("Float") => value
            .as_f64()
            .map(|v| GValue::Float32(v as f32))
            .ok_or_else(invalid),
        Some("Double") => value.as_f64().map(GValue::Float).ok_or_else(invalid),
        Some("String") => value
            .as_str()
            .map(|v| GValue::String(v.to_owned()))
            .ok_or_else(invalid),
        Some("Boolean") => value.as_bool().map(GValue::Bool).ok_or_else(invalid),
        Some(other) => Err(format!("Unmapped fixture property type {other}")),
    }
}
pub fn import_graph(req: &Value) -> Result<PropertyGraph, String> {
    fn properties(item: &Value) -> Result<BTreeMap<String, GValue>, String> {
        item["properties"]
            .as_object()
            .ok_or("Fixture properties must be an object")?
            .iter()
            .map(|(key, value)| {
                typed_property(value, item["property_types"][key].as_str())
                    .map(|v| (key.clone(), v))
            })
            .collect()
    }
    let graph = PropertyGraph::new();
    graph.enable_null_property_values(req["allow_null_property_values"].as_bool().unwrap_or(false));
    let mut nodes = BTreeMap::new();
    for n in req["nodes"]
        .as_array()
        .ok_or("Fixture nodes must be an array")?
    {
        let v = graph.insert_node(
            n["label"].as_str().ok_or("Fixture node label missing")?,
            if n["property_records"].is_array() {
                BTreeMap::new()
            } else {
                properties(n)?
            },
        );
        graph
            .set_element_public_id(&v, typed_property(&n["id"], n["id_type"].as_str())?)
            .map_err(|e| e.to_string())?;
        if let Some(records) = n["property_records"].as_array() {
            for record in records {
                let key = record["key"]
                    .as_str()
                    .ok_or("Fixture property key missing")?;
                let value = typed_property(&record["value"], record["type"].as_str())?;
                let mut meta = BTreeMap::new();
                if let Some(entries) = record["meta"].as_object() {
                    for (key, value) in entries {
                        meta.insert(
                            key.clone(),
                            typed_property(value, record["meta_types"][key].as_str())?,
                        );
                    }
                }
                let property = graph
                    .set_vertex_property(
                        &v,
                        key,
                        value,
                        crate::ir::catalog::Cardinality::List,
                        meta,
                    )
                    .map_err(|e| e.to_string())?;
                if !record["id"].is_null() {
                    graph
                        .set_vertex_property_public_id(
                            &property,
                            typed_property(&record["id"], record["id_type"].as_str())?,
                        )
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        nodes.insert(n["id"].to_string(), v);
    }
    for e in req["edges"]
        .as_array()
        .ok_or("Fixture edges must be an array")?
    {
        let src = nodes
            .get(&e["src"].to_string())
            .ok_or("Fixture edge source missing")?;
        let dst = nodes
            .get(&e["dst"].to_string())
            .ok_or("Fixture edge target missing")?;
        let edge = graph
            .insert_edge(
                e["label"].as_str().ok_or("Fixture edge label missing")?,
                src,
                dst,
                properties(e)?,
            )
            .map_err(|e| e.to_string())?;
        graph
            .set_element_public_id(&edge, typed_property(&e["id"], e["id_type"].as_str())?)
            .map_err(|e| e.to_string())?;
    }

    Ok(graph)
}
