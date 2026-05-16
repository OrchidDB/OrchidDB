//! GraphReturn boundary — Row stream → Arrow RecordBatch.
//!
//! Extracted from `interpreter.rs` lines 3313..3463.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Builder, RecordBatch, StringBuilder,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};

use crate::ir::catalog::PropertyGraph;
use crate::ir::policy::{GraphPlanPolicy, Language, ResultForm};
use crate::ir::value::Value;

use super::runtime::display_for_concat;
use super::{InterpretError, IrResult, ReturnedBatches, Row};

pub(crate) fn finalize_return(
    fields: &[String],
    result_form: ResultForm,
    rows: Vec<Row>,
    graph: &PropertyGraph,
    policy: &GraphPlanPolicy,
) -> IrResult<ReturnedBatches> {
    if fields.is_empty() {
        let schema: SchemaRef = Arc::new(Schema::empty());
        let batch = RecordBatch::new_empty(schema);
        return Ok(ReturnedBatches {
            fields: Vec::new(),
            result_form,
            batch,
        });
    }

    let expand_elements = matches!(policy.language, Language::Cypher);
    let mut columns: Vec<ColumnBuilder> = fields.iter().map(|_| ColumnBuilder::new()).collect();

    for row in &rows {
        for (idx, field) in fields.iter().enumerate() {
            let mut value = row.bindings.get(field).cloned().unwrap_or(Value::Null);
            if expand_elements {
                value = expand_element(value, graph);
            }
            columns[idx].push(value);
        }
    }

    let mut arrow_fields = Vec::with_capacity(fields.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(fields.len());
    for (name, builder) in fields.iter().zip(columns.into_iter()) {
        let (data_type, array) = builder.finish();
        arrow_fields.push(Field::new(name, data_type, true));
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(Schema::new(arrow_fields));
    let batch = RecordBatch::try_new(schema, arrays)
        .map_err(|err| InterpretError::Type(format!("record batch: {err}")))?;
    Ok(ReturnedBatches {
        fields: fields.to_vec(),
        result_form,
        batch,
    })
}

/// Type-promoting column builder: scans values to pick a primitive Arrow
/// type. Mixed types fall back to Utf8.
pub(crate) struct ColumnBuilder {
    values: Vec<Value>,
}

impl ColumnBuilder {
    fn new() -> Self {
        Self { values: Vec::new() }
    }

    fn push(&mut self, value: Value) {
        self.values.push(value);
    }

    fn finish(self) -> (DataType, ArrayRef) {
        let kind = infer_kind(&self.values);
        match kind {
            ColumnKind::Int => {
                let mut builder = Int64Builder::with_capacity(self.values.len());
                for v in self.values {
                    match v {
                        Value::Byte(n) => builder.append_value(n as i64),
                        Value::Short(n) => builder.append_value(n as i64),
                        Value::Int(n) | Value::Long(n) => builder.append_value(n),
                        Value::Null => builder.append_null(),
                        Value::Float32(f) => builder.append_value(f as i64),
                        Value::Float(f) => builder.append_value(f as i64),
                        _ => builder.append_null(),
                    }
                }
                (DataType::Int64, Arc::new(builder.finish()) as ArrayRef)
            }
            ColumnKind::Float => {
                let mut builder = Float64Builder::with_capacity(self.values.len());
                for v in self.values {
                    match v {
                        Value::Float(f) => builder.append_value(f),
                        Value::Float32(f) => builder.append_value(f as f64),
                        Value::Byte(n) => builder.append_value(n as f64),
                        Value::Short(n) => builder.append_value(n as f64),
                        Value::Int(n) | Value::Long(n) => builder.append_value(n as f64),
                        Value::Null => builder.append_null(),
                        _ => builder.append_null(),
                    }
                }
                (DataType::Float64, Arc::new(builder.finish()) as ArrayRef)
            }
            ColumnKind::Bool => {
                let mut builder = BooleanBuilder::with_capacity(self.values.len());
                for v in self.values {
                    match v {
                        Value::Bool(b) => builder.append_value(b),
                        Value::Null => builder.append_null(),
                        _ => builder.append_null(),
                    }
                }
                (DataType::Boolean, Arc::new(builder.finish()) as ArrayRef)
            }
            ColumnKind::Utf8 => {
                let mut builder = StringBuilder::new();
                for v in self.values {
                    match v {
                        Value::String(s) => builder.append_value(s),
                        Value::Null => builder.append_null(),
                        Value::Byte(n) => builder.append_value(format!("d[{n}].b")),
                        Value::Short(n) => builder.append_value(format!("d[{n}].s")),
                        Value::Int(n) => builder.append_value(n.to_string()),
                        Value::Long(n) => builder.append_value(format!("d[{n}].l")),
                        Value::Float32(f) => builder.append_value(format!("d[{}].f", f as f64)),
                        Value::Float(f) => builder.append_value(f.to_string()),
                        Value::Bool(b) => builder.append_value(b.to_string()),
                        // Tag arbitrary-precision values with TinkerPop's
                        // `d[N].m` / `d[N].i` shape so the harness can
                        // compare against the expected gherkin lines.
                        Value::BigDecimal(d) => builder.append_value(format!("d[{d}].m")),
                        Value::BigInt(n) => builder.append_value(format!("d[{n}].n")),
                        Value::DateTime(s) => builder.append_value(format!("dt[{s}]")),
                        Value::Node { label, id } => {
                            builder.append_value(format!("{label}#{id}"));
                        }
                        Value::Edge { rel_type, id, .. } => {
                            builder.append_value(format!("{rel_type}#{id}"));
                        }
                        other => builder.append_value(display_for_concat(&other)),
                    }
                }
                (DataType::Utf8, Arc::new(builder.finish()) as ArrayRef)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColumnKind {
    Int,
    Float,
    Bool,
    Utf8,
}

/// Expand `Value::Node` and `Value::Edge` into Kuzu-style property maps
/// rendered as `Value::String` literals before they reach the Arrow
/// boundary. Cypher conformance cases expect nodes/edges to print as the
/// schema-ordered property bag (`{_ID: 0:0, _LABEL: person, ID: 0, ...}`),
/// not the structured `v[label#id]` form used by the Gremlin formatter.
/// We bake the formatting in here because [`Value::Map`] is a
/// `BTreeMap`, which would alphabetise keys and break the ordering
/// downstream. Recurses into `List`, `Map`, and `Path` so a
/// `collect(n)` / `nodes(p)` projection rewrites every element.
pub(crate) fn expand_element(value: Value, graph: &PropertyGraph) -> Value {
    match value {
        Value::Node { label, id } => Value::String(format_node(graph, &label, id)),
        Value::Edge {
            rel_type,
            id,
            src_label,
            src_id,
            dst_label,
            dst_id,
        } => Value::String(format_edge(
            graph, &rel_type, id, &src_label, src_id, &dst_label, dst_id,
        )),
        Value::List(items) => Value::List(
            items
                .into_iter()
                .map(|item| expand_element(item, graph))
                .collect(),
        ),
        Value::Map(map) => Value::Map(
            map.into_iter()
                .map(|(k, v)| (k, expand_element(v, graph)))
                .collect(),
        ),
        Value::Path(items) => Value::Path(
            items
                .into_iter()
                .map(|item| expand_element(item, graph))
                .collect(),
        ),
        other => other,
    }
}

fn node_table_index(graph: &PropertyGraph, label: &str) -> i64 {
    graph
        .node_label_order()
        .iter()
        .position(|candidate| candidate == label)
        .map(|idx| idx as i64)
        .unwrap_or(0)
}

fn edge_table_index(graph: &PropertyGraph, rel_type: &str) -> i64 {
    // Kuzu numbers edge tables after node tables in the same shared
    // namespace, so a graph with N node tables prints edges as `N:row`.
    let node_count = graph.node_label_order().len() as i64;
    graph
        .edge_rel_order()
        .iter()
        .position(|candidate| candidate == rel_type)
        .map(|idx| node_count + idx as i64)
        .unwrap_or(node_count)
}

fn format_node(graph: &PropertyGraph, label: &str, id: i64) -> String {
    let table_id = node_table_index(graph, label);
    let mut parts = vec![
        format!("_ID: {table_id}:{id}"),
        format!("_LABEL: {label}"),
    ];
    for key in graph.node_property_keys(label) {
        let value = graph.node_property(label, id, &key);
        if matches!(value, Value::Null) {
            continue;
        }
        parts.push(format!("{key}: {}", format_property_value(&value)));
    }
    format!("{{{}}}", parts.join(", "))
}

fn format_edge(
    graph: &PropertyGraph,
    rel_type: &str,
    id: i64,
    src_label: &str,
    src_id: i64,
    dst_label: &str,
    dst_id: i64,
) -> String {
    let table_id = edge_table_index(graph, rel_type);
    let src_table = node_table_index(graph, src_label);
    let dst_table = node_table_index(graph, dst_label);
    // Kuzu prints edges with `_LABEL` before `_ID`; nodes use the
    // reverse order. Mirror that convention so the case files match
    // without per-line tweaks.
    let mut parts = vec![
        format!("_LABEL: {rel_type}"),
        format!("_ID: {table_id}:{id}"),
    ];
    for key in graph.edge_property_keys(rel_type) {
        let value = graph.edge_property(rel_type, id, &key);
        if matches!(value, Value::Null) {
            continue;
        }
        parts.push(format!("{key}: {}", format_property_value(&value)));
    }
    format!(
        "({src_table}:{src_id})-{{{body}}}->({dst_table}:{dst_id})",
        body = parts.join(", ")
    )
}

/// Print a property value the way Kuzu does inside the schema-ordered
/// node/edge map: scalars are unquoted, strings are unquoted (the case
/// files don't surround them with `'…'`), booleans capitalise, floats
/// keep their decimal point, lists/maps recurse.
fn format_property_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Byte(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::Int(n) | Value::Long(n) => n.to_string(),
        Value::Float32(f) => format_float(*f as f64),
        Value::Float(f) => format_float(*f),
        Value::BigInt(n) => n.to_string(),
        Value::BigDecimal(d) => d.to_string(),
        Value::DateTime(s) => s.clone(),
        Value::List(items) => {
            let body: Vec<String> = items.iter().map(format_property_value).collect();
            format!("[{}]", body.join(","))
        }
        Value::Map(map) => {
            let body: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_property_value(v)))
                .collect();
            format!("{{{}}}", body.join(", "))
        }
        Value::Path(items) => {
            let body: Vec<String> = items.iter().map(format_property_value).collect();
            format!("[{}]", body.join(","))
        }
        Value::Node { label, id } => format!("{label}#{id}"),
        Value::Edge { rel_type, id, .. } => format!("{rel_type}#{id}"),
    }
}

fn format_float(value: f64) -> String {
    // Kuzu prints all doubles with six trailing decimals (`5.000000`,
    // `1.731000`); follow that convention for property-bag output so
    // the case files can compare directly. Tail zeros stay because
    // Kuzu does not trim them either.
    if !value.is_finite() {
        return value.to_string();
    }
    format!("{value:.6}")
}

pub(crate) fn infer_kind(values: &[Value]) -> ColumnKind {
    let mut kind: Option<ColumnKind> = None;
    for v in values {
        let candidate = match v {
            Value::Null => continue,
            Value::Byte(_) | Value::Short(_) | Value::Int(_) | Value::Long(_) => ColumnKind::Int,
            Value::Float32(_) | Value::Float(_) => ColumnKind::Float,
            Value::Bool(_) => ColumnKind::Bool,
            _ => ColumnKind::Utf8,
        };
        kind = Some(match kind {
            None => candidate,
            Some(existing) if existing == candidate => existing,
            Some(ColumnKind::Int) if matches!(candidate, ColumnKind::Float) => ColumnKind::Float,
            Some(ColumnKind::Float) if matches!(candidate, ColumnKind::Int) => ColumnKind::Float,
            _ => ColumnKind::Utf8,
        });
    }
    kind.unwrap_or(ColumnKind::Utf8)
}
