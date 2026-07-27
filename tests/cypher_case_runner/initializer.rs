//! Per-scenario graph initializer for the Cypher case runner.
//!
//! Many TCK cases (e.g. `tck/match/*`, `tck/with/*`) reference the
//! placeholder `CSV tck` dataset, but no shared schema/CSV exists for
//! them — each TCK scenario sets up its own tiny graph. To honour
//! that without seeding fixtures from expected output, a case file may
//! include a `--- graph_initializer` section using the structured DSL
//! parsed here.
//!
//! Format (structured, NOT raw Cypher):
//!
//! ```text
//! --- graph_initializer
//! node a:Person {id: 0, name: "Alice", age: 30}
//! node b:Person {id: 1, name: "Bob"}
//! node x:Movie  {title: "Inception"}
//! edge a -[:KNOWS {since: 2020}]-> b
//! edge a -[:LIKES]-> x
//! ```
//!
//! Each `node` line introduces an alias bound to a fresh vertex of the
//! given label; every property literal is a string, integer, float, or
//! boolean (no nested structures). Aliases are local to the case.
//!
//! Each `edge` line connects two aliases with a relationship type and
//! optional property map. Source and target labels are derived from the
//! aliases — we don't ask the case author to repeat them. The harness
//! groups edges by `(rel_type, src_label, dst_label)` and emits one
//! `EdgeTable` per group, which is what `PropertyGraph::add_edges`
//! expects.
//!
//! Comments (lines starting with `#`) and blank lines are ignored.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int64Array, StringArray};

use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};

use super::dataset::DatasetError;

/// One value literal supported in initializer property maps.
#[derive(Debug, Clone)]
pub enum InitValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Bool,
    Int,
    Float,
    String,
}

#[derive(Debug, Clone)]
struct NodeDecl {
    alias: String,
    label: String,
    properties: BTreeMap<String, InitValue>,
}

#[derive(Debug, Clone)]
struct EdgeDecl {
    rel_type: String,
    from_alias: String,
    to_alias: String,
    properties: BTreeMap<String, InitValue>,
}

#[derive(Debug, Default)]
pub struct ScenarioGraph {
    nodes: Vec<NodeDecl>,
    edges: Vec<EdgeDecl>,
}

/// Parse a `--- graph_initializer` body into a structured scenario.
pub fn parse(source: &str) -> Result<ScenarioGraph, DatasetError> {
    let mut graph = ScenarioGraph::default();
    for (lineno, raw_line) in source.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = strip_keyword(line, "node") {
            let node = parse_node(rest).map_err(|e| init_err(lineno, &e))?;
            graph.nodes.push(node);
        } else if let Some(rest) = strip_keyword(line, "edge") {
            let edge = parse_edge(rest).map_err(|e| init_err(lineno, &e))?;
            graph.edges.push(edge);
        } else {
            return Err(init_err(
                lineno,
                &format!("expected `node` or `edge`, got `{line}`"),
            ));
        }
    }
    Ok(graph)
}

/// Build a `PropertyGraph` from a parsed scenario.
pub fn build(scenario: &ScenarioGraph) -> Result<PropertyGraph, DatasetError> {
    // Group node declarations by label so each label becomes one
    // NodeTable. Track the row of each alias inside its label for
    // edge wiring.
    let mut by_label: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut alias_to_index: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, node) in scenario.nodes.iter().enumerate() {
        if alias_to_index.insert(node.alias.clone(), idx).is_some() {
            return Err(DatasetError(format!(
                "duplicate node alias `{}`",
                node.alias
            )));
        }
        by_label.entry(node.label.clone()).or_default().push(idx);
    }

    let mut alias_row: BTreeMap<String, (String, i64)> = BTreeMap::new();
    let mut graph = PropertyGraph::new();
    for (label, idxs) in &by_label {
        for (row, idx) in idxs.iter().enumerate() {
            alias_row.insert(
                scenario.nodes[*idx].alias.clone(),
                (label.clone(), row as i64),
            );
        }
        let table = build_node_table(label, idxs, &scenario.nodes)?;
        graph.add_nodes(table);
    }

    // Group edges by (rel_type, src_label, dst_label) so each group
    // becomes one EdgeTable.
    let mut edge_groups: BTreeMap<(String, String, String), Vec<&EdgeDecl>> = BTreeMap::new();
    for edge in &scenario.edges {
        let from = alias_row.get(&edge.from_alias).ok_or_else(|| {
            DatasetError(format!(
                "edge references unknown alias `{}`",
                edge.from_alias
            ))
        })?;
        let to = alias_row.get(&edge.to_alias).ok_or_else(|| {
            DatasetError(format!("edge references unknown alias `{}`", edge.to_alias))
        })?;
        edge_groups
            .entry((edge.rel_type.clone(), from.0.clone(), to.0.clone()))
            .or_default()
            .push(edge);
    }
    for ((rel_type, src_label, dst_label), edges) in &edge_groups {
        let table = build_edge_table(rel_type, src_label, dst_label, edges, &alias_row)?;
        graph
            .add_edges(table)
            .map_err(|err| DatasetError(format!("add edges: {err}")))?;
    }
    Ok(graph)
}

fn build_node_table(
    label: &str,
    rows: &[usize],
    nodes: &[NodeDecl],
) -> Result<new_graph::ir::catalog::NodeTable, DatasetError> {
    let kinds = collect_property_kinds(rows.iter().map(|i| &nodes[*i].properties))?;
    if kinds.is_empty() {
        // Property-less nodes: Arrow cannot carry rows without columns,
        // so emit a hidden `__row` column to materialize the row count.
        // The cypher formatter strips `__`-prefixed keys from node
        // renders, so the synthetic column never leaks into output.
        let ids: Vec<i64> = (0..rows.len() as i64).collect();
        let column: ArrayRef = Arc::new(Int64Array::from(ids));
        return Ok(nodes_from_columns(
            label.to_string(),
            vec![("__row", column)],
        ));
    }
    let columns: Vec<(&str, ArrayRef)> = kinds
        .iter()
        .map(|(name, kind)| {
            let values: Vec<Option<&InitValue>> = rows
                .iter()
                .map(|i| nodes[*i].properties.get(name.as_str()))
                .collect();
            (name.as_str(), value_column(*kind, &values))
        })
        .collect();
    Ok(nodes_from_columns(label.to_string(), columns))
}

fn build_edge_table(
    rel_type: &str,
    src_label: &str,
    dst_label: &str,
    rows: &[&EdgeDecl],
    alias_row: &BTreeMap<String, (String, i64)>,
) -> Result<new_graph::ir::catalog::EdgeTable, DatasetError> {
    let src: Vec<i64> = rows
        .iter()
        .map(|e| alias_row.get(&e.from_alias).unwrap().1)
        .collect();
    let dst: Vec<i64> = rows
        .iter()
        .map(|e| alias_row.get(&e.to_alias).unwrap().1)
        .collect();
    let kinds = collect_property_kinds(rows.iter().map(|e| &e.properties))?;
    let columns: Vec<(&str, ArrayRef)> = kinds
        .iter()
        .map(|(name, kind)| {
            let values: Vec<Option<&InitValue>> = rows
                .iter()
                .map(|e| e.properties.get(name.as_str()))
                .collect();
            (name.as_str(), value_column(*kind, &values))
        })
        .collect();
    Ok(edges_from_columns(
        rel_type.to_string(),
        src_label.to_string(),
        dst_label.to_string(),
        src,
        dst,
        columns,
    ))
}

/// Decide one Arrow type per property name across all rows. The widest
/// observed value wins (Bool < Int < Float < String); a `String` value
/// anywhere in the column forces a String column.
fn collect_property_kinds<'a, I>(rows: I) -> Result<Vec<(String, ValueKind)>, DatasetError>
where
    I: IntoIterator<Item = &'a BTreeMap<String, InitValue>>,
{
    let mut order: Vec<String> = Vec::new();
    let mut kinds: BTreeMap<String, ValueKind> = BTreeMap::new();
    for props in rows {
        for (key, value) in props {
            let kind = match value {
                InitValue::Null => continue,
                InitValue::Bool(_) => ValueKind::Bool,
                InitValue::Int(_) => ValueKind::Int,
                InitValue::Float(_) => ValueKind::Float,
                InitValue::String(_) => ValueKind::String,
            };
            kinds
                .entry(key.clone())
                .and_modify(|existing| *existing = widen(*existing, kind))
                .or_insert(kind);
            if !order.contains(key) {
                order.push(key.clone());
            }
        }
    }
    let out = order
        .into_iter()
        .map(|name| {
            let kind = kinds.remove(&name).unwrap_or(ValueKind::String);
            (name, kind)
        })
        .collect();
    Ok(out)
}

fn widen(a: ValueKind, b: ValueKind) -> ValueKind {
    use ValueKind::*;
    match (a, b) {
        (String, _) | (_, String) => String,
        (Float, _) | (_, Float) => Float,
        (Int, _) | (_, Int) => Int,
        _ => Bool,
    }
}

fn value_column(kind: ValueKind, values: &[Option<&InitValue>]) -> ArrayRef {
    match kind {
        ValueKind::Bool => Arc::new(BooleanArray::from(
            values.iter().map(|v| as_bool(*v)).collect::<Vec<_>>(),
        )),
        ValueKind::Int => Arc::new(Int64Array::from(
            values.iter().map(|v| as_i64(*v)).collect::<Vec<_>>(),
        )),
        ValueKind::Float => Arc::new(Float64Array::from(
            values.iter().map(|v| as_f64(*v)).collect::<Vec<_>>(),
        )),
        ValueKind::String => {
            let owned: Vec<Option<String>> = values.iter().map(|v| as_string(*v)).collect();
            let refs: Vec<Option<&str>> = owned.iter().map(|s| s.as_deref()).collect();
            Arc::new(StringArray::from(refs))
        }
    }
}

fn as_bool(value: Option<&InitValue>) -> Option<bool> {
    match value? {
        InitValue::Bool(b) => Some(*b),
        _ => None,
    }
}

fn as_i64(value: Option<&InitValue>) -> Option<i64> {
    match value? {
        InitValue::Int(i) => Some(*i),
        InitValue::Float(f) => Some(*f as i64),
        InitValue::Bool(b) => Some(if *b { 1 } else { 0 }),
        _ => None,
    }
}

fn as_f64(value: Option<&InitValue>) -> Option<f64> {
    match value? {
        InitValue::Float(f) => Some(*f),
        InitValue::Int(i) => Some(*i as f64),
        InitValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn as_string(value: Option<&InitValue>) -> Option<String> {
    match value? {
        InitValue::String(s) => Some(s.clone()),
        InitValue::Int(i) => Some(i.to_string()),
        InitValue::Float(f) => Some(format!("{f}")),
        InitValue::Bool(b) => Some(b.to_string()),
        InitValue::Null => None,
    }
}

// ============================================================
// Line parsers
// ============================================================

fn parse_node(rest: &str) -> Result<NodeDecl, String> {
    // <alias>:<label> [{props}]
    let trimmed = rest.trim();
    let (head, props_text) = split_props(trimmed);
    let head = head.trim();
    let (alias, label) = head
        .split_once(':')
        .ok_or_else(|| format!("node header must be `alias:Label`, got `{head}`"))?;
    let alias = alias.trim();
    let label = label.trim();
    if alias.is_empty() || label.is_empty() {
        return Err("alias and label must both be non-empty".into());
    }
    let properties = match props_text {
        Some(body) => parse_property_map(body)?,
        None => BTreeMap::new(),
    };
    Ok(NodeDecl {
        alias: alias.to_string(),
        label: label.to_string(),
        properties,
    })
}

fn parse_edge(rest: &str) -> Result<EdgeDecl, String> {
    // <from> -[:RelType {props}]-> <to>
    let trimmed = rest.trim();
    let dash_idx = trimmed
        .find('-')
        .ok_or_else(|| format!("edge missing leading `-`: `{trimmed}`"))?;
    let from_alias = trimmed[..dash_idx].trim().to_string();
    let after_from = &trimmed[dash_idx + 1..];
    let body_start = after_from
        .find('[')
        .ok_or_else(|| format!("edge missing `[`: `{trimmed}`"))?;
    let after_open = &after_from[body_start + 1..];
    let body_end = after_open
        .find(']')
        .ok_or_else(|| format!("edge missing `]`: `{trimmed}`"))?;
    let body = &after_open[..body_end];
    let after_body = &after_open[body_end + 1..];
    let arrow_idx = after_body
        .find("->")
        .ok_or_else(|| format!("edge missing `->`: `{trimmed}`"))?;
    let to_alias = after_body[arrow_idx + 2..].trim().to_string();

    // Body is `:RelType` or `:RelType {props}`.
    let body = body.trim();
    let body = body
        .strip_prefix(':')
        .ok_or_else(|| format!("edge body must start with `:`, got `{body}`"))?;
    let (rel_text, props_text) = split_props(body);
    let rel_type = rel_text.trim().to_string();
    if rel_type.is_empty() {
        return Err("edge must declare a relationship type".into());
    }
    let properties = match props_text {
        Some(map) => parse_property_map(map)?,
        None => BTreeMap::new(),
    };
    if from_alias.is_empty() || to_alias.is_empty() {
        return Err("edge endpoints must both be non-empty aliases".into());
    }
    Ok(EdgeDecl {
        rel_type,
        from_alias,
        to_alias,
        properties,
    })
}

/// Split a header off any trailing `{...}` property map. Returns the
/// head text and the inner-map text (without braces) when present.
fn split_props(text: &str) -> (&str, Option<&str>) {
    let Some(open) = text.find('{') else {
        return (text, None);
    };
    let after = &text[open + 1..];
    let Some(close) = after.rfind('}') else {
        return (text, None);
    };
    (&text[..open], Some(&after[..close]))
}

fn parse_property_map(body: &str) -> Result<BTreeMap<String, InitValue>, String> {
    let mut out = BTreeMap::new();
    for raw_entry in split_top_level_commas(body) {
        let entry = raw_entry.trim();
        if entry.is_empty() {
            continue;
        }
        let colon = entry
            .find(':')
            .ok_or_else(|| format!("property entry missing `:` in `{entry}`"))?;
        let key = entry[..colon].trim().trim_matches('"').to_string();
        let value = parse_value(entry[colon + 1..].trim())?;
        out.insert(key, value);
    }
    Ok(out)
}

fn parse_value(text: &str) -> Result<InitValue, String> {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("null") {
        return Ok(InitValue::Null);
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Ok(InitValue::Bool(true));
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Ok(InitValue::Bool(false));
    }
    if let Some(stripped) = strip_string(trimmed) {
        return Ok(InitValue::String(stripped));
    }
    if let Ok(i) = trimmed.parse::<i64>() {
        return Ok(InitValue::Int(i));
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return Ok(InitValue::Float(f));
    }
    Err(format!("unrecognised literal `{trimmed}`"))
}

fn strip_string(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote != b'"' && quote != b'\'') || bytes[bytes.len() - 1] != quote {
        return None;
    }
    let inner = &text[1..text.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => break,
            }
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn split_top_level_commas(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth_brace = 0i32;
    let mut depth_bracket = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_single {
            if ch == '\'' && bytes[i.saturating_sub(1)] != b'\\' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if ch == '"' && bytes[i.saturating_sub(1)] != b'\\' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            ',' if depth_brace == 0 && depth_bracket == 0 => {
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

fn strip_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let lower = line.to_ascii_lowercase();
    if !lower.starts_with(keyword) {
        return None;
    }
    let after = &line[keyword.len()..];
    // Require whitespace after the keyword to avoid matching identifiers.
    if !after.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    Some(after)
}

fn strip_comment(line: &str) -> &str {
    if let Some(idx) = line.find('#') {
        // Don't strip `#` inside quoted strings; case files we author
        // here don't use `#` in literals, so a leading `#` check is
        // sufficient. Same approach for `//`.
        if idx == 0 || line[..idx].trim().is_empty() {
            return &line[..idx];
        }
    }
    line
}

fn init_err(lineno: usize, msg: &str) -> DatasetError {
    DatasetError(format!("graph_initializer line {}: {msg}", lineno + 1))
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_scenario() {
        let src = r#"
            node a:Person {id: 0, name: "Alice", age: 30}
            node b:Person {id: 1, name: "Bob"}
            edge a -[:KNOWS {since: 2020}]-> b
        "#;
        let scenario = parse(src).expect("parse");
        assert_eq!(scenario.nodes.len(), 2);
        assert_eq!(scenario.edges.len(), 1);
        let graph = build(&scenario).expect("build");
        let person = graph.nodes.get("Person").expect("Person table");
        assert_eq!(person.batch.num_rows(), 2);
        assert!(graph.edges.contains_key("KNOWS"));
    }

    #[test]
    fn duplicate_aliases_are_rejected() {
        let src = "node a:Person {id: 0}\nnode a:Person {id: 1}";
        let scenario = parse(src).expect("parse");
        let err = build(&scenario).expect_err("duplicate alias must error");
        assert!(err.0.contains("duplicate node alias"));
    }

    #[test]
    fn unknown_alias_in_edge_errors() {
        let src = "node a:Person {id: 0}\nedge a -[:KNOWS]-> ghost";
        let scenario = parse(src).expect("parse");
        let err = build(&scenario).expect_err("unknown alias must error");
        assert!(err.0.contains("unknown alias"));
    }

    #[test]
    fn dataset_path_prefers_inline_initializer() {
        // Sanity-check the dataset facade: when an initializer is
        // supplied, the named dataset (here a bogus one) is ignored and
        // the inline graph wins. This mirrors the runner glue used by
        // `cypher_ladybug_cases.rs`.
        let src = r#"
            node a:Person {id: 0, name: "Zoe"}
            node b:Person {id: 1, name: "Yara"}
            edge a -[:KNOWS]-> b
        "#;
        let graph = super::super::dataset::build_with_initializer("CSV tck", Some(src))
            .expect("inline init build");
        let person = graph.nodes.get("Person").expect("Person table");
        assert_eq!(person.batch.num_rows(), 2);
        let knows = graph.edges.get("KNOWS").expect("KNOWS table");
        assert_eq!(knows.batch.num_rows(), 1);
        assert_eq!(knows.src_label, "Person");
        assert_eq!(knows.dst_label, "Person");
    }

    #[test]
    fn mixed_label_groups_to_separate_node_tables() {
        let src = r#"
            node a:Person {id: 0}
            node m:Movie  {title: "Inception"}
            edge a -[:LIKES]-> m
        "#;
        let scenario = parse(src).expect("parse");
        let graph = build(&scenario).expect("build");
        assert!(graph.nodes.contains_key("Person"));
        assert!(graph.nodes.contains_key("Movie"));
        assert_eq!(graph.edges.len(), 1);
    }
}
