//! Cypher (Ladybug) output formatter.
//!
//! The interpreter emits results in the gremlin-style "tagged" form
//! (`l[d[1].i,d[2].i]`, `v[label#id]`, `m[{"k":"v"}]`, `true` / `false`,
//! `null`). Ladybug case files expect a Python/Kuzu-style scalar form
//! with pipe-separated columns (`1|2`), bracketed lists (`[1,2]`),
//! capitalised booleans (`True` / `False`), and an empty cell for null.
//!
//! This module unwraps the interpreter's tagged form into the Ladybug
//! convention so the existing `compare` helper can match expected lines
//! directly. Anything we do not recognise is passed through verbatim,
//! which still surfaces real correctness issues rather than swallowing
//! them.

use arrow::array::{
    Array, BooleanArray, Float32Array, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array,
    ListArray, StringArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};
use arrow::datatypes::DataType;

use new_graph::ir::interpreter::ReturnedBatches;

pub fn lines_from_batch(batches: &ReturnedBatches) -> Vec<String> {
    let batch = &batches.batch;
    if batch.num_columns() == 0 {
        return (0..batch.num_rows()).map(|_| String::new()).collect();
    }

    if batch.num_columns() == 1 {
        return (0..batch.num_rows())
            .map(|row| render_cell(batch.column(0).as_ref(), row))
            .collect();
    }

    (0..batch.num_rows())
        .map(|row| {
            let parts: Vec<String> = (0..batch.num_columns())
                .map(|col| render_cell(batch.column(col).as_ref(), row))
                .collect();
            parts.join("|")
        })
        .collect()
}

fn render_cell(array: &dyn Array, row: usize) -> String {
    if row >= array.len() || array.is_null(row) {
        return String::new();
    }
    match array.data_type() {
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Int8 => array
            .as_any()
            .downcast_ref::<Int8Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Int16 => array
            .as_any()
            .downcast_ref::<Int16Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Int32 => array
            .as_any()
            .downcast_ref::<Int32Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::UInt8 => array
            .as_any()
            .downcast_ref::<UInt8Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::UInt16 => array
            .as_any()
            .downcast_ref::<UInt16Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::UInt32 => array
            .as_any()
            .downcast_ref::<UInt32Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::UInt64 => array
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Float64 => render_float(
            array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(row),
        ),
        DataType::Float32 => render_float(
            array
                .as_any()
                .downcast_ref::<Float32Array>()
                .unwrap()
                .value(row) as f64,
        ),
        DataType::Boolean => render_bool(
            array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .unwrap()
                .value(row),
        ),
        DataType::Utf8 => render_string_cell(
            array
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(row),
        ),
        DataType::List(_) => {
            let values = array
                .as_any()
                .downcast_ref::<ListArray>()
                .unwrap()
                .value(row);
            let items = (0..values.len())
                .map(|idx| render_cell(values.as_ref(), idx))
                .collect::<Vec<_>>();
            format!("[{}]", items.join(","))
        }
        _ => format!("?({:?})", array.data_type()),
    }
}

fn render_bool(value: bool) -> String {
    if value { "True".into() } else { "False".into() }
}

fn render_string_cell(value: &str) -> String {
    untag(value)
}

/// Strip a gremlin-style tag wrapper, recursing into nested lists / maps.
/// Anything we can't classify passes through, so a plain `"hello"` stays
/// `hello`, while `l[d[1].i,d[2].i]` becomes `[1,2]`.
fn untag(value: &str) -> String {
    let v = value.trim();
    if v == "null" {
        return String::new();
    }
    if v == "true" {
        return "True".into();
    }
    if v == "false" {
        return "False".into();
    }
    if let Some(rendered) = untag_list(v, "l[") {
        return rendered;
    }
    if let Some(rendered) = untag_list(v, "p[") {
        // Paths render the same as lists for case comparison; the
        // structured node/edge forms inside are unwrapped recursively.
        return rendered;
    }
    if let Some(rendered) = untag_map(v) {
        return rendered;
    }
    if let Some(rendered) = untag_scalar_double_bracket(v, "d[") {
        return rendered;
    }
    if let Some(rendered) = untag_simple_bracket(v, "v[") {
        return rendered;
    }
    if let Some(rendered) = untag_simple_bracket(v, "e[") {
        return rendered;
    }
    if let Some(rendered) = untag_simple_bracket(v, "dt[") {
        return rendered;
    }
    v.to_string()
}

/// `l[a,b,c]` → `[a,b,c]`, with each element recursively unwrapped.
fn untag_list(value: &str, prefix: &str) -> Option<String> {
    let inner = value.strip_prefix(prefix)?.strip_suffix(']')?;
    if inner.is_empty() {
        return Some("[]".to_string());
    }
    let items: Vec<String> = split_top_level(inner, ',')
        .into_iter()
        .map(|item| untag(item.trim()))
        .collect();
    Some(format!("[{}]", items.join(",")))
}

/// `m[{"k1":"v1","k2":"v2"}]` → `{k1: v1, k2: v2}`.
fn untag_map(value: &str) -> Option<String> {
    let inner = value.strip_prefix("m[{")?.strip_suffix("}]")?;
    if inner.is_empty() {
        return Some("{}".to_string());
    }
    let mut entries: Vec<(String, String)> = Vec::new();
    for entry in split_top_level(inner, ',') {
        let (key, raw_value) = entry.split_once(':')?;
        let key = key.trim().trim_matches('"').to_string();
        let value = untag(raw_value.trim().trim_matches('"'));
        entries.push((key, value));
    }
    let body: Vec<String> = entries
        .into_iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    Some(format!("{{{}}}", body.join(", ")))
}

/// `d[12].i` → `12`, `d[1.5].d` → `1.5`, `d[1.0].d` → `1.000000` (Kuzu
/// double convention — keep six trailing decimals so floats compare
/// numerically). Non-`d[…]` forms return `None`.
fn untag_scalar_double_bracket(value: &str, prefix: &str) -> Option<String> {
    let after = value.strip_prefix(prefix)?;
    let close = after.find(']')?;
    let body = &after[..close];
    let suffix = &after[close + 1..];
    match suffix {
        ".d" | ".f" | ".m" => Some(format_double(body)),
        ".i" | ".l" | ".s" | ".b" | ".n" => Some(body.to_string()),
        "" => Some(body.to_string()),
        _ => None,
    }
}

/// `v[name]` / `e[s-rel->d]` / `dt[…]` → unwrap to the inner body. The
/// case format mostly references nodes by a property (name) rather than
/// the structured form, so we let those flow through and rely on
/// `normalize_numbers` / property-tag stripping where it kicks in.
fn untag_simple_bracket(value: &str, prefix: &str) -> Option<String> {
    let inner = value.strip_prefix(prefix)?.strip_suffix(']')?;
    Some(inner.to_string())
}

fn format_double(body: &str) -> String {
    if let Ok(parsed) = body.parse::<f64>() {
        render_float(parsed)
    } else {
        body.to_string()
    }
}

fn render_float(value: f64) -> String {
    // Kuzu always prints doubles with six trailing decimals
    // (`4.700000`, `1.731000`), even when they have a non-trivial
    // fractional part; the conformance comparator uses
    // `normalize_numbers` to fold `1.731000` and `1.731` together so
    // padding here is safe.
    if !value.is_finite() {
        return value.to_string();
    }
    format!("{value:.6}")
}

/// Split a tagged body on `delim`, respecting bracket nesting and
/// double-quoted strings so `l[1,2,3]` and `"a, b"` survive intact.
fn split_top_level(input: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0usize;
    for (i, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quote.is_some() => escaped = true,
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            '[' | '{' | '(' if quote.is_none() => depth += 1,
            ']' | '}' | ')' if quote.is_none() => depth -= 1,
            c if c == delim && quote.is_none() && depth == 0 => {
                parts.push(&input[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

/// Cypher cases store expected output verbatim; no tag-stripping needed
/// on the expected side. Kept for API parity with the gremlin format
/// module so the shared `compare` helper compiles for both runners.
pub fn strip_expected_tags(line: &str) -> String {
    normalize_cypher_row(line)
}

pub fn ignore_unrepresented_empty_rows() -> bool {
    true
}

fn normalize_cypher_row(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    split_top_level(trimmed, '|')
        .into_iter()
        .map(normalize_cypher_cell)
        .collect::<Vec<_>>()
        .join("|")
}

fn normalize_cypher_cell(cell: &str) -> String {
    canonicalize_value(cell)
}

/// Canonicalize one cell (or nested value) so structurally-equal values
/// from different renders compare equal. Both the engine's Kuzu-style
/// render (`{_ID: 0:0, _LABEL: End, num: 42}`) and the openCypher TCK's
/// neutral render (`(:End {num: 42})`) fold into the same canonical node
/// text with internal identifiers (`_ID`, `__`-prefixed synthetic
/// columns) dropped. The transformation is applied symmetrically to the
/// actual and expected sides, so equal inputs always stay equal.
fn canonicalize_value(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return String::new();
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return "True".to_string();
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return "False".to_string();
    }
    // One layer of surrounding quotes: TCK strings are single-quoted,
    // engine output is bare. (Symmetric on both sides.)
    if let Some(inner) = strip_symmetric_quotes(trimmed) {
        return inner.to_string();
    }
    // Engine relationship render: `(0:0)-{_LABEL: T, _ID: 2:0, p: v}->(0:1)`.
    if let Some(canon) = canonicalize_engine_rel(trimmed) {
        return canon;
    }
    // TCK relationship render: `[:T]` / `[:T {p: v}]`.
    if let Some(canon) = canonicalize_tck_rel(trimmed) {
        return canon;
    }
    // Map / struct / engine node render.
    if let Some(inner) = trimmed.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return canonicalize_map_body(inner);
    }
    // TCK node render: `(:L {p: v})` / `(:L)` / `({p: v})` / `()`.
    if let Some(canon) = canonicalize_tck_node(trimmed) {
        return canon;
    }
    // List: recurse into items.
    if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if inner.trim().is_empty() {
            return "[]".to_string();
        }
        let items: Vec<String> = split_top_level(inner, ',')
            .into_iter()
            .map(canonicalize_value)
            .collect();
        return format!("[{}]", items.join(","));
    }
    trimmed.to_string()
}

fn strip_symmetric_quotes(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote == b'\'' || quote == b'"') && bytes[bytes.len() - 1] == quote {
        Some(&text[1..text.len() - 1])
    } else {
        None
    }
}

/// Parse a `k: v, k: v, …` body. Returns `(label, sorted props)` where
/// `label` comes from a `_LABEL` entry when present; `_ID` and
/// `__`-prefixed synthetic keys are dropped.
fn parse_props_body(inner: &str) -> (Option<String>, Vec<(String, String)>) {
    let mut label = None;
    let mut props: Vec<(String, String)> = Vec::new();
    for entry in split_top_level(inner, ',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once(':') else {
            props.push((entry.to_string(), String::new()));
            continue;
        };
        let key = key.trim().trim_matches('"').trim_matches('`').to_string();
        if key == "_ID" || key.starts_with("__") {
            continue;
        }
        if key == "_LABEL" {
            label = Some(value.trim().to_string());
            continue;
        }
        props.push((key, canonicalize_value(value)));
    }
    props.sort();
    (label, props)
}

fn render_props(props: &[(String, String)]) -> String {
    let body: Vec<String> = props.iter().map(|(k, v)| format!("{k}:{v}")).collect();
    format!("{{{}}}", body.join(","))
}

/// `{_ID: 0:0, _LABEL: End, num: 42}` → `(:End {num:42})`;
/// any other `{…}` body → `{k:v,…}` with sorted keys.
fn canonicalize_map_body(inner: &str) -> String {
    let (label, props) = parse_props_body(inner);
    match label {
        Some(label) => format!("(:{label} {})", render_props(&props)),
        None => render_props(&props),
    }
}

/// `(:L {p: 1})` / `(:L:M)` / `({p: 1})` / `()` → `(:L {p:1})` canonical.
fn canonicalize_tck_node(text: &str) -> Option<String> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    // Reject things that are clearly not node patterns (e.g. arithmetic
    // in parentheses): the inner text may only be labels + a prop map.
    let (head, props_body) = match inner.find('{') {
        Some(idx) => {
            let body = inner[idx..].strip_prefix('{')?.strip_suffix('}')?;
            (inner[..idx].trim(), Some(body))
        }
        None => (inner.trim(), None),
    };
    let labels = head.trim();
    if !labels.is_empty() {
        let stripped = labels.strip_prefix(':')?;
        if !stripped
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '`' | ' '))
        {
            return None;
        }
    }
    let (_, props) = match props_body {
        Some(body) => parse_props_body(body),
        None => (None, Vec::new()),
    };
    let label = labels.strip_prefix(':').unwrap_or("").trim().to_string();
    Some(format!("(:{label} {})", render_props(&props)))
}

/// `[:T {p: 1}]` / `[:T]` → `[:T {p:1}]` canonical.
fn canonicalize_tck_rel(text: &str) -> Option<String> {
    let inner = text.strip_prefix("[:")?.strip_suffix(']')?;
    let (ty, props_body) = match inner.find('{') {
        Some(idx) => {
            let body = inner[idx..].strip_prefix('{')?.strip_suffix('}')?;
            (inner[..idx].trim(), Some(body))
        }
        None => (inner.trim(), None),
    };
    if ty.is_empty() || !ty.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let (_, props) = match props_body {
        Some(body) => parse_props_body(body),
        None => (None, Vec::new()),
    };
    Some(format!("[:{ty} {}]", render_props(&props)))
}

/// Engine relationship render `(0:0)-{_LABEL: T, _ID: 2:0, p: v}->(0:1)`
/// → `[:T {p:v}]` canonical (endpoint internal ids dropped).
fn canonicalize_engine_rel(text: &str) -> Option<String> {
    if !text.starts_with('(') || !text.ends_with(')') {
        return None;
    }
    let open = text.find("-{")?;
    let close = text.find("}->")?;
    if close < open {
        return None;
    }
    let body = &text[open + 2..close];
    let (label, props) = parse_props_body(body);
    let ty = label?;
    Some(format!("[:{ty} {}]", render_props(&props)))
}

#[cfg(test)]
mod tests {
    use super::strip_expected_tags;

    #[test]
    fn normalizes_tck_scalar_rows_to_ladybug_cells() {
        assert_eq!(strip_expected_tags("true | null | false"), "True||False");
    }

    #[test]
    fn preserves_pipe_like_text_inside_collections() {
        assert_eq!(
            strip_expected_tags("[true, false] | {x: 'a|b'}"),
            "[True,False]|{x:a|b}"
        );
    }

    #[test]
    fn kuzu_and_tck_node_renders_canonicalize_identically() {
        assert_eq!(
            strip_expected_tags("{_ID: 0:0, _LABEL: End, num: 42, id: 0}"),
            strip_expected_tags("(:End {id: 0, num: 42})"),
        );
    }

    #[test]
    fn synthetic_row_column_is_dropped_from_node_renders() {
        assert_eq!(
            strip_expected_tags("{_ID: 0:0, _LABEL: A, __row: 0}"),
            strip_expected_tags("(:A)"),
        );
    }

    #[test]
    fn rel_renders_canonicalize_identically() {
        assert_eq!(
            strip_expected_tags("(0:0)-{_LABEL: KNOWS, _ID: 2:0, since: 2020}->(0:1)"),
            strip_expected_tags("[:KNOWS {since: 2020}]"),
        );
    }
}
