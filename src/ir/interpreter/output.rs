//! GraphReturn boundary — Row stream → Arrow RecordBatch.
//!
//! Extracted from `interpreter.rs` lines 3313..3463.

use std::collections::BTreeSet;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Builder, RecordBatch, StringBuilder,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};

use crate::ir::catalog::PropertyGraph;
use crate::ir::policy::{GraphPlanPolicy, Language, ResultForm};
use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY, Value};

use super::element_id::{edge_table_index, node_table_index};

const KUZU_MAP_ENTRIES_KEY: &str = "\u{0}kuzu_map_entries";
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
    let return_fields = plan_return_fields(fields, &rows, expand_elements);
    let output_fields = return_fields
        .iter()
        .flat_map(ReturnField::output_names)
        .collect::<Vec<_>>();
    let mut columns: Vec<ColumnBuilder> = output_fields
        .iter()
        .map(|_| ColumnBuilder::new(expand_elements))
        .collect();

    for row in &rows {
        let mut column_idx = 0;
        for field in &return_fields {
            match field {
                ReturnField::Scalar { source, .. } => {
                    let mut value = row.bindings.get(source).cloned().unwrap_or(Value::Null);
                    if expand_elements {
                        value = expand_element(value, graph);
                    }
                    columns[column_idx].push(value);
                    column_idx += 1;
                }
                ReturnField::Star { source, keys, .. } => {
                    let value = row.bindings.get(source).cloned().unwrap_or(Value::Null);
                    for key in keys {
                        let mut value = star_projection_value(&value, key);
                        if expand_elements {
                            value = expand_element(value, graph);
                        }
                        columns[column_idx].push(value);
                        column_idx += 1;
                    }
                }
            }
        }
    }

    let mut arrow_fields = Vec::with_capacity(output_fields.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(output_fields.len());
    for (name, builder) in output_fields.iter().zip(columns.into_iter()) {
        let (data_type, array) = builder.finish();
        arrow_fields.push(Field::new(name.as_str(), data_type, true));
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(Schema::new(arrow_fields));
    let batch = RecordBatch::try_new(schema, arrays)
        .map_err(|err| InterpretError::Type(format!("record batch: {err}")))?;
    Ok(ReturnedBatches {
        fields: output_fields,
        result_form,
        batch,
    })
}

#[derive(Debug, Clone)]
enum ReturnField {
    Scalar {
        source: String,
        output: String,
    },
    Star {
        source: String,
        keys: Vec<String>,
        outputs: Vec<String>,
    },
}

impl ReturnField {
    fn output_names(&self) -> Vec<String> {
        match self {
            ReturnField::Scalar { output, .. } => vec![output.clone()],
            ReturnField::Star { outputs, .. } => outputs.clone(),
        }
    }
}

fn plan_return_fields(fields: &[String], rows: &[Row], expand_star: bool) -> Vec<ReturnField> {
    let mut used = BTreeSet::new();
    let mut planned = Vec::new();
    for field in fields {
        if expand_star && is_star_projection_field(field) {
            let keys = star_projection_keys(field, rows);
            if !keys.is_empty() {
                let outputs = keys
                    .iter()
                    .map(|key| unique_output_name(&mut used, key))
                    .collect();
                planned.push(ReturnField::Star {
                    source: field.clone(),
                    keys,
                    outputs,
                });
                continue;
            }
        }
        planned.push(ReturnField::Scalar {
            source: field.clone(),
            output: unique_output_name(&mut used, field),
        });
    }
    planned
}

fn is_star_projection_field(field: &str) -> bool {
    field.trim_end().ends_with(".*")
}

fn star_projection_keys(field: &str, rows: &[Row]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut keys = Vec::new();
    for row in rows {
        let Some(Value::Map(map)) = row.bindings.get(field) else {
            continue;
        };
        for key in visible_map_keys(map) {
            if seen.insert(key.clone()) {
                keys.push(key);
            }
        }
    }
    keys
}

fn star_projection_value(value: &Value, key: &str) -> Value {
    match value {
        Value::Map(map) => map.get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn unique_output_name(used: &mut BTreeSet<String>, preferred: &str) -> String {
    if used.insert(preferred.to_string()) {
        return preferred.to_string();
    }
    let mut index = 1;
    loop {
        let candidate = format!("{preferred}_{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

/// Type-promoting column builder: scans values to pick a primitive Arrow
/// type. Mixed types fall back to Utf8.
pub(crate) struct ColumnBuilder {
    values: Vec<Value>,
    cypher_output: bool,
}

impl ColumnBuilder {
    fn new(cypher_output: bool) -> Self {
        Self {
            values: Vec::new(),
            cypher_output,
        }
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
                        Value::UInt8(n) => builder.append_value(n as i64),
                        Value::Short(n) => builder.append_value(n as i64),
                        Value::UInt16(n) => builder.append_value(n as i64),
                        Value::Int(n) | Value::Long(n) => builder.append_value(n),
                        Value::UInt32(n) => builder.append_value(n as i64),
                        Value::UInt64(n) => match i64::try_from(n) {
                            Ok(n) => builder.append_value(n),
                            Err(_) => builder.append_null(),
                        },
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
                        Value::UInt8(n) => builder.append_value(n as f64),
                        Value::Short(n) => builder.append_value(n as f64),
                        Value::UInt16(n) => builder.append_value(n as f64),
                        Value::Int(n) | Value::Long(n) => builder.append_value(n as f64),
                        Value::UInt32(n) => builder.append_value(n as f64),
                        Value::UInt64(n) => builder.append_value(n as f64),
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
                        Value::UInt8(n) if self.cypher_output => {
                            builder.append_value(n.to_string())
                        }
                        Value::UInt8(n) => builder.append_value(format!("d[{n}].u8")),
                        Value::Short(n) => builder.append_value(format!("d[{n}].s")),
                        Value::UInt16(n) if self.cypher_output => {
                            builder.append_value(n.to_string())
                        }
                        Value::UInt16(n) => builder.append_value(format!("d[{n}].u16")),
                        Value::Int(n) => builder.append_value(n.to_string()),
                        Value::UInt32(n) if self.cypher_output => {
                            builder.append_value(n.to_string())
                        }
                        Value::UInt32(n) => builder.append_value(format!("d[{n}].u32")),
                        Value::Long(n) => builder.append_value(format!("d[{n}].l")),
                        Value::UInt64(n) if self.cypher_output => {
                            builder.append_value(n.to_string())
                        }
                        Value::UInt64(n) => builder.append_value(format!("d[{n}].u64")),
                        Value::Float32(f) => builder.append_value(format!("d[{}].f", f as f64)),
                        Value::Float(f) => builder.append_value(f.to_string()),
                        Value::Bool(b) => builder.append_value(b.to_string()),
                        // Gremlin expects tagged arbitrary-precision and
                        // datetime displays; Cypher/Kuzu prints them plain.
                        Value::BigDecimal(d) if self.cypher_output => {
                            builder.append_value(d.to_string())
                        }
                        Value::BigDecimal(d) => builder.append_value(format!("d[{d}].m")),
                        Value::BigInt(n) if self.cypher_output => {
                            builder.append_value(n.to_string())
                        }
                        Value::BigInt(n) => builder.append_value(format!("d[{n}].n")),
                        Value::UInt128(n) if self.cypher_output => {
                            builder.append_value(n.to_string())
                        }
                        Value::UInt128(n) => builder.append_value(format!("d[{n}].u128")),
                        Value::DateTime(s) if self.cypher_output => builder.append_value(s),
                        Value::DateTime(s) => builder.append_value(format!("dt[{s}]")),
                        Value::Node { label, id } => {
                            builder.append_value(format!("{label}#{id}"));
                        }
                        Value::Edge { rel_type, id, .. } => {
                            builder.append_value(format!("{rel_type}#{id}"));
                        }
                        other @ (Value::List(_) | Value::Map(_) | Value::Path(_))
                            if self.cypher_output =>
                        {
                            builder.append_value(format_property_value(&other))
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
            projected_properties,
        } => Value::String(format_edge(
            graph,
            &rel_type,
            id,
            &src_label,
            src_id,
            &dst_label,
            dst_id,
            projected_properties.as_deref(),
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

fn format_node(graph: &PropertyGraph, label: &str, id: i64) -> String {
    let table_id = node_table_index(graph, label);
    let mut parts = vec![format!("_ID: {table_id}:{id}"), format!("_LABEL: {label}")];
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
    projected_properties: Option<&[String]>,
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
    let keys = projected_properties
        .map(|keys| keys.to_vec())
        .unwrap_or_else(|| graph.edge_property_keys(rel_type));
    for key in keys {
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
        Value::String(s) => format_bytea_display(s).unwrap_or_else(|| unescape_display_quotes(s)),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Byte(n) => n.to_string(),
        Value::UInt8(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::UInt16(n) => n.to_string(),
        Value::Int(n) | Value::Long(n) => n.to_string(),
        Value::UInt32(n) => n.to_string(),
        Value::UInt64(n) => n.to_string(),
        Value::Float32(f) => format_float(*f as f64),
        Value::Float(f) => format_float(*f),
        Value::BigInt(n) => n.to_string(),
        Value::UInt128(n) => n.to_string(),
        Value::BigDecimal(d) => d.to_string(),
        Value::DateTime(s) => s.clone(),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::List(items) => {
            let body: Vec<String> = items.iter().map(format_property_value).collect();
            format!("[{}]", body.join(","))
        }
        Value::Map(map) => {
            if let Some(value) = union_display_value(map) {
                return format_property_value(value);
            }
            if let Some(entries) = kuzu_map_entries(map) {
                let body = entries
                    .iter()
                    .filter_map(kuzu_map_entry)
                    .map(|(key, value)| {
                        format!(
                            "{}={}",
                            format_property_value(key),
                            format_property_value(value)
                        )
                    })
                    .collect::<Vec<_>>();
                return format!("{{{}}}", body.join(", "));
            }
            let body: Vec<String> = visible_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key)
                        .map(|value| format!("{key}: {}", format_property_value(value)))
                })
                .collect();
            format!("{{{}}}", body.join(", "))
        }
        Value::Path(items) => {
            let mut nodes = Vec::new();
            let mut rels = Vec::new();
            for item in items {
                match item {
                    Value::Node { .. } => nodes.push(format_property_value(item)),
                    Value::Edge { .. } => rels.push(format_property_value(item)),
                    Value::String(text) if text.starts_with("{_ID:") => {
                        nodes.push(format_property_value(item))
                    }
                    Value::String(text) if text.starts_with('(') => {
                        rels.push(format_property_value(item))
                    }
                    _ => {}
                }
            }
            format!(
                "{{_NODES: [{}], _RELS: [{}]}}",
                nodes.join(","),
                rels.join(",")
            )
        }
        Value::Node { label, id } => format!("{label}#{id}"),
        Value::Edge { rel_type, id, .. } => format!("{rel_type}#{id}"),
    }
}

fn unescape_display_quotes(text: &str) -> String {
    let mut out = text.to_string();
    while out.contains("\\\"") {
        out = out.replace("\\\"", "\"");
    }
    while out.contains("\\'") {
        out = out.replace("\\'", "'");
    }
    out
}

fn format_bytea_display(text: &str) -> Option<String> {
    if !text.starts_with("\\x") {
        return None;
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(hex) = rest.strip_prefix("\\x") {
        if hex.len() < 2 {
            return None;
        }
        let byte = u8::from_str_radix(&hex[..2], 16).ok()?;
        if (0x20..=0x7e).contains(&byte) && byte != b'\\' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("\\x{byte:02X}"));
        }
        rest = &hex[2..];
    }
    if rest.is_empty() { Some(out) } else { None }
}

fn kuzu_map_entries(map: &std::collections::BTreeMap<String, Value>) -> Option<&[Value]> {
    let Value::List(entries) = map.get(KUZU_MAP_ENTRIES_KEY)? else {
        return None;
    };
    Some(entries.as_slice())
}

fn kuzu_map_entry(entry: &Value) -> Option<(&Value, &Value)> {
    let Value::List(items) = entry else {
        return None;
    };
    let [key, value] = items.as_slice() else {
        return None;
    };
    Some((key, value))
}

fn union_display_value(map: &std::collections::BTreeMap<String, Value>) -> Option<&Value> {
    match (map.get("__tag"), map.get("__value")) {
        (Some(Value::String(_)), Some(value)) => Some(value),
        _ => None,
    }
}

fn visible_map_keys(map: &std::collections::BTreeMap<String, Value>) -> Vec<String> {
    if let Some(order) = struct_field_order(map) {
        return order;
    }
    map.keys()
        .filter(|key| is_visible_map_key(key))
        .cloned()
        .collect()
}

fn struct_field_order(map: &std::collections::BTreeMap<String, Value>) -> Option<Vec<String>> {
    let Value::List(items) = map.get(STRUCT_ORDER_KEY)? else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(|item| match item {
                Value::String(key) if map.contains_key(key) => Some(key.clone()),
                _ => None,
            })
            .collect(),
    )
}

fn is_visible_map_key(key: &str) -> bool {
    key != STRUCT_ORDER_KEY && key != STRUCT_TYPES_KEY && key != KUZU_MAP_ENTRIES_KEY
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
            Value::Byte(_)
            | Value::UInt8(_)
            | Value::Short(_)
            | Value::UInt16(_)
            | Value::Int(_)
            | Value::UInt32(_)
            | Value::Long(_) => ColumnKind::Int,
            Value::UInt64(n) if i64::try_from(*n).is_ok() => ColumnKind::Int,
            Value::UInt64(_) => ColumnKind::Utf8,
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

#[cfg(test)]
mod tests {
    use super::format_property_value;
    use crate::ir::value::Value;

    #[test]
    fn cypher_path_output_uses_node_and_relationship_sections() {
        let rendered = format_property_value(&Value::Path(vec![
            Value::String("{_ID: 0:0, _LABEL: person}".into()),
            Value::String("({0:0})-{_LABEL: knows, _ID: 3:0}->(0:1)".into()),
            Value::String("{_ID: 0:1, _LABEL: person}".into()),
        ]));

        assert_eq!(
            rendered,
            "{_NODES: [{_ID: 0:0, _LABEL: person},{_ID: 0:1, _LABEL: person}], _RELS: [({0:0})-{_LABEL: knows, _ID: 3:0}->(0:1)]}"
        );

        let rendered = format_property_value(&Value::Path(vec![
            Value::String("({0:0})-{_LABEL: knows, _ID: 3:0}->(0:1)".into()),
            Value::String("{_ID: 0:1, _LABEL: person}".into()),
        ]));
        assert_eq!(
            rendered,
            "{_NODES: [{_ID: 0:1, _LABEL: person}], _RELS: [({0:0})-{_LABEL: knows, _ID: 3:0}->(0:1)]}"
        );
    }

    #[test]
    fn cypher_bytea_output_decodes_printable_bytes() {
        assert_eq!(
            format_property_value(&Value::String("\\x3A\\xA3".into())),
            ":\\xA3"
        );
        assert_eq!(
            format_property_value(&Value::String("\\xAA\\xBB".into())),
            "\\xAA\\xBB"
        );
    }
}
