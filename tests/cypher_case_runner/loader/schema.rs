//! Fixture schema declarations and COPY directives.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ColumnType {
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
pub(super) struct Column {
    pub(super) name: String,
    pub(super) ty: ColumnType,
}

#[derive(Debug, Clone)]
pub(super) struct NodeDef {
    pub(super) label: String,
    pub(super) columns: Vec<Column>,
    pub(super) pk_index: usize,
}

#[derive(Debug, Clone)]
pub(super) struct EdgeDef {
    pub(super) rel_type: String,
    pub(super) src_label: String,
    pub(super) dst_label: String,
    pub(super) properties: Vec<Column>,
}

#[derive(Debug)]
pub(super) struct Schema {
    pub(super) nodes: HashMap<String, NodeDef>,
    pub(super) edges: HashMap<String, EdgeDef>,
    /// Order in which `CREATE NODE TABLE` declarations appeared in the
    /// schema. Kuzu numbers node tables in this order when printing
    /// node `_ID` values, so the loader has to remember it instead of
    /// falling back to alphabetic iteration over the HashMap.
    pub(super) node_order: Vec<String>,
    /// Same for `CREATE REL TABLE`.
    pub(super) edge_order: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct CopyEntry {
    pub(super) table: String,
    pub(super) file: String,
    /// `true` when the CSV's first row is the header. Inferred from the
    /// `(header = true)` option in `COPY` statements; otherwise we
    /// derive it heuristically when loading the file.
    pub(super) has_header_hint: Option<bool>,
}

// ============================================================
// Schema parsing
// ============================================================

pub(super) fn parse_schema(text: &str) -> Schema {
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
                for def in parse_rel_body(&label, &body) {
                    let key = label.to_ascii_lowercase();
                    if !edge_order.contains(&key) {
                        edge_order.push(key.clone());
                    }
                    let def_key = if edges.contains_key(&key) {
                        format!(
                            "{}@{}->{}",
                            def.rel_type.to_ascii_lowercase(),
                            def.src_label.to_ascii_lowercase(),
                            def.dst_label.to_ascii_lowercase()
                        )
                    } else {
                        key.clone()
                    };
                    edges.insert(def_key, def);
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

pub(super) fn parse_identifier(text: &str) -> Option<(String, &str)> {
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
pub(super) fn balanced_inner(body: &str) -> Option<String> {
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

fn parse_rel_body(label: &str, body: &str) -> Vec<EdgeDef> {
    let parts = split_top_level_commas(body);
    if parts.is_empty() {
        return Vec::new();
    }
    let mut from_to_pairs = Vec::new();
    let mut properties = Vec::new();
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
        .map(|(src_label, dst_label)| EdgeDef {
            rel_type: label.to_string(),
            src_label,
            dst_label,
            properties: properties.clone(),
        })
        .collect()
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

pub(super) fn split_statements(text: &str) -> Vec<String> {
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

pub(super) fn parse_copies(text: &str) -> Vec<CopyEntry> {
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
