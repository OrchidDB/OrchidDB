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

use arrow::array::{Array, BooleanArray, Float64Array, Int64Array, StringArray};
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
        DataType::Float64 => render_float(
            array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(row),
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
    let trimmed = cell.trim();
    if trimmed.eq_ignore_ascii_case("null") {
        return String::new();
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return "True".to_string();
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return "False".to_string();
    }
    trimmed.to_string()
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
            "[true, false]|{x: 'a|b'}"
        );
    }
}
