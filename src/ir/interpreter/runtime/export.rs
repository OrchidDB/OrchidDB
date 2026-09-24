//! Export native state from one snapshot and atomically replace the destination.
use crate::ir::{
    catalog::PropertyGraph,
    interpreter::{InterpretError, IrResult},
    value::Value,
};
use serde_json::{Map, Value as Json, json};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

fn error(message: impl Into<String>) -> InterpretError {
    InterpretError::Runtime(message.into())
}
fn typed(kind: &str, value: Json) -> Json {
    json!({"@type":kind,"@value":value})
}
fn float(value: f64) -> Json {
    if value.is_nan() {
        json!("NaN")
    } else if value == f64::INFINITY {
        json!("Infinity")
    } else if value == f64::NEG_INFINITY {
        json!("-Infinity")
    } else {
        json!(value)
    }
}
fn encode(value: &Value) -> IrResult<Json> {
    Ok(match value {
        Value::Null => Json::Null,
        Value::Bool(v) => json!(v),
        Value::String(v) => json!(v),
        Value::Byte(v) => typed("gx:Byte", json!(v)),
        Value::Short(v) => typed("gx:Int16", json!(v)),
        Value::Int(v) => {
            let value = i32::try_from(*v).map_err(|_| error("GraphSON Int32 out of range"))?;
            typed("g:Int32", json!(value))
        }
        Value::Long(v) => typed("g:Int64", json!(v)),
        Value::Float32(v) => typed("g:Float", float(*v as f64)),
        Value::Float(v) => typed("g:Double", float(*v)),
        Value::BigInt(v) => typed(
            "gx:BigInteger",
            serde_json::from_str(&v.to_string()).map_err(|e| error(e.to_string()))?,
        ),
        Value::BigDecimal(v) => typed(
            "gx:BigDecimal",
            serde_json::from_str(&v.to_string()).map_err(|e| error(e.to_string()))?,
        ),
        Value::List(values) | Value::Set(values) => typed(
            if matches!(value, Value::Set(_)) {
                "g:Set"
            } else {
                "g:List"
            },
            Json::Array(values.iter().map(encode).collect::<IrResult<_>>()?),
        ),
        Value::Map(values) => typed(
            "g:Map",
            Json::Array(
                values
                    .iter()
                    .map(|(k, v)| Ok([json!(k), encode(v)?]))
                    .collect::<IrResult<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
        ),
        Value::TypedMap(values) => typed(
            "g:Map",
            Json::Array(
                values
                    .iter()
                    .map(|(k, v)| Ok([encode(k)?, encode(v)?]))
                    .collect::<IrResult<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
        ),
        other => {
            return Err(error(format!(
                "Unsupported GraphSON property value: {other:?}"
            )));
        }
    })
}
fn property_map(graph: &PropertyGraph, owner: &Value) -> IrResult<Json> {
    let mut properties = Map::new();
    for property in graph.properties(owner, &[]) {
        if let Value::Property { key, value, .. } = property {
            properties.insert(key, encode(&value)?);
        }
    }
    Ok(Json::Object(properties))
}
fn graphml_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Bool(_)
            | Value::String(_)
            | Value::Int(_)
            | Value::Long(_)
            | Value::Float32(_)
            | Value::Float(_)
    )
}
fn validate_graphml(
    graph: &PropertyGraph,
    owner: &Value,
    types: &mut std::collections::BTreeMap<(bool, String), std::mem::Discriminant<Value>>,
) -> IrResult<()> {
    let mut keys = std::collections::BTreeSet::new();
    for property in graph.properties(owner, &[]) {
        let (key, value) = match &property {
            Value::VertexProperty { key, value, .. } | Value::Property { key, value, .. } => {
                (key, value)
            }
            _ => continue,
        };
        let kind = std::mem::discriminant(value.as_ref());
        if types
            .insert((matches!(owner, Value::Edge { .. }), key.clone()), kind)
            .is_some_and(|previous| previous != kind)
        {
            return Err(error(
                "GraphML requires one scalar type per property key; use GraphSON or Gryo",
            ));
        }
        if !keys.insert(key.clone())
            || !graphml_scalar(value)
            || !graph.properties(&property, &[]).is_empty()
        {
            return Err(error(
                "GraphML requires single scalar properties without meta-properties; use GraphSON or Gryo",
            ));
        }
    }
    Ok(())
}
fn graphson(graph: &PropertyGraph, graphml: bool) -> IrResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut graphml_types = std::collections::BTreeMap::new();
    for label in graph.labels() {
        for id in graph.node_ids(&label)? {
            let vertex = Value::Node {
                label: label.clone(),
                id,
            };
            if graphml {
                validate_graphml(graph, &vertex, &mut graphml_types)?;
            }
            let mut properties = Map::new();
            for property in graph.properties(&vertex, &[]) {
                if let Value::VertexProperty { key, value, .. } = &property {
                    let record = json!({"id":encode(&graph.element_public_id(&property))?, "value":encode(value)?, "properties":property_map(graph, &property)?});
                    properties
                        .entry(key.clone())
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .unwrap()
                        .push(record);
                }
            }
            let mut record = json!({"id":encode(&graph.element_public_id(&vertex))?, "label":label, "properties":properties});
            for incoming in [false, true] {
                let mut edges = Map::new();
                let adjacent = if incoming {
                    graph.in_edges(&label, id, &[])
                } else {
                    graph.out_edges(&label, id, &[])
                };
                for (rel_type, edge_id, other_label, other_id) in adjacent {
                    let other = Value::Node {
                        label: other_label.clone(),
                        id: other_id,
                    };
                    let (src_label, src_id, dst_label, dst_id) = if incoming {
                        (other_label, other_id, label.clone(), id)
                    } else {
                        (label.clone(), id, other_label, other_id)
                    };
                    let edge = Value::Edge {
                        rel_type: rel_type.clone(),
                        id: edge_id,
                        src_label,
                        src_id,
                        dst_label,
                        dst_id,
                        projected_properties: None,
                    };
                    if graphml {
                        validate_graphml(graph, &edge, &mut graphml_types)?;
                    }
                    let mut edge_record = json!({"id":encode(&graph.element_public_id(&edge))?, "properties":property_map(graph, &edge)?});
                    edge_record[if incoming { "outV" } else { "inV" }] =
                        encode(&graph.element_public_id(&other))?;
                    edges
                        .entry(rel_type)
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .unwrap()
                        .push(edge_record);
                }
                record[if incoming { "inE" } else { "outE" }] = Json::Object(edges);
            }
            serde_json::to_writer(&mut bytes, &record).map_err(|e| error(e.to_string()))?;
            bytes.push(b'\n');
        }
    }
    Ok(bytes)
}

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Staged {
    path: PathBuf,
    file: File,
}
impl Staged {
    fn create(destination: &Path) -> IrResult<Self> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        for _ in 0..100 {
            let path = parent.join(format!(
                ".crabgraph-export-{}-{}.tmp",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(error(format!("Cannot stage export: {e}"))),
            }
        }
        Err(error("Cannot reserve export staging file"))
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) fn write(graph: &PropertyGraph, path: &str, writer: &str) -> IrResult<()> {
    let destination = Path::new(path);
    let writer = if writer.is_empty() {
        match destination.extension().and_then(|v| v.to_str()) {
            Some("json" | "graphson") => "graphson",
            Some("xml" | "graphml") => "graphml",
            Some("kryo" | "gryo") => "gryo",
            _ => {
                return Err(error(
                    "Cannot infer writer from file extension; specify IO.writer",
                ));
            }
        }
    } else {
        writer
    };
    if !matches!(writer, "graphson" | "graphml" | "gryo") {
        return Err(error(format!("Unsupported IO.writer: {writer}")));
    }
    // Arrow base batches are immutable and the mutable overlay is cloned. All
    // property materialization and codec reads operate on this private snapshot.
    let snapshot = graph.clone();
    let bytes = graphson(&snapshot, writer == "graphml")?;
    let mut output = Staged::create(destination)?;
    if writer == "graphson" {
        output
            .file
            .write_all(&bytes)
            .map_err(|e| error(format!("Cannot write export: {e}")))?;
    } else {
        let classpath = std::env::var("CRABGRAPH_GREMLIN_IO_CLASSPATH").map_err(|_|error("GraphML/Gryo require CRABGRAPH_GREMLIN_IO_CLASSPATH pointing to the TinkerPop ExportGraph codec"))?;
        let java = std::env::var("CRABGRAPH_GREMLIN_IO_JAVA").unwrap_or_else(|_| "java".into());
        let mut source = Staged::create(destination)?;
        source
            .file
            .write_all(&bytes)
            .and_then(|_| source.file.flush())
            .map_err(|e| error(format!("Cannot stage native graph: {e}")))?;
        let result = Command::new(java)
            .args([
                "--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED",
                "--add-opens=java.base/java.lang=ALL-UNNAMED",
                "--add-opens=java.base/java.util=ALL-UNNAMED",
                "-cp",
                &classpath,
                "ExportGraph",
                writer,
            ])
            .arg(&source.path)
            .arg(&output.path)
            .output()
            .map_err(|e| error(format!("Cannot start export codec: {e}")))?;
        if !result.status.success() {
            return Err(error(format!(
                "{writer} writer failed: {}",
                String::from_utf8_lossy(&result.stderr)
            )));
        }
        if output
            .file
            .metadata()
            .map_err(|e| error(e.to_string()))?
            .len()
            == 0
        {
            return Err(error("Export codec produced an empty file"));
        }
    }
    output
        .file
        .sync_all()
        .map_err(|e| error(format!("Cannot flush export: {e}")))?;
    std::fs::rename(&output.path, destination)
        .map_err(|e| error(format!("Cannot replace export destination: {e}")))?;
    Ok(())
}
