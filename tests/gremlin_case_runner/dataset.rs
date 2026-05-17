//! Datasets exposed to the case runner.
//!
//! Built from knowledge of the standard TinkerPop fixtures. Only the
//! `modern` graph is fully realized; the rest return enough structure to
//! run their cases (or surface as unsupported when the harness can't
//! provide them).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};

use new_graph::ir::catalog::{
    EdgeTable, NodeTable, PropertyGraph, edges_from_columns, nodes_from_columns,
};
use new_graph::ir::value::Value;

#[derive(Debug)]
pub struct DatasetError(pub String);

impl std::fmt::Display for DatasetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn build(name: &str) -> Result<PropertyGraph, DatasetError> {
    match name {
        "modern" => Ok(modern_graph()),
        "empty" => Ok(empty_graph()),
        "crew" => Ok(crew_graph()),
        "grateful" => Err(DatasetError(
            "grateful dataset (770 vertices) not provided by harness".into(),
        )),
        "sink" => Err(DatasetError(
            "sink dataset (write-only) not relevant to the read interpreter".into(),
        )),
        other => Err(DatasetError(format!("unknown dataset `{other}`"))),
    }
}

pub fn build_with_initializer(
    name: &str,
    initializer: Option<&str>,
) -> Result<PropertyGraph, DatasetError> {
    let base = build(name)?;
    match initializer {
        Some(source) if !source.trim().is_empty() => apply_initializer(base, source),
        _ => Ok(base),
    }
}

#[derive(Debug, Clone)]
struct InitVertex {
    label: String,
    properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
struct InitEdge {
    rel_type: String,
    from: String,
    to: String,
    properties: BTreeMap<String, Value>,
}

#[derive(Debug)]
enum InitElement {
    Vertex(usize),
    Edge(usize),
}

fn apply_initializer(base: PropertyGraph, source: &str) -> Result<PropertyGraph, DatasetError> {
    if !base.nodes.is_empty() || !base.edges.is_empty() {
        return Err(DatasetError(
            "graph initializers are currently supported only with the empty dataset".into(),
        ));
    }

    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut aliases = HashMap::<String, usize>::new();
    let mut current: Option<InitElement> = None;

    for step in split_initializer_steps(source) {
        let step = step.trim();
        if step.is_empty() || step == "g" {
            continue;
        }
        if let Some(args) = call_args(step, "addV") {
            // `addV()` with no argument uses the TinkerPop default label.
            let label = if args.trim().is_empty() {
                "vertex".to_string()
            } else {
                parse_string_arg(args)?
            };
            vertices.push(InitVertex {
                label,
                properties: BTreeMap::new(),
            });
            current = Some(InitElement::Vertex(vertices.len() - 1));
        } else if let Some(args) = call_args(step, "addE") {
            let rel_type = parse_string_arg(args)?;
            edges.push(InitEdge {
                rel_type,
                from: String::new(),
                to: String::new(),
                properties: BTreeMap::new(),
            });
            current = Some(InitElement::Edge(edges.len() - 1));
        } else if let Some(args) = call_args(step, "property") {
            let (key, value) = parse_property_args(args)?;
            match current {
                Some(InitElement::Vertex(idx)) => {
                    vertices[idx].properties.insert(key, value);
                }
                Some(InitElement::Edge(idx)) => {
                    edges[idx].properties.insert(key, value);
                }
                None => {
                    return Err(DatasetError(
                        "initializer property() appeared before addV()/addE()".into(),
                    ));
                }
            }
        } else if let Some(args) = call_args(step, "as") {
            let alias = parse_string_arg(args)?;
            match current {
                Some(InitElement::Vertex(idx)) => {
                    aliases.insert(alias, idx);
                }
                _ => {
                    return Err(DatasetError(
                        "initializer as() is supported only after addV()".into(),
                    ));
                }
            }
        } else if let Some(args) = call_args(step, "from") {
            let alias = parse_string_arg(args)?;
            let Some(InitElement::Edge(idx)) = current else {
                return Err(DatasetError(
                    "initializer from() appeared before addE()".into(),
                ));
            };
            edges[idx].from = alias;
        } else if let Some(args) = call_args(step, "to") {
            let alias = parse_string_arg(args)?;
            let Some(InitElement::Edge(idx)) = current else {
                return Err(DatasetError(
                    "initializer to() appeared before addE()".into(),
                ));
            };
            edges[idx].to = alias;
        } else {
            return Err(DatasetError(format!(
                "unsupported graph initializer step `{step}`"
            )));
        }
    }

    build_initialized_graph(vertices, edges, aliases)
}

fn split_initializer_steps(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    let mut depth = 0usize;

    for ch in source.chars() {
        if let Some(q) = quote {
            buf.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                buf.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                buf.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                buf.push(ch);
            }
            '.' if depth == 0 => {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            ch if ch.is_whitespace() && buf.trim().is_empty() => {}
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn call_args<'a>(step: &'a str, name: &str) -> Option<&'a str> {
    let rest = step.strip_prefix(name)?;
    rest.strip_prefix('(')?.strip_suffix(')')
}

fn parse_property_args(args: &str) -> Result<(String, Value), DatasetError> {
    let parts = split_args(args);
    if parts.len() != 2 {
        return Err(DatasetError(format!(
            "initializer property() expected 2 arguments, got {}",
            parts.len()
        )));
    }
    Ok((parse_string_arg(&parts[0])?, parse_value_arg(&parts[1])?))
}

fn split_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    let mut depth = 0usize;

    for ch in args.chars() {
        if let Some(q) = quote {
            buf.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                buf.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                buf.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                buf.push(ch);
            }
            ',' if depth == 0 => {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn parse_string_arg(arg: &str) -> Result<String, DatasetError> {
    let token = arg.trim();
    let quote = token
        .chars()
        .next()
        .filter(|ch| *ch == '"' || *ch == '\'')
        .ok_or_else(|| DatasetError(format!("expected string argument, got `{token}`")))?;
    if !token.ends_with(quote) || token.len() < 2 {
        return Err(DatasetError(format!("unterminated string `{token}`")));
    }
    Ok(token[1..token.len() - 1].to_string())
}

fn parse_value_arg(arg: &str) -> Result<Value, DatasetError> {
    let token = arg.trim();
    if token.starts_with('"') || token.starts_with('\'') {
        return parse_string_arg(token).map(Value::String);
    }
    if let Some(args) = call_args(token, "datetime") {
        return parse_string_arg(args).map(Value::DateTime);
    }
    // UUID("...") uses the same tagged string representation produced by
    // the Gremlin parser for UUID literals, so equality, typeOf, and output
    // formatting all see one canonical runtime value.
    if let Some(args) = call_args(token, "UUID") {
        return parse_string_arg(args).map(|uuid| Value::String(format!("uuid[{uuid}]")));
    }
    // List / Set: `[...]` and `{...}`. Maps share `[` with lists but contain
    // `:` between keys and values — disambiguate before recursing.
    if token.starts_with('[') && token.ends_with(']') {
        let inner = &token[1..token.len() - 1];
        if looks_like_map(inner) {
            return parse_map_entries(inner).map(Value::Map);
        }
        return parse_list_entries(inner).map(Value::List);
    }
    if token.starts_with('{') && token.ends_with('}') {
        let mut values = Vec::new();
        for value in parse_list_entries(&token[1..token.len() - 1])? {
            if !values.contains(&value) {
                values.push(value);
            }
        }
        return Ok(Value::List(values));
    }
    // Numeric type suffixes (case-insensitive): L=long, I=int, S=short,
    // B=byte, F=float, D=double, BI=bigint, BD=bigdecimal.
    if let Some(rest) = strip_suffix_ci(token, "L") {
        return rest
            .parse::<i64>()
            .map(Value::Long)
            .map_err(|err| DatasetError(format!("invalid long literal `{token}`: {err}")));
    }
    if let Some(rest) = strip_suffix_ci(token, "I") {
        return rest
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|err| DatasetError(format!("invalid int literal `{token}`: {err}")));
    }
    if let Some(rest) = strip_suffix_ci(token, "S") {
        return rest
            .parse::<i16>()
            .map(Value::Short)
            .map_err(|err| DatasetError(format!("invalid short literal `{token}`: {err}")));
    }
    if let Some(rest) = strip_suffix_ci(token, "B") {
        return rest
            .parse::<i8>()
            .map(Value::Byte)
            .map_err(|err| DatasetError(format!("invalid byte literal `{token}`: {err}")));
    }
    if let Some(rest) = strip_suffix_ci(token, "F") {
        return rest
            .parse::<f32>()
            .map(Value::Float32)
            .map_err(|err| DatasetError(format!("invalid float literal `{token}`: {err}")));
    }
    if let Some(rest) = strip_suffix_ci(token, "d") {
        return rest
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|err| DatasetError(format!("invalid double literal `{token}`: {err}")));
    }
    if token.eq_ignore_ascii_case("true") {
        return Ok(Value::Bool(true));
    }
    if token.eq_ignore_ascii_case("false") {
        return Ok(Value::Bool(false));
    }
    if token.contains('.') {
        return token
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|err| DatasetError(format!("invalid float literal `{token}`: {err}")));
    }
    token
        .parse::<i64>()
        .map(Value::Int)
        .map_err(|err| DatasetError(format!("unsupported initializer value `{token}`: {err}")))
}

fn strip_suffix_ci<'a>(token: &'a str, suffix: &str) -> Option<&'a str> {
    if token.len() <= suffix.len() {
        return None;
    }
    let (head, tail) = token.split_at(token.len() - suffix.len());
    if !tail.eq_ignore_ascii_case(suffix) {
        return None;
    }
    // Reject if the head isn't numeric-looking — avoids stripping the trailing
    // letter off identifiers like `false`.
    if head
        .chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
    {
        Some(head)
    } else {
        None
    }
}

fn looks_like_map(inner: &str) -> bool {
    // Map entries look like `"key": value`. Scan for a top-level `:` outside
    // of strings and nested brackets.
    let mut quote: Option<char> = None;
    let mut escape = false;
    let mut depth = 0usize;
    for ch in inner.chars() {
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

fn parse_list_entries(inner: &str) -> Result<Vec<Value>, DatasetError> {
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_args(inner)
        .iter()
        .map(|s| parse_value_arg(s))
        .collect()
}

fn parse_map_entries(inner: &str) -> Result<BTreeMap<String, Value>, DatasetError> {
    let mut map = BTreeMap::new();
    if inner.trim().is_empty() {
        return Ok(map);
    }
    for entry in split_args(inner) {
        let mut quote: Option<char> = None;
        let mut escape = false;
        let mut depth = 0usize;
        let mut split_at: Option<usize> = None;
        for (i, ch) in entry.char_indices() {
            if let Some(q) = quote {
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == q {
                    quote = None;
                }
                continue;
            }
            match ch {
                '"' | '\'' => quote = Some(ch),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                ':' if depth == 0 => {
                    split_at = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(idx) = split_at else {
            return Err(DatasetError(format!("map entry missing `:` in `{entry}`")));
        };
        let key_tok = entry[..idx].trim();
        let val_tok = entry[idx + 1..].trim();
        let key = if key_tok.starts_with('"') || key_tok.starts_with('\'') {
            parse_string_arg(key_tok)?
        } else {
            key_tok.to_string()
        };
        map.insert(key, parse_value_arg(val_tok)?);
    }
    Ok(map)
}

fn build_initialized_graph(
    vertices: Vec<InitVertex>,
    edges: Vec<InitEdge>,
    aliases: HashMap<String, usize>,
) -> Result<PropertyGraph, DatasetError> {
    let mut graph = PropertyGraph::new();
    let mut label_rows = HashMap::<String, Vec<usize>>::new();
    let mut vertex_refs = Vec::<(String, i64)>::new();

    for (idx, vertex) in vertices.iter().enumerate() {
        let rows = label_rows.entry(vertex.label.clone()).or_default();
        vertex_refs.push((vertex.label.clone(), rows.len() as i64));
        rows.push(idx);
    }

    for (label, rows) in label_rows {
        graph.add_nodes(initialized_node_table(&label, &rows, &vertices)?);
    }

    let mut edge_groups = BTreeMap::<(String, String, String), Vec<&InitEdge>>::new();
    for edge in &edges {
        let Some(from_idx) = aliases.get(&edge.from).copied() else {
            return Err(DatasetError(format!(
                "initializer edge references unknown from() alias `{}`",
                edge.from
            )));
        };
        let Some(to_idx) = aliases.get(&edge.to).copied() else {
            return Err(DatasetError(format!(
                "initializer edge references unknown to() alias `{}`",
                edge.to
            )));
        };
        let (src_label, _) = &vertex_refs[from_idx];
        let (dst_label, _) = &vertex_refs[to_idx];
        edge_groups
            .entry((edge.rel_type.clone(), src_label.clone(), dst_label.clone()))
            .or_default()
            .push(edge);
    }

    for ((rel_type, src_label, dst_label), rows) in edge_groups {
        graph
            .add_edges(initialized_edge_table(
                &rel_type,
                &src_label,
                &dst_label,
                &rows,
                &aliases,
                &vertex_refs,
            )?)
            .map_err(|err| DatasetError(err.to_string()))?;
    }

    Ok(graph)
}

fn initialized_node_table(
    label: &str,
    rows: &[usize],
    vertices: &[InitVertex],
) -> Result<NodeTable, DatasetError> {
    let mut keys = BTreeMap::<String, ValueKind>::new();
    for row in rows {
        for (key, value) in &vertices[*row].properties {
            keys.entry(key.clone())
                .or_insert_with(|| ValueKind::of(value));
        }
    }
    let columns = keys
        .into_iter()
        .map(|(key, kind)| {
            let values = rows
                .iter()
                .map(|row| vertices[*row].properties.get(&key))
                .collect::<Vec<_>>();
            value_column(&key, kind, &values)
        })
        .collect::<Result<Vec<_>, _>>()?;
    record_node_table(label, columns)
}

fn initialized_edge_table(
    rel_type: &str,
    src_label: &str,
    dst_label: &str,
    rows: &[&InitEdge],
    aliases: &HashMap<String, usize>,
    vertex_refs: &[(String, i64)],
) -> Result<EdgeTable, DatasetError> {
    let src = rows
        .iter()
        .map(|edge| {
            aliases
                .get(&edge.from)
                .map(|idx| vertex_refs[*idx].1)
                .ok_or_else(|| DatasetError(format!("unknown from() alias `{}`", edge.from)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let dst = rows
        .iter()
        .map(|edge| {
            aliases
                .get(&edge.to)
                .map(|idx| vertex_refs[*idx].1)
                .ok_or_else(|| DatasetError(format!("unknown to() alias `{}`", edge.to)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut keys = BTreeMap::<String, ValueKind>::new();
    for edge in rows {
        for (key, value) in &edge.properties {
            keys.entry(key.clone())
                .or_insert_with(|| ValueKind::of(value));
        }
    }
    let columns = keys
        .into_iter()
        .map(|(key, kind)| {
            let values = rows
                .iter()
                .map(|edge| edge.properties.get(&key))
                .collect::<Vec<_>>();
            value_column(&key, kind, &values)
        })
        .collect::<Result<Vec<_>, _>>()?;

    record_edge_table(rel_type, src_label, dst_label, src, dst, columns)
}

#[derive(Debug, Clone, Copy)]
enum ValueKind {
    Bool,
    Int,
    Float,
    String,
    DateTime,
    Value,
}

impl ValueKind {
    fn of(value: &Value) -> Self {
        match value {
            Value::Bool(_) => Self::Bool,
            Value::Int(_) => Self::Int,
            Value::Float(_) => Self::Float,
            Value::DateTime(_) => Self::DateTime,
            Value::String(_) => Self::String,
            // Preserve Gremlin-specific typed initializer values through the
            // Arrow-backed catalog. Without this, Long/List/Set/Map properties
            // are read back as plain ints/nulls/strings before the planner can
            // apply Gremlin type semantics.
            Value::Byte(_)
            | Value::UInt8(_)
            | Value::Short(_)
            | Value::UInt16(_)
            | Value::Long(_)
            | Value::UInt32(_)
            | Value::UInt64(_)
            | Value::Float32(_)
            | Value::BigInt(_)
            | Value::UInt128(_)
            | Value::BigDecimal(_)
            | Value::InternalId { .. }
            | Value::Node { .. }
            | Value::Edge { .. }
            | Value::List(_)
            | Value::Map(_)
            | Value::Path(_)
            | Value::Null => Self::Value,
        }
    }
}

fn value_column(
    name: &str,
    kind: ValueKind,
    values: &[Option<&Value>],
) -> Result<(Field, ArrayRef), DatasetError> {
    match kind {
        ValueKind::Bool => Ok((
            Field::new(name, DataType::Boolean, true),
            Arc::new(BooleanArray::from(
                values
                    .iter()
                    .map(|value| match value {
                        Some(Value::Bool(v)) => Some(*v),
                        Some(other) => other.as_bool(),
                        None => None,
                    })
                    .collect::<Vec<_>>(),
            )) as ArrayRef,
        )),
        ValueKind::Int => Ok((
            Field::new(name, DataType::Int64, true),
            Arc::new(Int64Array::from(
                values
                    .iter()
                    .map(|value| value.and_then(Value::as_i64))
                    .collect::<Vec<_>>(),
            )) as ArrayRef,
        )),
        ValueKind::Float => Ok((
            Field::new(name, DataType::Float64, true),
            Arc::new(Float64Array::from(
                values
                    .iter()
                    .map(|value| match value {
                        Some(Value::Float(v)) => Some(*v),
                        Some(Value::Float32(v)) => Some(*v as f64),
                        Some(other) => other.as_i64().map(|v| v as f64),
                        None => None,
                    })
                    .collect::<Vec<_>>(),
            )) as ArrayRef,
        )),
        ValueKind::String | ValueKind::DateTime | ValueKind::Value => {
            let mut field = Field::new(name, DataType::Utf8, true);
            let value_type = match kind {
                ValueKind::DateTime => Some("datetime"),
                ValueKind::Value => Some("value"),
                _ => None,
            };
            if let Some(value_type) = value_type {
                field = field.with_metadata(HashMap::from([(
                    "new_graph.value_type".to_string(),
                    value_type.to_string(),
                )]));
            }
            Ok((
                field,
                Arc::new(StringArray::from(
                    values
                        .iter()
                        .map(|value| match value {
                            Some(Value::DateTime(v)) | Some(Value::String(v)) => Some(v.clone()),
                            Some(_) if matches!(kind, ValueKind::Value) => {
                                value.map(|value| format!("{value:?}"))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                )) as ArrayRef,
            ))
        }
    }
}

fn record_node_table(
    label: &str,
    columns: Vec<(Field, ArrayRef)>,
) -> Result<NodeTable, DatasetError> {
    let fields = columns
        .iter()
        .map(|(field, _)| field.clone())
        .collect::<Vec<_>>();
    let arrays = columns
        .into_iter()
        .map(|(_, array)| array)
        .collect::<Vec<_>>();
    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let batch =
        RecordBatch::try_new(schema, arrays).map_err(|err| DatasetError(err.to_string()))?;
    Ok(NodeTable {
        label: label.to_string(),
        batch,
    })
}

fn record_edge_table(
    rel_type: &str,
    src_label: &str,
    dst_label: &str,
    src: Vec<i64>,
    dst: Vec<i64>,
    columns: Vec<(Field, ArrayRef)>,
) -> Result<EdgeTable, DatasetError> {
    let mut fields = vec![
        Field::new("__src_id", DataType::Int64, false),
        Field::new("__dst_id", DataType::Int64, false),
    ];
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(src)),
        Arc::new(Int64Array::from(dst)),
    ];
    for (field, array) in columns {
        fields.push(field);
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let batch =
        RecordBatch::try_new(schema, arrays).map_err(|err| DatasetError(err.to_string()))?;
    Ok(EdgeTable {
        rel_type: rel_type.to_string(),
        src_label: src_label.to_string(),
        dst_label: dst_label.to_string(),
        batch,
    })
}

/// Standard TinkerPop "modern" graph.
///
/// Vertex layout (per-label row id):
/// ```text
///   Person    row 0  marko    age 29
///   Person    row 1  vadas    age 27
///   Person    row 2  josh     age 32
///   Person    row 3  peter    age 35
///   Software  row 0  lop      lang java
///   Software  row 1  ripple   lang java
/// ```
///
/// Edge layout (per-relation row id):
/// ```text
///   KNOWS    0  marko -> vadas   weight 0.5
///   KNOWS    1  marko -> josh    weight 1.0
///   CREATED  0  marko -> lop     weight 0.4
///   CREATED  1  josh  -> ripple  weight 1.0
///   CREATED  2  josh  -> lop     weight 0.4
///   CREATED  3  peter -> lop     weight 0.2
/// ```
pub fn modern_graph() -> PropertyGraph {
    let mut graph = PropertyGraph::new();

    let person = nodes_from_columns(
        "person",
        vec![
            (
                "id",
                Arc::new(Int64Array::from(vec![1, 2, 4, 6])) as ArrayRef,
            ),
            (
                "name",
                Arc::new(StringArray::from(vec!["marko", "vadas", "josh", "peter"])) as ArrayRef,
            ),
            (
                "age",
                Arc::new(Int64Array::from(vec![29, 27, 32, 35])) as ArrayRef,
            ),
        ],
    );
    graph.add_nodes(person);

    let software = nodes_from_columns(
        "software",
        vec![
            ("id", Arc::new(Int64Array::from(vec![3, 5])) as ArrayRef),
            (
                "name",
                Arc::new(StringArray::from(vec!["lop", "ripple"])) as ArrayRef,
            ),
            (
                "lang",
                Arc::new(StringArray::from(vec!["java", "java"])) as ArrayRef,
            ),
        ],
    );
    graph.add_nodes(software);

    let knows = edges_from_columns(
        "knows",
        "person",
        "person",
        vec![0, 0],
        vec![1, 2],
        vec![
            ("id", Arc::new(Int64Array::from(vec![7, 8])) as ArrayRef),
            (
                "weight",
                Arc::new(Float64Array::from(vec![0.5, 1.0])) as ArrayRef,
            ),
        ],
    );
    graph.add_edges(knows).expect("KNOWS edges");

    let created = edges_from_columns(
        "created",
        "person",
        "software",
        vec![0, 2, 2, 3],
        vec![0, 1, 0, 0],
        vec![
            (
                "id",
                Arc::new(Int64Array::from(vec![9, 10, 11, 12])) as ArrayRef,
            ),
            (
                "weight",
                Arc::new(Float64Array::from(vec![0.4, 1.0, 0.4, 0.2])) as ArrayRef,
            ),
        ],
    );
    graph.add_edges(created).expect("CREATED edges");

    graph
}

/// `empty` is exactly that — no vertices, no edges. The IR catalog
/// simply has no tables registered.
pub fn empty_graph() -> PropertyGraph {
    PropertyGraph::new()
}

/// TinkerPop "crew" graph — focused on multi-property vertices and
/// metadata. Our IR catalog doesn't model multi-properties or
/// `properties()` traversers, so we ship a minimal representation:
/// labels and primary single-valued properties only. Cases that lean on
/// crew-specific metadata will surface as Mismatch / RunError.
pub fn crew_graph() -> PropertyGraph {
    let mut graph = PropertyGraph::new();

    // Persons: marko, stephen, matthias, daniel.
    let crew_locations = vec![
        Value::List(vec![
            Value::String("san diego".into()),
            Value::String("santa cruz".into()),
            Value::String("brussels".into()),
            Value::String("santa fe".into()),
        ]),
        Value::List(vec![
            Value::String("centreville".into()),
            Value::String("dulles".into()),
            Value::String("purcellville".into()),
        ]),
        Value::List(vec![
            Value::String("bremen".into()),
            Value::String("baltimore".into()),
            Value::String("oakland".into()),
            Value::String("seattle".into()),
        ]),
        Value::List(vec![
            Value::String("spremberg".into()),
            Value::String("kaiserslautern".into()),
            Value::String("aachen".into()),
        ]),
    ];
    let location_refs = crew_locations.iter().map(Some).collect::<Vec<_>>();
    let person = record_node_table(
        "person",
        vec![
            (
                Field::new("name", DataType::Utf8, true),
                Arc::new(StringArray::from(vec![
                    "marko", "stephen", "matthias", "daniel",
                ])) as ArrayRef,
            ),
            value_column("location", ValueKind::Value, &location_refs)
                .expect("crew location column"),
        ],
    )
    .expect("crew person table");
    graph.add_nodes(person);

    let software = nodes_from_columns(
        "software",
        vec![(
            "name",
            Arc::new(StringArray::from(vec!["gremlin", "tinkergraph"])) as ArrayRef,
        )],
    );
    graph.add_nodes(software);

    let develops = edges_from_columns(
        "develops",
        "person",
        "software",
        vec![0, 0, 1, 2],
        vec![0, 1, 0, 0],
        vec![(
            "since",
            Arc::new(Int64Array::from(vec![2009, 2010, 2010, 2012])) as ArrayRef,
        )],
    );
    graph.add_edges(develops).expect("develops edges");

    let uses = edges_from_columns(
        "uses",
        "person",
        "software",
        vec![0, 0, 1, 1, 2, 2, 3, 3],
        vec![0, 1, 0, 1, 0, 1, 0, 1],
        vec![(
            "skill",
            Arc::new(Int64Array::from(vec![5, 4, 5, 4, 3, 3, 5, 4])) as ArrayRef,
        )],
    );
    graph.add_edges(uses).expect("uses edges");

    let traverses = edges_from_columns(
        "traverses",
        "software",
        "software",
        vec![0],
        vec![1],
        Vec::<(&str, ArrayRef)>::new(),
    );
    graph.add_edges(traverses).expect("traverses edges");

    graph
}
