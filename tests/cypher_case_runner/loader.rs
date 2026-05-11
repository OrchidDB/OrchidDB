//! Schema-driven Ladybug dataset loader.
//!
//! Each Ladybug fixture under `tests/data/ladybug/dataset/<dir>` ships
//! with two control files:
//!
//! - `schema.cypher` — a sequence of `create node table` /
//!   `create rel table` statements describing labels, columns, and
//!   primary keys.
//! - `copy.cypher`   — `COPY <table> FROM "<file>";` lines wiring CSV
//!   files (with optional `(header = true)` and similar options) to
//!   their destination tables.
//!
//! This module parses both, then loads the referenced CSVs into an
//! in-memory `PropertyGraph`. The interpreter only models scalar
//! Arrow types (`Int64`, `Float64`, `Bool`, `String`); anything richer
//! in the schema (struct, union, list, map, interval, uuid, bytea, …)
//! is collapsed to the raw CSV string so projection still surfaces
//! the literal text the cases expect.
//!
//! The loader is intentionally lenient: malformed rows / unresolvable
//! edge endpoints are dropped rather than failing the whole load,
//! since the case files can still exercise interesting query shapes
//! against partially-loaded data.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int64Array, StringArray};

use new_graph::ir::catalog::{
    EdgeTable, NodeTable, PropertyGraph, edges_from_columns, nodes_from_columns,
};

use super::dataset::DatasetError;

const LADYBUG_ROOT: &str = "tests/data/ladybug/dataset";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnType {
    Int64,
    Float64,
    Bool,
    String,
}

#[derive(Debug, Clone)]
struct Column {
    name: String,
    ty: ColumnType,
}

#[derive(Debug, Clone)]
struct NodeDef {
    label: String,
    columns: Vec<Column>,
    pk_index: usize,
}

#[derive(Debug, Clone)]
struct EdgeDef {
    rel_type: String,
    src_label: String,
    dst_label: String,
    properties: Vec<Column>,
}

#[derive(Debug)]
struct Schema {
    nodes: HashMap<String, NodeDef>,
    edges: HashMap<String, EdgeDef>,
}

#[derive(Debug, Clone)]
struct CopyEntry {
    table: String,
    file: String,
    /// `true` when the CSV's first row is the header. Inferred from the
    /// `(header = true)` option in `COPY` statements; otherwise we
    /// derive it heuristically when loading the file.
    has_header_hint: Option<bool>,
}

pub fn build(directory_name: &str) -> Result<PropertyGraph, DatasetError> {
    let root = PathBuf::from(LADYBUG_ROOT).join(directory_name);
    if !root.is_dir() {
        return Err(DatasetError(format!(
            "no fixture directory at `{}`",
            root.display()
        )));
    }
    let schema_text = fs::read_to_string(root.join("schema.cypher"))
        .map_err(|err| DatasetError(format!("read schema.cypher: {err}")))?;
    let copy_text = fs::read_to_string(root.join("copy.cypher")).unwrap_or_default();
    let schema = parse_schema(&schema_text);
    let copies = parse_copies(&copy_text);
    build_property_graph(&schema, &copies, &root)
}

// ============================================================
// Schema parsing
// ============================================================

fn parse_schema(text: &str) -> Schema {
    let mut nodes: HashMap<String, NodeDef> = HashMap::new();
    let mut edges: HashMap<String, EdgeDef> = HashMap::new();
    for stmt in split_statements(text) {
        let lower = stmt.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("create node table") {
            let original_rest = &stmt[(stmt.len() - rest.len())..];
            if let Some((label, body)) = split_table_signature(original_rest) {
                if let Some(def) = parse_node_body(&label, &body) {
                    nodes.insert(label.to_ascii_lowercase(), def);
                }
            }
        } else if let Some(rest) = lower.strip_prefix("create rel table group") {
            let original_rest = &stmt[(stmt.len() - rest.len())..];
            // REL TABLE GROUP exposes one rel_type spanning multiple
            // FROM/TO pairs. The harness models it as separate entries
            // sharing the rel_type name.
            if let Some((label, body)) = split_table_signature(original_rest) {
                for def in parse_rel_group_body(&label, &body) {
                    edges.insert(
                        format!("{}@{}->{}", def.rel_type, def.src_label, def.dst_label),
                        def,
                    );
                }
            }
        } else if let Some(rest) = lower.strip_prefix("create rel table") {
            let original_rest = &stmt[(stmt.len() - rest.len())..];
            if let Some((label, body)) = split_table_signature(original_rest) {
                if let Some(def) = parse_rel_body(&label, &body) {
                    edges.insert(label.to_ascii_lowercase(), def);
                }
            }
        }
    }
    Schema { nodes, edges }
}

/// Split a statement after the `create <kind> table` keyword: returns
/// the table name plus the parenthesized body. Backtick-quoted names
/// are unwrapped.
fn split_table_signature(rest: &str) -> Option<(String, String)> {
    let trimmed = rest.trim_start();
    let (label, after) = parse_identifier(trimmed)?;
    let after = after.trim_start();
    let body = after.strip_prefix('(')?;
    // `body` runs to the matching `)` — ignore anything past it.
    let inner = balanced_inner(body)?;
    Some((label, inner))
}

fn parse_identifier(text: &str) -> Option<(String, &str)> {
    let trimmed = text.trim_start();
    if let Some(stripped) = trimmed.strip_prefix('`') {
        let end = stripped.find('`')?;
        let label = &stripped[..end];
        Some((label.to_string(), &stripped[end + 1..]))
    } else {
        let end = trimmed
            .find(|c: char| c.is_whitespace() || c == '(' || c == ',' || c == ';')
            .unwrap_or(trimmed.len());
        if end == 0 {
            return None;
        }
        let label = &trimmed[..end];
        Some((label.to_string(), &trimmed[end..]))
    }
}

/// Returns the substring before the matching `)`, treating quoted
/// strings (`'...'`, `"..."`) as opaque.
fn balanced_inner(body: &str) -> Option<String> {
    let mut depth = 1;
    let mut out = String::new();
    let mut chars = body.chars();
    let mut in_single = false;
    let mut in_double = false;
    while let Some(ch) = chars.next() {
        if in_single {
            out.push(ch);
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            out.push(ch);
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                out.push(ch);
            }
            '"' => {
                in_double = true;
                out.push(ch);
            }
            '(' => {
                depth += 1;
                out.push(ch);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(out);
                }
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    None
}

fn parse_node_body(label: &str, body: &str) -> Option<NodeDef> {
    let parts = split_top_level_commas(body);
    let mut columns = Vec::new();
    let mut pk_name: Option<String> = None;
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("primary key") {
            let original_rest = &trimmed[(trimmed.len() - rest.len())..];
            if let Some(inner) = original_rest.trim().strip_prefix('(') {
                if let Some(name) = balanced_inner(inner) {
                    pk_name = Some(name.trim().trim_matches('`').to_string());
                }
            }
            continue;
        }
        if let Some((name, ty)) = parse_column(trimmed) {
            columns.push(Column { name, ty });
        }
    }
    let pk_index = pk_name
        .as_deref()
        .and_then(|name| {
            columns
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(name))
        })
        .unwrap_or(0);
    if columns.is_empty() {
        return None;
    }
    Some(NodeDef {
        label: label.to_string(),
        columns,
        pk_index,
    })
}

fn parse_rel_body(label: &str, body: &str) -> Option<EdgeDef> {
    let parts = split_top_level_commas(body);
    if parts.is_empty() {
        return None;
    }
    let head = parts[0].trim();
    let (src_label, dst_label) = parse_from_to(head)?;
    let mut properties = Vec::new();
    for part in parts.into_iter().skip(1) {
        let trimmed = part.trim();
        if trimmed.is_empty() || is_multiplicity_keyword(trimmed) {
            continue;
        }
        if let Some((name, ty)) = parse_column(trimmed) {
            properties.push(Column { name, ty });
        }
    }
    Some(EdgeDef {
        rel_type: label.to_string(),
        src_label,
        dst_label,
        properties,
    })
}

fn parse_rel_group_body(label: &str, body: &str) -> Vec<EdgeDef> {
    let parts = split_top_level_commas(body);
    let mut from_to_pairs = Vec::new();
    let mut properties: Vec<Column> = Vec::new();
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() || is_multiplicity_keyword(trimmed) {
            continue;
        }
        if let Some(pair) = parse_from_to(trimmed) {
            from_to_pairs.push(pair);
        } else if let Some((name, ty)) = parse_column(trimmed) {
            properties.push(Column { name, ty });
        }
    }
    from_to_pairs
        .into_iter()
        .map(|(src, dst)| EdgeDef {
            rel_type: label.to_string(),
            src_label: src,
            dst_label: dst,
            properties: properties.clone(),
        })
        .collect()
}

fn parse_from_to(text: &str) -> Option<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let from_idx = lower.find("from")?;
    let after_from = &text[from_idx + 4..];
    let (src, after_src) = parse_identifier(after_from)?;
    let after_lower = after_src.to_ascii_lowercase();
    let to_idx = after_lower.find("to")?;
    let after_to = &after_src[to_idx + 2..];
    let (dst, _) = parse_identifier(after_to)?;
    Some((src, dst))
}

fn is_multiplicity_keyword(text: &str) -> bool {
    matches!(
        text.to_ascii_uppercase().as_str(),
        "MANY_MANY" | "MANY_ONE" | "ONE_MANY" | "ONE_ONE"
    )
}

fn parse_column(text: &str) -> Option<(String, ColumnType)> {
    let trimmed = text.trim();
    let (name_token, after) = parse_identifier(trimmed)?;
    let type_text = after.trim();
    if type_text.is_empty() {
        return None;
    }
    let ty = classify_type(type_text);
    Some((name_token, ty))
}

fn classify_type(text: &str) -> ColumnType {
    let trimmed = text.trim();
    let upper = trimmed.to_ascii_uppercase();
    // Anything compound (struct / union / map / list / fixed-width)
    // collapses to string. List markers can appear after the base
    // type (`INT64[]`, `INT64[4]`, `INT64[][]`).
    if upper.contains('[')
        || upper.starts_with("STRUCT")
        || upper.starts_with("UNION")
        || upper.starts_with("MAP")
    {
        return ColumnType::String;
    }
    let head = upper
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("");
    match head {
        "INT" | "INT8" | "INT16" | "INT32" | "INT64" | "INT128" | "BIGINT" | "SERIAL" | "UINT8"
        | "UINT16" | "UINT32" | "UINT64" | "BYTE" | "SHORT" | "LONG" => ColumnType::Int64,
        "FLOAT" | "FLOAT32" | "FLOAT64" | "DOUBLE" | "DECIMAL" => ColumnType::Float64,
        "BOOL" | "BOOLEAN" => ColumnType::Bool,
        _ => ColumnType::String,
    }
}

/// Split a body on top-level commas, respecting balanced `()`/`[]` and
/// quoted strings.
fn split_top_level_commas(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            ',' if depth_paren == 0 && depth_bracket == 0 => {
                out.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&body[start..]);
    out
}

fn split_statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    for ch in text.chars() {
        if in_single {
            buf.push(ch);
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            buf.push(ch);
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        if in_backtick {
            buf.push(ch);
            if ch == '`' {
                in_backtick = false;
            }
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                buf.push(ch);
            }
            '"' => {
                in_double = true;
                buf.push(ch);
            }
            '`' => {
                in_backtick = true;
                buf.push(ch);
            }
            ';' => {
                let trimmed = buf.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(trimmed);
                }
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }
    let trimmed = buf.trim().to_string();
    if !trimmed.is_empty() {
        out.push(trimmed);
    }
    out
}

// ============================================================
// COPY parsing
// ============================================================

fn parse_copies(text: &str) -> Vec<CopyEntry> {
    let mut out = Vec::new();
    for stmt in split_statements(text) {
        let upper = stmt.to_ascii_uppercase();
        if !upper.starts_with("COPY") {
            continue;
        }
        let body = &stmt[4..];
        let (table, after) = match parse_identifier(body.trim_start()) {
            Some(parsed) => parsed,
            None => continue,
        };
        let after = after.trim_start();
        let after_upper = after.to_ascii_uppercase();
        let from_idx = match after_upper.find("FROM") {
            Some(idx) => idx,
            None => continue,
        };
        let after_from = &after[from_idx + 4..].trim_start();
        let file = match extract_quoted(after_from) {
            Some(s) => s,
            None => continue,
        };
        let after_file = match strip_first_quoted(after_from) {
            Some(s) => s,
            None => continue,
        };
        let mut has_header_hint = None;
        if let Some(open) = after_file.find('(') {
            let inside = &after_file[open + 1..];
            if let Some(close) = inside.rfind(')') {
                let options = &inside[..close];
                if options.to_ascii_lowercase().contains("header") {
                    let lower = options.to_ascii_lowercase();
                    if lower.contains("header=true") || lower.contains("header = true") {
                        has_header_hint = Some(true);
                    } else if lower.contains("header=false") || lower.contains("header = false") {
                        has_header_hint = Some(false);
                    }
                }
            }
        }
        out.push(CopyEntry {
            table,
            file,
            has_header_hint,
        });
    }
    out
}

fn extract_quoted(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let stripped = trimmed.strip_prefix('"')?;
    let end = stripped.find('"')?;
    Some(stripped[..end].to_string())
}

fn strip_first_quoted(text: &str) -> Option<&str> {
    let trimmed = text.trim_start();
    let stripped = trimmed.strip_prefix('"')?;
    let end = stripped.find('"')?;
    Some(&stripped[end + 1..])
}

// ============================================================
// Property graph assembly
// ============================================================

fn build_property_graph(
    schema: &Schema,
    copies: &[CopyEntry],
    root: &Path,
) -> Result<PropertyGraph, DatasetError> {
    let mut graph = PropertyGraph::new();
    let mut pk_index: HashMap<(String, String), i64> = HashMap::new();

    // Group COPY entries by table name (case-insensitive).
    let mut by_table: HashMap<String, Vec<&CopyEntry>> = HashMap::new();
    for entry in copies {
        by_table
            .entry(entry.table.to_ascii_lowercase())
            .or_default()
            .push(entry);
    }

    // Stable order: nodes first, edges second, alphabetically within
    // each. Edges depend on node row indices, so this ordering matters.
    let mut node_keys: Vec<&String> = schema.nodes.keys().collect();
    node_keys.sort();
    for key in node_keys {
        let def = &schema.nodes[key];
        let entries = by_table.get(key).cloned().unwrap_or_default();
        match build_node_table(root, def, &entries, &mut pk_index) {
            Ok(table) => graph.add_nodes(table),
            Err(_) => continue,
        }
    }

    let mut edge_keys: Vec<&String> = schema.edges.keys().collect();
    edge_keys.sort();
    for key in edge_keys {
        let def = &schema.edges[key];
        let entries = by_table
            .get(&def.rel_type.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default();
        match build_edge_table(root, def, &entries, &pk_index) {
            Ok(table) => {
                let _ = graph.add_edges(table);
            }
            Err(_) => continue,
        }
    }

    Ok(graph)
}

fn build_node_table(
    root: &Path,
    def: &NodeDef,
    entries: &[&CopyEntry],
    pk_index: &mut HashMap<(String, String), i64>,
) -> Result<NodeTable, DatasetError> {
    let mut columns: Vec<Vec<Option<String>>> = vec![Vec::new(); def.columns.len()];
    for entry in entries {
        let path = root.join(&entry.file);
        let raw = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let mut rows = parse_csv(&raw);
        let has_header = entry
            .has_header_hint
            .unwrap_or_else(|| infer_header(&rows, def));
        if has_header && !rows.is_empty() {
            rows.remove(0);
        }
        for row in rows {
            for idx in 0..def.columns.len() {
                let cell = row.get(idx).cloned().flatten();
                columns[idx].push(cell);
            }
        }
    }

    for (row_index, value) in columns[def.pk_index].iter().enumerate() {
        if let Some(text) = value {
            pk_index.insert(
                (def.label.to_ascii_lowercase(), text.clone()),
                row_index as i64,
            );
        }
    }

    let arrays: Vec<(&str, ArrayRef)> = def
        .columns
        .iter()
        .enumerate()
        .map(|(idx, column)| {
            let array = build_array(&columns[idx], column.ty);
            (column.name.as_str(), array)
        })
        .collect();
    Ok(nodes_from_columns(def.label.clone(), arrays))
}

fn build_edge_table(
    root: &Path,
    def: &EdgeDef,
    entries: &[&CopyEntry],
    pk_index: &HashMap<(String, String), i64>,
) -> Result<EdgeTable, DatasetError> {
    let mut src_ids: Vec<i64> = Vec::new();
    let mut dst_ids: Vec<i64> = Vec::new();
    let mut prop_columns: Vec<Vec<Option<String>>> = vec![Vec::new(); def.properties.len()];

    for entry in entries {
        let path = root.join(&entry.file);
        let raw = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let mut rows = parse_csv(&raw);
        let has_header = entry
            .has_header_hint
            .unwrap_or_else(|| infer_edge_header(&rows));
        if has_header && !rows.is_empty() {
            rows.remove(0);
        }
        for row in rows {
            let from_text = match row.first().and_then(|c| c.clone()) {
                Some(text) => text,
                None => continue,
            };
            let to_text = match row.get(1).and_then(|c| c.clone()) {
                Some(text) => text,
                None => continue,
            };
            let Some(src) = pk_index
                .get(&(def.src_label.to_ascii_lowercase(), from_text))
                .copied()
            else {
                continue;
            };
            let Some(dst) = pk_index
                .get(&(def.dst_label.to_ascii_lowercase(), to_text))
                .copied()
            else {
                continue;
            };
            src_ids.push(src);
            dst_ids.push(dst);
            for (prop_idx, _) in def.properties.iter().enumerate() {
                let cell = row.get(2 + prop_idx).cloned().flatten();
                prop_columns[prop_idx].push(cell);
            }
        }
    }

    let arrays: Vec<(&str, ArrayRef)> = def
        .properties
        .iter()
        .enumerate()
        .map(|(idx, column)| {
            let array = build_array(&prop_columns[idx], column.ty);
            (column.name.as_str(), array)
        })
        .collect();
    Ok(edges_from_columns(
        def.rel_type.clone(),
        def.src_label.clone(),
        def.dst_label.clone(),
        src_ids,
        dst_ids,
        arrays,
    ))
}

/// Header inference for node CSVs. We compare the first row against
/// the schema's known column names: if the first cell matches the
/// primary-key column name (case-insensitively), it's a header. As a
/// fallback for INT primary keys, attempt to parse the first cell as
/// `i64`; failure indicates a header.
fn infer_header(rows: &[Vec<Option<String>>], def: &NodeDef) -> bool {
    let Some(first) = rows.first() else {
        return false;
    };
    let pk_name = &def.columns[def.pk_index].name;
    if let Some(cell) = first.first().and_then(|c| c.as_deref()) {
        if cell.eq_ignore_ascii_case(pk_name) {
            return true;
        }
        if matches!(def.columns[def.pk_index].ty, ColumnType::Int64)
            && cell.trim().parse::<i64>().is_err()
        {
            return true;
        }
    }
    false
}

/// Header inference for edge CSVs: the first two columns are FROM/TO
/// references. We treat the row as a header iff the first cell isn't
/// parseable as an integer (FROM is virtually always a numeric PK in
/// the corpus) and matches a small set of header tokens.
fn infer_edge_header(rows: &[Vec<Option<String>>]) -> bool {
    let Some(first) = rows.first() else {
        return false;
    };
    if let Some(cell) = first.first().and_then(|c| c.as_deref()) {
        let lower = cell.trim().to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "from" | "src" | "source" | "_id" | "__src_id"
        ) {
            return true;
        }
        if cell.trim().parse::<i64>().is_err() && !cell.trim().is_empty() {
            return true;
        }
    }
    false
}

fn build_array(values: &[Option<String>], ty: ColumnType) -> ArrayRef {
    match ty {
        ColumnType::Int64 => {
            let parsed: Vec<Option<i64>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(parse_i64))
                .collect();
            Arc::new(Int64Array::from(parsed))
        }
        ColumnType::Float64 => {
            let parsed: Vec<Option<f64>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(|s| s.trim().parse::<f64>().ok()))
                .collect();
            Arc::new(Float64Array::from(parsed))
        }
        ColumnType::Bool => {
            let parsed: Vec<Option<bool>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(parse_bool))
                .collect();
            Arc::new(BooleanArray::from(parsed))
        }
        ColumnType::String => {
            let parsed: Vec<Option<&str>> = values.iter().map(|cell| cell.as_deref()).collect();
            Arc::new(StringArray::from(parsed))
        }
    }
}

fn parse_i64(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<i64>().ok()
}

fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "1" => Some(true),
        "false" | "f" | "0" => Some(false),
        _ => None,
    }
}

// ============================================================
// CSV reader
// ============================================================

/// Minimal CSV reader: `,` delimiter, `"`-quoted fields, `""` for an
/// embedded quote, `\` for backslash escapes inside quoted fields.
/// Empty fields between delimiters surface as `None`; quoted empty
/// fields surface as `Some("")`.
fn parse_csv(input: &str) -> Vec<Vec<Option<String>>> {
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut row: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut field_was_quoted = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        match next {
                            'n' => field.push('\n'),
                            'r' => field.push('\r'),
                            't' => field.push('\t'),
                            '"' => field.push('"'),
                            '\\' => field.push('\\'),
                            other => {
                                field.push('\\');
                                field.push(other);
                            }
                        }
                    }
                }
                _ => field.push(ch),
            }
            continue;
        }
        match ch {
            '"' => {
                in_quotes = true;
                field_was_quoted = true;
            }
            ',' => {
                row.push(finalize_field(&mut field, &mut field_was_quoted));
            }
            '\n' => {
                row.push(finalize_field(&mut field, &mut field_was_quoted));
                rows.push(std::mem::take(&mut row));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                row.push(finalize_field(&mut field, &mut field_was_quoted));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(ch),
        }
    }

    if !field.is_empty() || !row.is_empty() || field_was_quoted {
        row.push(finalize_field(&mut field, &mut field_was_quoted));
        rows.push(row);
    }

    rows
}

fn finalize_field(field: &mut String, was_quoted: &mut bool) -> Option<String> {
    let value = std::mem::take(field);
    let quoted = std::mem::replace(was_quoted, false);
    if quoted {
        Some(value)
    } else if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
