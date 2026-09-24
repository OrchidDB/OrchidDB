//! File readers stage and validate a complete graph before applying writes.
//! GraphEngine wraps this write procedure in its normal statement transaction.
use crate::ir::{
    catalog::PropertyGraph,
    interpreter::{InterpretError, IrResult},
    value::Value,
};
use serde_json::Value as Json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

fn error(message: impl Into<String>) -> InterpretError {
    InterpretError::Runtime(message.into())
}
fn field<'a>(value: &'a Json, key: &str) -> IrResult<&'a Json> {
    value
        .get(key)
        .ok_or_else(|| error(format!("GraphSON missing {key}")))
}
fn string(value: &Json) -> IrResult<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| error("GraphSON expected string"))
}
// GraphSON X writes arbitrary-precision values as JSON numbers. Accept older
// string payloads too, retaining the original numeric text and decimal scale.
fn number_text(value: &Json) -> IrResult<String> {
    match value {
        Json::String(v) => Ok(v.clone()),
        Json::Number(v) => Ok(v.to_string()),
        _ => Err(error("GraphSON expected number or numeric string")),
    }
}
fn object(value: &Json) -> IrResult<&serde_json::Map<String, Json>> {
    value
        .as_object()
        .ok_or_else(|| error("GraphSON expected object"))
}
fn array(value: &Json) -> IrResult<&Vec<Json>> {
    value
        .as_array()
        .ok_or_else(|| error("GraphSON expected array"))
}
fn integer(value: &Json) -> IrResult<i64> {
    value
        .as_i64()
        .ok_or_else(|| error("GraphSON expected signed integer"))
}
fn float(value: &Json) -> IrResult<f64> {
    match value.as_str() {
        Some("NaN") => Ok(f64::NAN),
        Some("Infinity") => Ok(f64::INFINITY),
        Some("-Infinity") => Ok(f64::NEG_INFINITY),
        _ => value
            .as_f64()
            .ok_or_else(|| error("GraphSON expected floating point number")),
    }
}

/// Decode GraphSON's declared numeric types without inferring width from magnitude.
fn decode(value: &Json) -> IrResult<Value> {
    if let Some(kind) = value.get("@type").and_then(Json::as_str) {
        let v = field(value, "@value")?;
        return Ok(match kind {
            "g:Int32" => Value::Int(
                i32::try_from(integer(v)?).map_err(|_| error("Int32 out of range"))? as i64,
            ),
            "g:Int64" => Value::Long(integer(v)?),
            "gx:Byte" => {
                Value::Byte(i8::try_from(integer(v)?).map_err(|_| error("Byte out of range"))?)
            }
            "gx:Int16" => {
                Value::Short(i16::try_from(integer(v)?).map_err(|_| error("Int16 out of range"))?)
            }
            "g:Float" => Value::Float32(float(v)? as f32),
            "g:Double" => Value::Float(float(v)?),
            "gx:BigInteger" => Value::BigInt(
                number_text(v)?
                    .parse()
                    .map_err(|_| error("Invalid BigInteger"))?,
            ),
            "gx:BigDecimal" => Value::BigDecimal(
                number_text(v)?
                    .parse()
                    .map_err(|_| error("Invalid BigDecimal"))?,
            ),
            "g:List" => Value::List(array(v)?.iter().map(decode).collect::<IrResult<_>>()?),
            "g:Set" => crate::ir::value::gremlin_set(array(v)?.iter().map(decode).collect::<IrResult<_>>()?),
            "g:Map" => {
                let entries = array(v)?;
                if entries.len() % 2 != 0 {
                    return Err(error("GraphSON map requires key/value pairs"));
                }
                Value::TypedMap(
                    entries
                        .chunks_exact(2)
                        .map(|p| Ok((decode(&p[0])?, decode(&p[1])?)))
                        .collect::<IrResult<_>>()?,
                )
            }
            other => return Err(error(format!("Unsupported GraphSON value type: {other}"))),
        });
    }
    Ok(match value {
        Json::Null => Value::Null,
        Json::Bool(v) => Value::Bool(*v),
        Json::String(v) => Value::String(v.clone()),
        Json::Number(v) => {
            if let Some(n) = v.as_i64() {
                Value::Long(n)
            } else {
                Value::Float(float(value)?)
            }
        }
        Json::Array(v) => Value::List(v.iter().map(decode).collect::<IrResult<_>>()?),
        Json::Object(v) => Value::Map(
            v.iter()
                .map(|(k, v)| Ok((k.clone(), decode(v)?)))
                .collect::<IrResult<_>>()?,
        ),
    })
}

#[derive(Debug)]
struct VertexProperty {
    id: Option<Value>,
    value: Value,
    meta: BTreeMap<String, Value>,
}
#[derive(Debug)]
struct Vertex {
    id: Value,
    key: String,
    label: String,
    properties: BTreeMap<String, Vec<VertexProperty>>,
}
#[derive(Debug)]
struct Edge {
    id: Value,
    label: String,
    src: String,
    dst: String,
    properties: BTreeMap<String, Value>,
}
#[derive(Debug, Default)]
struct Import {
    vertices: Vec<Vertex>,
    edges: Vec<Edge>,
}
fn identity(value: &Json) -> IrResult<(Value, String)> {
    let id = decode(value)?;
    if !matches!(id, Value::Int(_) | Value::Long(_) | Value::String(_)) {
        return Err(error("Unsupported GraphSON element ID type"));
    }
    Ok((id, value.to_string()))
}
fn properties(value: Option<&Json>) -> IrResult<BTreeMap<String, Value>> {
    match value {
        None => Ok(BTreeMap::new()),
        Some(value) => object(value)?
            .iter()
            .map(|(k, v)| Ok((k.clone(), decode(v)?)))
            .collect(),
    }
}

impl Import {
    fn parse(bytes: &[u8]) -> IrResult<Self> {
        let mut import = Self::default();
        let mut vertex_ids = BTreeSet::new();
        let mut edge_ids = BTreeSet::new();
        // GraphSON adjacency-list files contain one JSON object per vertex.
        // Streaming serde handles arbitrary whitespace and rejects a malformed tail.
        for record in serde_json::Deserializer::from_slice(bytes).into_iter::<Json>() {
            let record = record.map_err(|e| error(format!("Invalid GraphSON: {e}")))?;
            let (id, key) = identity(field(&record, "id")?)?;
            if !vertex_ids.insert(key.clone()) {
                return Err(error("Duplicate imported vertex ID"));
            }
            let label = string(field(&record, "label")?)?;
            let mut vertex_properties = BTreeMap::new();
            if let Some(props) = record.get("properties") {
                for (name, values) in object(props)? {
                    let mut records = Vec::new();
                    for property in array(values)? {
                        records.push(VertexProperty {
                            id: property.get("id").map(decode).transpose()?,
                            value: decode(field(property, "value")?)?,
                            meta: properties(property.get("properties"))?,
                        });
                    }
                    vertex_properties.insert(name.clone(), records);
                }
            }
            if let Some(labels) = record.get("outE") {
                for (label, edges) in object(labels)? {
                    for edge in array(edges)? {
                        let (id, edge_key) = identity(field(edge, "id")?)?;
                        if !edge_ids.insert(edge_key.clone()) {
                            return Err(error("Duplicate imported edge ID"));
                        }
                        import.edges.push(Edge {
                            id,
                            label: label.clone(),
                            src: key.clone(),
                            dst: identity(field(edge, "inV")?)?.1,
                            properties: properties(edge.get("properties"))?,
                        });
                    }
                }
            }
            import.vertices.push(Vertex {
                id,
                key,
                label,
                properties: vertex_properties,
            });
        }
        for edge in &import.edges {
            if !vertex_ids.contains(&edge.dst) {
                return Err(error("Imported edge has missing endpoint"));
            }
        }
        Ok(import)
    }

    fn apply(self, graph: &PropertyGraph) -> IrResult<()> {
        use crate::ir::catalog::Cardinality;
        let mut vertices = BTreeMap::new();
        for vertex in self.vertices {
            let element = graph.insert_node(vertex.label, BTreeMap::new());
            graph.set_element_public_id(&element, vertex.id)?;
            for (key, records) in vertex.properties {
                for record in records {
                    let property = graph.set_vertex_property(
                        &element, &key, record.value, Cardinality::List, record.meta,
                    )?;
                    if let Some(id) = record.id {
                        graph.set_vertex_property_public_id(&property, id)?;
                    }
                }
            }
            vertices.insert(vertex.key, element);
        }
        for edge in self.edges {
            let element = graph.insert_edge(
                edge.label,
                &vertices[&edge.src],
                &vertices[&edge.dst],
                BTreeMap::new(),
            )?;
            graph.set_element_public_id(&element, edge.id)?;
            for (key, value) in edge.properties {
                graph.set_gremlin_property(&element, &key, value)?;
            }
        }
        Ok(())
    }

}

pub(crate) fn read(graph: &PropertyGraph, path: &str, reader: &str) -> IrResult<()> {
    let reader = if reader.is_empty() {
        match Path::new(path).extension().and_then(|e| e.to_str()) {
            Some("json" | "graphson") => "graphson",
            Some("xml" | "graphml") => "graphml",
            Some("kryo" | "gryo") => "gryo",
            _ => {
                return Err(error(
                    "Cannot infer reader from file extension; specify IO.reader",
                ));
            }
        }
    } else {
        reader
    };
    let bytes = match reader {
        "graphson" => std::fs::read(path).map_err(|e| error(format!("Cannot read {path}: {e}")))?,
        "graphml" | "gryo" => {
            let classpath = std::env::var("CRABGRAPH_GREMLIN_IO_CLASSPATH").map_err(|_|error("GraphML/Gryo require CRABGRAPH_GREMLIN_IO_CLASSPATH pointing to the TinkerPop ImportGraph codec"))?;
            let java = std::env::var("CRABGRAPH_GREMLIN_IO_JAVA").unwrap_or_else(|_| "java".into());
            let output = Command::new(java)
                .args([
                    "--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED",
                    "--add-opens=java.base/java.lang=ALL-UNNAMED",
                    "--add-opens=java.base/java.util=ALL-UNNAMED",
                    "-cp",
                    &classpath,
                    "io.crabgraph.gremlin.codec.ImportGraph",
                    reader,
                    path,
                ])
                .output()
                .map_err(|e| error(format!("Cannot start import codec: {e}")))?;
            if !output.status.success() {
                return Err(error(format!(
                    "{reader} reader failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            output.stdout
        }
        _ => return Err(error(format!("Unsupported IO.reader: {reader}"))),
    };
    Import::parse(&bytes)?.apply(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_values_are_not_narrowed() {
        assert!(matches!(
            decode(&serde_json::json!({"@type":"g:Int64","@value":3})).unwrap(),
            Value::Long(3)
        ));
        assert!(matches!(
            decode(&serde_json::json!({"@type":"g:Int32","@value":3})).unwrap(),
            Value::Int(3)
        ));
        assert!(decode(&serde_json::json!({"@type":"g:Int32","@value":2147483648_i64})).is_err());
    }
    #[test]
    fn invalid_tail_and_missing_endpoints_are_rejected_before_writes() {
        assert!(Import::parse(br#"{"id":1,"label":"person"} {"id":"#).is_err());
        assert!(
            Import::parse(br#"{"id":1,"label":"person","outE":{"knows":[{"id":2,"inV":3}]}}"#)
                .is_err()
        );
        assert!(Import::parse(br#"{"id":1,"label":"person"} {"id":1,"label":"person"}"#).is_err());
    }
}
