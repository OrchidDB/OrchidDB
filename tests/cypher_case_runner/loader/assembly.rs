//! Graph assembly and inline CREATE fixture setup.

use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::value::Value;
use num_bigint::BigInt;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::str::FromStr;
use super::DatasetError;
use super::schema::{CopyEntry, Schema, balanced_inner, split_statements};
use super::tables::{build_edge_table, build_node_table, record_edge_table};
use super::values::{raw_map_entries, split_value_items, unquote};

// ============================================================
// Property graph assembly
// ============================================================

pub(super) fn build_property_graph(
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

pub(super) fn apply_inline_schema_creates(
    schema_text: &str,
    graph: &mut PropertyGraph,
) -> Result<(), DatasetError> {
    let mut aliases: HashMap<String, (String, i64)> = HashMap::new();
    let mut edge_groups: Vec<((String, String, String), Vec<(i64, i64)>)> = Vec::new();

    for stmt in split_statements(schema_text) {
        let lower = stmt.to_ascii_lowercase();
        if lower.starts_with("create node table")
            || lower.starts_with("create rel table")
            || lower.starts_with("create rel table group")
        {
            continue;
        }
        for body in split_inline_create_clauses(&stmt) {
            for pattern in split_value_items(body) {
                let pattern = pattern.trim();
                if pattern.is_empty() {
                    continue;
                }
                if let Some((src_alias, rel_type, dst_alias)) = parse_inline_edge_pattern(pattern) {
                    let Some((src_label, src_id)) = aliases.get(&src_alias).cloned() else {
                        continue;
                    };
                    let Some((dst_label, dst_id)) = aliases.get(&dst_alias).cloned() else {
                        continue;
                    };
                    push_inline_edge_group(
                        &mut edge_groups,
                        (rel_type, src_label, dst_label),
                        (src_id, dst_id),
                    );
                } else if let Some((alias, label, properties)) = parse_inline_node_pattern(pattern)
                {
                    let value = graph.insert_node(label.clone(), properties);
                    if let (Some(alias), Value::Node { id, .. }) = (alias, value) {
                        aliases.insert(alias, (label, id));
                    }
                }
            }
        }
    }

    for ((rel_type, src_label, dst_label), endpoints) in edge_groups {
        let (src, dst): (Vec<i64>, Vec<i64>) = endpoints.into_iter().unzip();
        let table = record_edge_table(&rel_type, &src_label, &dst_label, src, dst, Vec::new())?;
        graph.add_edges(table).map_err(|err| {
            DatasetError(format!("inline CREATE relationship `{rel_type}`: {err}"))
        })?;
    }
    Ok(())
}

fn split_inline_create_clauses(stmt: &str) -> Vec<&str> {
    let mut starts = Vec::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let bytes = stmt.as_bytes();
    let lower = stmt.to_ascii_lowercase();
    let lower_bytes = lower.as_bytes();
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
        if in_backtick {
            if ch == '`' {
                in_backtick = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '`' => in_backtick = true,
            _ => {
                if i + 6 <= lower_bytes.len()
                    && &lower_bytes[i..i + 6] == b"create"
                    && (i == 0 || !is_inline_ident_char(bytes[i - 1] as char))
                    && (i + 6 == bytes.len() || !is_inline_ident_char(bytes[i + 6] as char))
                {
                    let after = i + 6;
                    if stmt[after..].trim_start().starts_with('(') {
                        starts.push(i);
                    }
                }
            }
        }
        i += 1;
    }
    let mut out = Vec::new();
    for (idx, start) in starts.iter().copied().enumerate() {
        let body_start = start + 6;
        let body_end = starts.get(idx + 1).copied().unwrap_or(stmt.len());
        let body = stmt[body_start..body_end].trim();
        if !body.is_empty() {
            out.push(body);
        }
    }
    out
}

fn is_inline_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn parse_inline_node_pattern(
    pattern: &str,
) -> Option<(Option<String>, String, BTreeMap<String, Value>)> {
    if pattern.contains(")-") || pattern.contains("]-") {
        return None;
    }
    let inner = pattern
        .trim()
        .strip_prefix('(')
        .and_then(|body| balanced_inner(body))?;
    let (head, props) = if let Some(open) = inner.find('{') {
        let head = inner[..open].trim();
        let props = inner[open..].trim();
        (head, Some(props))
    } else {
        (inner.trim(), None)
    };
    let (alias, label) = parse_inline_node_head(head)?;
    let properties = props.and_then(parse_inline_properties).unwrap_or_default();
    Some((alias, label, properties))
}

fn parse_inline_node_head(head: &str) -> Option<(Option<String>, String)> {
    let (alias, labels) = head.split_once(':')?;
    let alias = alias.trim();
    let label = labels
        .split(':')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim();
    if label.is_empty() {
        return None;
    }
    Some((
        (!alias.is_empty()).then(|| alias.trim_matches('`').to_string()),
        label.trim_matches('`').to_string(),
    ))
}

fn parse_inline_properties(raw: &str) -> Option<BTreeMap<String, Value>> {
    raw_map_entries(raw).map(|entries| {
        entries
            .into_iter()
            .map(|(key, value)| (key, parse_inline_value(&value)))
            .collect()
    })
}

fn parse_inline_edge_pattern(pattern: &str) -> Option<(String, String, String)> {
    let trimmed = pattern.trim();
    let first_close = trimmed.find(')')?;
    let src = trimmed
        .strip_prefix('(')
        .map(|body| body[..first_close - 1].trim().trim_matches('`').to_string())?;
    let after_src = &trimmed[first_close + 1..];
    let rel_open = after_src.find('[')?;
    let rel_close = after_src[rel_open + 1..].find(']')? + rel_open + 1;
    let rel_spec = after_src[rel_open + 1..rel_close].trim();
    let rel_type = rel_spec
        .split_once(':')
        .map(|(_, ty)| ty)
        .unwrap_or(rel_spec)
        .split('|')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('`')
        .to_string();
    if src.is_empty() || rel_type.is_empty() {
        return None;
    }
    let after_rel = &after_src[rel_close + 1..];
    let dst_open = after_rel.rfind('(')?;
    let dst_close = after_rel[dst_open + 1..].find(')')? + dst_open + 1;
    let dst = after_rel[dst_open + 1..dst_close]
        .trim()
        .trim_matches('`')
        .to_string();
    (!dst.is_empty()).then_some((src, rel_type, dst))
}

fn parse_inline_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if trimmed.starts_with('\'') || trimmed.starts_with('"') {
        return Value::String(unquote(trimmed).to_string());
    }
    if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if inner.trim().is_empty() {
            return Value::List(Vec::new());
        }
        return Value::List(
            split_value_items(inner)
                .into_iter()
                .map(parse_inline_value)
                .collect(),
        );
    }
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Value::Map(parse_inline_properties(trimmed).unwrap_or_default());
    }
    if let Ok(value) = trimmed.parse::<i64>() {
        return Value::Int(value);
    }
    if trimmed
        .strip_prefix(['-', '+'])
        .unwrap_or(trimmed)
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        && let Ok(value) = BigInt::from_str(trimmed)
    {
        return Value::BigInt(value);
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return Value::Float(value);
    }
    Value::String(trimmed.to_string())
}

fn push_inline_edge_group(
    groups: &mut Vec<((String, String, String), Vec<(i64, i64)>)>,
    key: (String, String, String),
    endpoint: (i64, i64),
) {
    if let Some((_, endpoints)) = groups.iter_mut().find(|(existing, _)| existing == &key) {
        endpoints.push(endpoint);
    } else {
        groups.push((key, vec![endpoint]));
    }
}
