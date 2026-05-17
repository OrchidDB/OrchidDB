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

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, SchemaRef};
use num_bigint::BigInt;

use new_graph::ir::catalog::{EdgeTable, NodeTable, PropertyGraph};
use new_graph::ir::value::{STRUCT_ORDER_KEY, Value};

use super::dataset::DatasetError;

const LADYBUG_ROOT: &str = "tests/data/ladybug/dataset";
const KUZU_MAP_ENTRIES_KEY: &str = "\u{0}kuzu_map_entries";

#[derive(Debug, Clone, PartialEq, Eq)]
enum ColumnType {
    Int64,
    /// Kuzu SERIAL columns are generated when COPY input omits the
    /// column. They are stored as Int64 once materialized.
    Serial,
    Float64,
    Bool,
    String,
    /// Calendar dates ("YYYY-MM-DD"). Stored as String columns but
    /// normalized to a zero-padded canonical form on load so output
    /// matches Kuzu's printer (which always emits `1950-07-23` rather
    /// than the raw CSV `1950-7-23`).
    Date,
    /// Timestamp values (`YYYY-MM-DD HH:MM:SS[.fff][Z|±HH:MM]`). Stored
    /// as String columns; the loader strips timezone offsets after
    /// shifting to UTC so we print Kuzu's naive form on output.
    Timestamp,
    /// UUID values. Lowercased and hyphenated so Kuzu's canonical
    /// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` form matches the case
    /// expectations regardless of the CSV's varied input shape
    /// (`{...}`, all-caps, hyphen-less).
    Uuid,
    /// Calendar-aware interval values. Stored as String; the loader
    /// normalises the verbose `H hours M minutes S seconds U us` tail
    /// into Kuzu's compact `HH:MM:SS[.uuuuuu]` representation.
    Interval,
    /// Values whose Arrow representation is a debug-encoded `Value`
    /// plus catalog metadata, used for lists, structs/maps/unions, and
    /// integer widths wider than i64.
    Value(String),
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
    /// Order in which `CREATE NODE TABLE` declarations appeared in the
    /// schema. Kuzu numbers node tables in this order when printing
    /// node `_ID` values, so the loader has to remember it instead of
    /// falling back to alphabetic iteration over the HashMap.
    node_order: Vec<String>,
    /// Same for `CREATE REL TABLE`.
    edge_order: Vec<String>,
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
    let mut node_order: Vec<String> = Vec::new();
    let mut edge_order: Vec<String> = Vec::new();
    for stmt in split_statements(text) {
        let lower = stmt.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("create node table") {
            let original_rest = &stmt[(stmt.len() - rest.len())..];
            if let Some((label, body)) = split_table_signature(original_rest) {
                if let Some(def) = parse_node_body(&label, &body) {
                    let key = label.to_ascii_lowercase();
                    if !node_order.contains(&key) {
                        node_order.push(key.clone());
                    }
                    nodes.insert(key, def);
                }
            }
        } else if let Some(rest) = lower.strip_prefix("create rel table group") {
            let original_rest = &stmt[(stmt.len() - rest.len())..];
            // REL TABLE GROUP exposes one rel_type spanning multiple
            // FROM/TO pairs. The harness models it as separate entries
            // sharing the rel_type name.
            if let Some((label, body)) = split_table_signature(original_rest) {
                for def in parse_rel_group_body(&label, &body) {
                    let key = format!("{}@{}->{}", def.rel_type, def.src_label, def.dst_label);
                    if !edge_order.contains(&def.rel_type) {
                        edge_order.push(def.rel_type.clone());
                    }
                    edges.insert(key, def);
                }
            }
        } else if let Some(rest) = lower.strip_prefix("create rel table") {
            let original_rest = &stmt[(stmt.len() - rest.len())..];
            if let Some((label, body)) = split_table_signature(original_rest) {
                if let Some(def) = parse_rel_body(&label, &body) {
                    let key = label.to_ascii_lowercase();
                    if !edge_order.contains(&key) {
                        edge_order.push(key.clone());
                    }
                    edges.insert(key, def);
                }
            }
        }
    }
    Schema {
        nodes,
        edges,
        node_order,
        edge_order,
    }
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
    // Compound values and integer widths wider than i64 ride through
    // the catalog as debug-encoded `Value`s so the interpreter sees
    // the same list/struct/numeric shape that Kuzu would.
    if upper.contains('[')
        || upper.starts_with("STRUCT")
        || upper.starts_with("MAP")
        || upper.starts_with("UNION")
        || upper.starts_with("INT128")
        || upper.starts_with("UINT64")
        || upper.starts_with("UINT128")
    {
        return ColumnType::Value(trimmed.to_string());
    }
    let head = upper
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("");
    match head {
        "SERIAL" => ColumnType::Serial,
        "INT" | "INT8" | "INT16" | "INT32" | "INT64" | "INT128" | "BIGINT" | "UINT8" | "UINT16"
        | "UINT32" | "UINT64" | "BYTE" | "SHORT" | "LONG" => ColumnType::Int64,
        "FLOAT" | "FLOAT32" | "FLOAT64" | "DOUBLE" | "DECIMAL" => ColumnType::Float64,
        "BOOL" | "BOOLEAN" => ColumnType::Bool,
        "DATE" => ColumnType::Date,
        "TIMESTAMP" | "DATETIME" | "TIMESTAMP_NS" | "TIMESTAMP_MS" | "TIMESTAMP_SEC"
        | "TIMESTAMP_TZ" => ColumnType::Timestamp,
        "UUID" => ColumnType::Uuid,
        "INTERVAL" => ColumnType::Interval,
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
    let mut depth = 0i32;
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
            '(' => {
                depth += 1;
                buf.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                buf.push(ch);
            }
            ';' => {
                let trimmed = buf.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(trimmed);
                }
                buf.clear();
            }
            '\n' if depth == 0 => {
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

    // Stable order: nodes first, edges second, in the order that each
    // `CREATE NODE/REL TABLE` declaration appeared in `schema.cypher`.
    // Kuzu's printed `_ID` values encode that index, so this is the only
    // ordering that lines up with the conformance fixtures.
    let mut visited_nodes = std::collections::HashSet::new();
    for key in &schema.node_order {
        if !schema.nodes.contains_key(key) {
            continue;
        }
        visited_nodes.insert(key.clone());
        let def = &schema.nodes[key];
        let entries = by_table.get(key).cloned().unwrap_or_default();
        match build_node_table(root, def, &entries, &mut pk_index) {
            Ok(table) => graph.add_nodes(table),
            Err(_) => continue,
        }
    }
    // Fallback: any node declared outside the recorded order (e.g.,
    // alternate schema flow) loads alphabetically so we don't drop it.
    let mut remaining: Vec<&String> = schema
        .nodes
        .keys()
        .filter(|k| !visited_nodes.contains(*k))
        .collect();
    remaining.sort();
    for key in remaining {
        let def = &schema.nodes[key];
        let entries = by_table.get(key).cloned().unwrap_or_default();
        match build_node_table(root, def, &entries, &mut pk_index) {
            Ok(table) => graph.add_nodes(table),
            Err(_) => continue,
        }
    }

    let mut edge_keys: Vec<&String> = schema
        .edge_order
        .iter()
        .flat_map(|order_key| {
            schema.edges.keys().filter(move |actual_key| {
                actual_key == &order_key || actual_key.starts_with(&format!("{order_key}@"))
            })
        })
        .collect();
    let mut seen_keys: std::collections::HashSet<&String> = edge_keys.iter().copied().collect();
    let mut fallback: Vec<&String> = schema
        .edges
        .keys()
        .filter(|k| !seen_keys.contains(k))
        .collect();
    fallback.sort();
    for fk in &fallback {
        if seen_keys.insert(*fk) {
            edge_keys.push(*fk);
        }
    }
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
            let omitted_serial_pk = matches!(def.columns[def.pk_index].ty, ColumnType::Serial)
                && row.len() + 1 == def.columns.len();
            let generated_serial =
                omitted_serial_pk.then(|| columns[def.pk_index].len().to_string());
            for idx in 0..def.columns.len() {
                let cell = if omitted_serial_pk {
                    if idx == def.pk_index {
                        generated_serial.clone()
                    } else {
                        let source_idx = if idx < def.pk_index { idx } else { idx - 1 };
                        row.get(source_idx).cloned().flatten()
                    }
                } else {
                    row.get(idx).cloned().flatten()
                };
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

    let columns: Vec<(Field, ArrayRef)> = def
        .columns
        .iter()
        .enumerate()
        .map(|(idx, column)| build_column(&column.name, &columns[idx], &column.ty))
        .collect();
    record_node_table(&def.label, columns)
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
            .unwrap_or_else(|| infer_edge_header(&rows, def, pk_index));
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

    let columns: Vec<(Field, ArrayRef)> = def
        .properties
        .iter()
        .enumerate()
        .map(|(idx, column)| build_column(&column.name, &prop_columns[idx], &column.ty))
        .collect();
    record_edge_table(
        &def.rel_type,
        &def.src_label,
        &def.dst_label,
        src_ids,
        dst_ids,
        columns,
    )
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
        if matches!(
            &def.columns[def.pk_index].ty,
            ColumnType::Int64 | ColumnType::Serial
        ) && cell.trim().parse::<i64>().is_err()
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
fn infer_edge_header(
    rows: &[Vec<Option<String>>],
    def: &EdgeDef,
    pk_index: &HashMap<(String, String), i64>,
) -> bool {
    let Some(first) = rows.first() else {
        return false;
    };
    if let (Some(from), Some(to)) = (
        first.first().and_then(|c| c.as_deref()),
        first.get(1).and_then(|c| c.as_deref()),
    ) {
        let from_key = (def.src_label.to_ascii_lowercase(), from.to_string());
        let to_key = (def.dst_label.to_ascii_lowercase(), to.to_string());
        if pk_index.contains_key(&from_key) && pk_index.contains_key(&to_key) {
            return false;
        }
    }
    if let Some(cell) = first.first().and_then(|c| c.as_deref()) {
        let lower = cell.trim().to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "from" | "src" | "source" | "_id" | "__src_id"
        ) {
            return true;
        }
    }
    false
}

fn build_column(name: &str, values: &[Option<String>], ty: &ColumnType) -> (Field, ArrayRef) {
    match ty {
        ColumnType::Int64 | ColumnType::Serial => {
            let parsed: Vec<Option<i64>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(parse_i64))
                .collect();
            (
                Field::new(name, DataType::Int64, true),
                Arc::new(Int64Array::from(parsed)) as ArrayRef,
            )
        }
        ColumnType::Float64 => {
            let parsed: Vec<Option<f64>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(|s| s.trim().parse::<f64>().ok()))
                .collect();
            (
                Field::new(name, DataType::Float64, true),
                Arc::new(Float64Array::from(parsed)) as ArrayRef,
            )
        }
        ColumnType::Bool => {
            let parsed: Vec<Option<bool>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(parse_bool))
                .collect();
            (
                Field::new(name, DataType::Boolean, true),
                Arc::new(BooleanArray::from(parsed)) as ArrayRef,
            )
        }
        ColumnType::String => {
            let parsed: Vec<Option<&str>> = values.iter().map(|cell| cell.as_deref()).collect();
            (
                Field::new(name, DataType::Utf8, true),
                Arc::new(StringArray::from(parsed)) as ArrayRef,
            )
        }
        ColumnType::Date => {
            // Canonicalize raw `YYYY-M-D` etc. to the zero-padded
            // `YYYY-MM-DD` form Kuzu prints, so result rows can
            // compare via simple string equality.
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_date))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            (
                Field::new(name, DataType::Utf8, true),
                Arc::new(StringArray::from(refs)) as ArrayRef,
            )
        }
        ColumnType::Timestamp => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_timestamp))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            let field = Field::new(name, DataType::Utf8, true).with_metadata(HashMap::from([(
                "new_graph.value_type".to_string(),
                "datetime".to_string(),
            )]));
            (field, Arc::new(StringArray::from(refs)) as ArrayRef)
        }
        ColumnType::Uuid => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_uuid))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            (
                Field::new(name, DataType::Utf8, true),
                Arc::new(StringArray::from(refs)) as ArrayRef,
            )
        }
        ColumnType::Interval => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_interval))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            (
                Field::new(name, DataType::Utf8, true),
                Arc::new(StringArray::from(refs)) as ArrayRef,
            )
        }
        ColumnType::Value(type_text) => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| {
                    cell.as_deref()
                        .and_then(|raw| parse_typed_value(raw, type_text))
                        .map(|value| format!("{value:?}"))
                })
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            let field = Field::new(name, DataType::Utf8, true).with_metadata(HashMap::from([(
                "new_graph.value_type".to_string(),
                "value".to_string(),
            )]));
            (field, Arc::new(StringArray::from(refs)) as ArrayRef)
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
    let schema: SchemaRef = Arc::new(ArrowSchema::new(fields));
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
        Arc::new(Int64Array::from(src)) as ArrayRef,
        Arc::new(Int64Array::from(dst)) as ArrayRef,
    ];
    for (field, array) in columns {
        fields.push(field);
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(ArrowSchema::new(fields));
    let batch =
        RecordBatch::try_new(schema, arrays).map_err(|err| DatasetError(err.to_string()))?;
    Ok(EdgeTable {
        rel_type: rel_type.to_string(),
        src_label: src_label.to_string(),
        dst_label: dst_label.to_string(),
        batch,
    })
}

fn parse_typed_value(raw: &str, type_text: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = type_text.trim().to_ascii_uppercase();
    if upper.starts_with("STRUCT") {
        return parse_struct_value(trimmed, type_text);
    }
    if upper.starts_with("MAP") {
        return parse_map_value(trimmed, type_text);
    }
    if upper.starts_with("UNION") {
        return parse_union_value(trimmed, type_text);
    }
    if upper.contains('[') {
        return parse_list_value(trimmed, type_text);
    }
    parse_scalar_value(trimmed, type_text)
}

fn parse_list_value(raw: &str, type_text: &str) -> Option<Value> {
    let (base, depth) = list_type_parts(type_text)?;
    let inner = raw.trim().strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Value::List(Vec::new()));
    }
    let items = split_value_items(inner)
        .into_iter()
        .map(|item| {
            if depth > 1 {
                let nested_type = format!("{base}{}", "[]".repeat(depth - 1));
                parse_list_value(item.trim(), &nested_type)
            } else {
                parse_list_scalar_value(item.trim(), base)
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Value::List(items))
}

fn parse_list_scalar_value(raw: &str, type_text: &str) -> Option<Value> {
    if is_string_type(type_text) {
        return Some(Value::String(unescape_list_string(raw.trim())));
    }
    parse_scalar_value(raw, type_text)
}

fn unescape_list_string(raw: &str) -> String {
    let mut out = raw.to_string();
    while out.contains("\\\"") {
        out = out.replace("\\\"", "\"");
    }
    while out.contains("\\'") {
        out = out.replace("\\'", "'");
    }
    out
}

fn list_type_parts(type_text: &str) -> Option<(&str, usize)> {
    let base_end = type_text.find('[')?;
    let depth = type_text[base_end..].matches('[').count();
    Some((type_text[..base_end].trim(), depth))
}

fn parse_struct_value(raw: &str, type_text: &str) -> Option<Value> {
    let fields = struct_fields(type_text)?;
    let entries = raw_map_entries(raw)?;
    let mut out = BTreeMap::new();
    out.insert(
        STRUCT_ORDER_KEY.to_string(),
        Value::List(
            fields
                .iter()
                .map(|(name, _)| Value::String(name.clone()))
                .collect(),
        ),
    );
    for (name, field_type) in fields {
        let value = entries
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(&name))
            .and_then(|(_, value)| parse_typed_value(value, &field_type))
            .unwrap_or(Value::Null);
        out.insert(name, value);
    }
    Some(Value::Map(out))
}

fn parse_map_value(raw: &str, type_text: &str) -> Option<Value> {
    let (key_type, value_type) = map_type_parts(type_text)?;
    let entries = raw_map_entries(raw)?;
    let mut out = Vec::new();
    for (key, raw_value) in entries {
        let key = parse_list_scalar_value(&key, &key_type)?;
        let value = parse_typed_value(&raw_value, &value_type).unwrap_or(Value::Null);
        out.push(Value::List(vec![key, value]));
    }
    Some(Value::Map(BTreeMap::from([(
        KUZU_MAP_ENTRIES_KEY.to_string(),
        Value::List(out),
    )])))
}

fn parse_union_value(raw: &str, type_text: &str) -> Option<Value> {
    let variants = struct_fields(type_text)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    for (tag, ty) in &variants {
        if let Some(value) = parse_typed_value(trimmed, ty) {
            let mut out = BTreeMap::new();
            out.insert("__tag".to_string(), Value::String(tag.clone()));
            out.insert("__value".to_string(), value);
            return Some(Value::Map(out));
        }
    }
    Some(Value::String(unquote(trimmed).to_string()))
}

fn map_type_parts(type_text: &str) -> Option<(String, String)> {
    let upper = type_text.trim().to_ascii_uppercase();
    let open = upper.find("MAP")?;
    let after = &type_text[open + 3..].trim_start();
    let inner = after.strip_prefix('(').and_then(balanced_inner)?;
    let mut parts = split_value_items(&inner).into_iter();
    let key = parts.next()?.trim().to_string();
    let value = parts.next()?.trim().to_string();
    Some((key, value))
}

fn struct_fields(type_text: &str) -> Option<Vec<(String, String)>> {
    let open = type_text.find('(')?;
    let inner = balanced_inner(&type_text[open + 1..])?;
    split_value_items(&inner)
        .into_iter()
        .map(|part| {
            let (name, after) = parse_identifier(part.trim())?;
            let ty = after.trim();
            (!ty.is_empty()).then(|| (name, ty.to_string()))
        })
        .collect()
}

fn raw_map_entries(raw: &str) -> Option<Vec<(String, String)>> {
    let inner = raw.trim().strip_prefix('{')?.strip_suffix('}')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    split_value_items(inner)
        .into_iter()
        .map(|entry| {
            let (key, value) = split_top_level_assignment(&entry)?;
            Some((unquote(key.trim()).to_string(), value.trim().to_string()))
        })
        .collect()
}

fn parse_scalar_value(raw: &str, type_text: &str) -> Option<Value> {
    let upper = type_text.trim().to_ascii_uppercase();
    let head = upper
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("");
    match head {
        "INT128" | "UINT64" | "UINT128" => BigInt::from_str(raw.trim()).ok().map(Value::BigInt),
        "INT" | "INT8" | "INT16" | "INT32" | "INT64" | "BIGINT" | "SERIAL" | "UINT8" | "UINT16"
        | "UINT32" | "BYTE" | "SHORT" | "LONG" => parse_i64(raw).map(Value::Int),
        "FLOAT" | "FLOAT32" | "FLOAT64" | "DOUBLE" | "DECIMAL" => {
            raw.trim().parse::<f64>().ok().map(Value::Float)
        }
        "BOOL" | "BOOLEAN" => parse_bool(raw).map(Value::Bool),
        "DATE" => parse_date_value(raw).map(Value::DateTime),
        "TIMESTAMP" | "DATETIME" | "TIMESTAMP_NS" | "TIMESTAMP_MS" | "TIMESTAMP_SEC"
        | "TIMESTAMP_TZ" => format_timestamp_for_type(raw, head).map(Value::DateTime),
        "UUID" => Some(Value::String(normalize_uuid(raw))),
        "INTERVAL" => Some(Value::String(normalize_interval(raw))),
        _ => Some(Value::String(unquote(raw.trim()).to_string())),
    }
}

fn is_string_type(type_text: &str) -> bool {
    matches!(
        type_text
            .trim()
            .to_ascii_uppercase()
            .split(|c: char| c.is_whitespace() || c == '(')
            .next()
            .unwrap_or(""),
        "STRING" | "VARCHAR" | "CHAR" | "TEXT" | "BLOB" | "BYTEA"
    )
}

fn split_value_items(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;
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
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            ',' if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                out.push(body[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = body[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

fn split_top_level_assignment(input: &str) -> Option<(&str, &str)> {
    split_top_level_char(input, ':').or_else(|| split_top_level_char(input, '='))
}

fn split_top_level_char(input: &str, needle: char) -> Option<(&str, &str)> {
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    for (idx, ch) in input.char_indices() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            ch if ch == needle && depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                return Some((&input[..idx], &input[idx + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| text.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(text)
}

fn format_timestamp_for_type(raw: &str, type_name: &str) -> Option<String> {
    let normalized = normalize_timestamp(raw);
    let (body, zone) = normalized
        .strip_suffix("+00")
        .map(|body| (body, "+00"))
        .unwrap_or((normalized.as_str(), ""));
    let (prefix, fraction) = body
        .split_once('.')
        .map(|(prefix, fraction)| (prefix, fraction))
        .unwrap_or((body, ""));
    let digits = match type_name {
        "TIMESTAMP_MS" => 3,
        "TIMESTAMP_SEC" => 0,
        _ => 6,
    };
    let fraction = if digits == 0 {
        String::new()
    } else {
        let mut value: String = fraction
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .take(digits)
            .collect();
        while value.len() < digits && !value.is_empty() {
            value.push('0');
        }
        if value.is_empty() {
            String::new()
        } else {
            format!(".{value}")
        }
    };
    let zone = if type_name == "TIMESTAMP_TZ" {
        "+00"
    } else {
        zone
    };
    Some(format!("{prefix}{fraction}{zone}"))
}

/// Normalise a verbose interval literal to Kuzu's printer form. Kuzu
/// emits years and months as words but rolls hours / minutes / seconds
/// (plus optional microseconds) into a compact `HH:MM:SS[.uuuuuu]`
/// tail. The CSV input may use any of `hours`, `minutes`, `seconds`,
/// `milliseconds`, `microseconds`, or the abbreviation `us`.
fn normalize_interval(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return raw.to_string();
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let mut prefix: Vec<String> = Vec::new();
    let mut hours: i64 = 0;
    let mut minutes: i64 = 0;
    let mut seconds: i64 = 0;
    let mut micros: i64 = 0;
    let mut saw_time_component = false;
    let mut i = 0;
    while i < parts.len() {
        let value_part = parts[i];
        let (value, unit, consumed) = match value_part.parse::<i64>() {
            Ok(v) if i + 1 < parts.len() => (
                v,
                parts[i + 1].trim_end_matches(',').to_ascii_lowercase(),
                2,
            ),
            _ => match split_compact_interval_part(value_part) {
                Some((value, unit)) => (value, unit, 1),
                None => return raw.to_string(),
            },
        };
        match unit.as_str() {
            "year" | "years" | "y" | "yr" | "yrs" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "year" } else { "years" }
                ));
            }
            "month" | "months" | "mon" | "mons" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "month" } else { "months" }
                ));
            }
            "week" | "weeks" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "week" } else { "weeks" }
                ));
            }
            "day" | "days" | "d" => {
                prefix.push(format!(
                    "{value} {}",
                    if value.abs() == 1 { "day" } else { "days" }
                ));
            }
            "hour" | "hours" | "h" => {
                hours = value;
                saw_time_component = true;
            }
            "minute" | "minutes" | "min" | "mins" | "m" => {
                minutes = value;
                saw_time_component = true;
            }
            "second" | "seconds" | "sec" | "secs" | "s" => {
                seconds = value;
                saw_time_component = true;
            }
            "millisecond" | "milliseconds" | "ms" => {
                micros += value * 1_000;
                saw_time_component = true;
            }
            "microsecond" | "microseconds" | "us" | "µs" => {
                micros += value;
                saw_time_component = true;
            }
            _ => return raw.to_string(),
        }
        i += consumed;
    }
    if !saw_time_component {
        return if prefix.is_empty() {
            raw.to_string()
        } else {
            prefix.join(" ")
        };
    }
    let time_tail = if micros != 0 {
        format!(
            "{hours:02}:{minutes:02}:{seconds:02}.{}",
            trim_interval_fraction(micros)
        )
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    };
    if prefix.is_empty() {
        time_tail
    } else {
        format!("{} {time_tail}", prefix.join(" "))
    }
}

fn split_compact_interval_part(part: &str) -> Option<(i64, String)> {
    let split = part
        .char_indices()
        .find_map(|(idx, ch)| (idx > 0 && !ch.is_ascii_digit() && ch != '-').then_some(idx))?;
    let (value, unit) = part.split_at(split);
    Some((value.parse().ok()?, unit.to_ascii_lowercase()))
}

fn trim_interval_fraction(micros: i64) -> String {
    let mut fraction = format!("{:06}", micros.abs());
    while fraction.ends_with('0') {
        fraction.pop();
    }
    if fraction.is_empty() {
        "0".to_string()
    } else {
        fraction
    }
}

/// Lowercase and hyphenate a UUID-shaped string to Kuzu's canonical
/// 8-4-4-4-12 form. Anything that doesn't look like a UUID after the
/// strip passes through.
fn normalize_uuid(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = trimmed.trim_start_matches('{').trim_end_matches('}').trim();
    let hex: String = inner.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return raw.to_string();
    }
    let lower = hex.to_ascii_lowercase();
    format!(
        "{}-{}-{}-{}-{}",
        &lower[..8],
        &lower[8..12],
        &lower[12..16],
        &lower[16..20],
        &lower[20..32]
    )
}

/// Normalize a CSV timestamp string to Kuzu's printer form:
/// `YYYY-MM-DD HH:MM:SS[.fff]`, with timezone offsets folded into UTC.
/// Anything we can't parse passes through untouched so we don't lose
/// information on weird inputs.
fn normalize_timestamp(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return raw.to_string();
    }
    // Split into date + time part on `T` or ` `.
    let Some((date, time)) = trimmed.split_once('T').or_else(|| trimmed.split_once(' ')) else {
        return normalize_date(trimmed);
    };
    let date_norm = normalize_date(date);

    // Detect timezone suffix.
    let (time_body, offset_minutes) = if let Some(body) = time.strip_suffix('Z') {
        (body.to_string(), 0i32)
    } else if let Some(idx) = time
        .char_indices()
        .rev()
        .find_map(|(i, c)| (i > 0 && (c == '+' || c == '-')).then_some(i))
    {
        let (body, offset) = time.split_at(idx);
        let sign: i32 = if offset.starts_with('-') { -1 } else { 1 };
        let mut parts = offset[1..].split(':');
        let hours: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minutes: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        // Kuzu's CSV occasionally produces `…Z+00:00` (a `Z` followed
        // by an explicit offset). Strip a residual `Z` so the body is
        // a clean `HH:MM:SS[.fff]`.
        let body = body.trim_end_matches('Z');
        (body.to_string(), sign * (hours * 60 + minutes))
    } else {
        (time.to_string(), 0)
    };

    // Apply the offset by treating the value as an epoch-millis number.
    if offset_minutes == 0 {
        return format!("{date_norm} {time_body}");
    }
    let mut date_parts = date_norm.split('-');
    let year: i64 = date_parts
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1970);
    let month: u32 = date_parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let day: u32 = date_parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut time_parts = time_body.split(':');
    let hour: i64 = time_parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minute: i64 = time_parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let second_part = time_parts.next().unwrap_or("0");
    let (second, fraction) = if let Some((s, f)) = second_part.split_once('.') {
        (s.parse::<i64>().unwrap_or(0), Some(f.to_string()))
    } else {
        (second_part.parse::<i64>().unwrap_or(0), None)
    };
    let Some(days) = days_from_civil(year, month, day) else {
        return format!("{date_norm} {time_body}");
    };
    let local_secs = days * 86_400 + hour * 3600 + minute * 60 + second;
    let utc_secs = local_secs - (offset_minutes as i64) * 60;
    let new_days = utc_secs.div_euclid(86_400);
    let secs_of_day = utc_secs.rem_euclid(86_400);
    let new_hour = secs_of_day / 3600;
    let new_minute = (secs_of_day % 3600) / 60;
    let new_second = secs_of_day % 60;
    let Some((y, m, d)) = civil_from_days(new_days) else {
        return format!("{date_norm} {time_body}");
    };
    let frac = fraction.map(|f| format!(".{f}")).unwrap_or_default();
    format!("{y:04}-{m:02}-{d:02} {new_hour:02}:{new_minute:02}:{new_second:02}{frac}")
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = month as i64 + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era.checked_mul(146_097)?
        .checked_add(doe)?
        .checked_sub(719_468)
}

fn civil_from_days(days_since_epoch: i64) -> Option<(i64, u32, u32)> {
    let z = days_since_epoch.checked_add(719_468)?;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    Some((year, m as u32, d as u32))
}

/// Zero-pad a `YYYY-M-D` style date to `YYYY-MM-DD`. Leaves anything
/// that doesn't look like a date alone.
fn normalize_date(raw: &str) -> String {
    let trimmed = raw.trim();
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 3 {
        return raw.to_string();
    }
    let (year, month, day) = (parts[0], parts[1], parts[2]);
    if year.is_empty() || month.is_empty() || day.is_empty() {
        return raw.to_string();
    }
    if !year.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
        || !day.chars().all(|c| c.is_ascii_digit())
    {
        return raw.to_string();
    }
    format!("{year:0>4}-{month:0>2}-{day:0>2}")
}

fn parse_date_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let (year, month, day) = (parts[0], parts[1], parts[2]);
    if year.is_empty() || month.is_empty() || day.is_empty() {
        return None;
    }
    if !year.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
        || !day.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(normalize_date(raw))
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
