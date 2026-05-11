//! Case-file parser. Each `.case` file has three required sections:
//!
//! ```text
//! --- metadata
//! { ...flat JSON object... }
//! --- query
//! <gremlin source, possibly multi-line>
//! --- graph_initializer
//! <optional Gremlin graph construction source>
//! --- expected
//! <expected output, one row per line; may be empty>
//! ```
//!
//! The metadata is a small flat JSON object, so we hand-parse it instead
//! of pulling in `serde_json`.

use std::fmt;

#[derive(Debug)]
pub struct Case {
    pub metadata: Metadata,
    pub query: String,
    pub graph_initializer: Option<String>,
    pub expected: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Metadata {
    pub dataset: String,
    pub expected_kind: String,
    pub id: String,
    pub language: String,
    pub ordered: bool,
    pub source: String,
    pub source_case: String,
    pub status: String,
    pub suite: String,
    pub version: i64,
}

#[derive(Debug)]
pub enum CaseError {
    MissingSection(&'static str),
    Json(String),
}

impl fmt::Display for CaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaseError::MissingSection(s) => write!(f, "missing `--- {s}` section"),
            CaseError::Json(msg) => write!(f, "metadata json: {msg}"),
        }
    }
}

pub fn parse(raw: &str) -> Result<Case, CaseError> {
    let mut metadata: Option<String> = None;
    let mut query: Option<String> = None;
    let mut graph_initializer: Option<String> = None;
    let mut expected: Option<Vec<String>> = None;

    let mut current_section: Option<&'static str> = None;
    let mut current_buf: Vec<String> = Vec::new();

    fn flush(
        section: Option<&'static str>,
        buf: Vec<String>,
        metadata: &mut Option<String>,
        query: &mut Option<String>,
        graph_initializer: &mut Option<String>,
        expected: &mut Option<Vec<String>>,
    ) {
        match section {
            Some("metadata") => {
                *metadata = Some(buf.join("\n"));
            }
            Some("query") => {
                *query = Some(buf.join("\n").trim().to_string());
            }
            Some("graph_initializer") => {
                *graph_initializer = Some(buf.join("\n").trim().to_string());
            }
            Some("expected") => {
                *expected = Some(buf);
            }
            _ => {}
        }
    }

    for line in raw.lines() {
        if let Some(stripped) = line.strip_prefix("--- ") {
            flush(
                current_section,
                std::mem::take(&mut current_buf),
                &mut metadata,
                &mut query,
                &mut graph_initializer,
                &mut expected,
            );
            current_section = Some(match stripped.trim() {
                "metadata" => "metadata",
                "query" => "query",
                "graph_initializer" => "graph_initializer",
                "expected" => "expected",
                _ => "?",
            });
        } else {
            current_buf.push(line.to_string());
        }
    }
    flush(
        current_section,
        current_buf,
        &mut metadata,
        &mut query,
        &mut graph_initializer,
        &mut expected,
    );

    let metadata_str = metadata.ok_or(CaseError::MissingSection("metadata"))?;
    let query_str = query.ok_or(CaseError::MissingSection("query"))?;
    let mut expected_lines = expected.ok_or(CaseError::MissingSection("expected"))?;
    while expected_lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        expected_lines.pop();
    }

    let metadata = parse_metadata(metadata_str.trim()).map_err(CaseError::Json)?;

    Ok(Case {
        metadata,
        query: query_str,
        graph_initializer,
        expected: expected_lines,
    })
}

/// Parse a single-line, flat JSON object with string / bool / int values.
/// We don't pull in serde_json — the metadata schema is tiny and stable.
fn parse_metadata(raw: &str) -> Result<Metadata, String> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| format!("expected `{{...}}`, got `{trimmed}`"))?;

    let mut metadata = Metadata::default();
    let pairs = split_json_pairs(inner);
    for pair in pairs {
        let (k, v) = split_kv(&pair)?;
        let key = unquote(k.trim())?;
        let value = v.trim();
        match key.as_str() {
            "dataset" => metadata.dataset = unquote(value)?,
            "expected_kind" => metadata.expected_kind = unquote(value)?,
            "id" => metadata.id = unquote(value)?,
            "language" => metadata.language = unquote(value)?,
            "ordered" => {
                metadata.ordered = value == "true";
            }
            "source" => metadata.source = unquote(value)?,
            "source_case" => metadata.source_case = unquote(value)?,
            "status" => metadata.status = unquote(value)?,
            "suite" => metadata.suite = unquote(value)?,
            "version" => {
                metadata.version = value.parse::<i64>().map_err(|e| e.to_string())?;
            }
            _ => {
                // Unknown key — ignore so future schema additions don't
                // break the runner.
            }
        }
    }
    Ok(metadata)
}

/// Split a flat JSON object body on top-level commas. Doesn't handle
/// nested arrays/objects, which the metadata schema doesn't use.
fn split_json_pairs(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_string = false;
    let mut escape = false;
    for ch in body.chars() {
        if in_string {
            buf.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                buf.push(ch);
            }
            ',' => {
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

fn split_kv(pair: &str) -> Result<(&str, &str), String> {
    // Find the first ':' outside a quoted string.
    let mut depth_string = false;
    let mut escape = false;
    for (idx, ch) in pair.char_indices() {
        if depth_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                depth_string = false;
            }
            continue;
        }
        match ch {
            '"' => depth_string = true,
            ':' => return Ok((&pair[..idx], &pair[idx + 1..])),
            _ => {}
        }
    }
    Err(format!("missing `:` in pair `{pair}`"))
}

fn unquote(token: &str) -> Result<String, String> {
    let t = token.trim();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        let body = &t[1..t.len() - 1];
        let mut out = String::with_capacity(body.len());
        let mut chars = body.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => out.push(other),
                    None => break,
                }
            } else {
                out.push(ch);
            }
        }
        Ok(out)
    } else {
        Ok(t.to_string())
    }
}
