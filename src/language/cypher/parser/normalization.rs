//! Syntax extension normalization before the generated Cypher parser runs.

mod json;
mod operators;
use operators::{
    normalize_bitwise_operators, normalize_postfix_factorial,
    normalize_regex_match_operator, normalize_spaced_unary_signs,
};
mod functions;
use functions::{normalize_keyword_function_names, normalize_named_function_args};
mod lists;
use lists::{normalize_colon_slices, normalize_lambda_list_functions};

pub(super) fn normalize_cypher_extensions(input: &str) -> String {
    let input = without_comments(input);
    let (protected, identifiers) = protect_escaped_identifiers(&input);
    let normalized = json::normalize_json(&protected);
    let normalized = normalize_count_subqueries(&normalized);
    let normalized = normalize_named_function_args(&normalized);
    let normalized = normalize_keyword_function_names(&normalized);
    let normalized = normalize_lambda_list_functions(&normalized);
    // Remove lambda arrows before scanning regex operands: the arrow's left
    // parameter is not part of the regex's left-hand expression.
    let normalized = normalize_regex_match_operator(&normalized);
    let normalized = normalize_spaced_unary_signs(&normalized);
    let normalized = normalize_postfix_factorial(&normalized);
    let normalized = normalize_bitwise_operators(&normalized);
    let mut normalized = normalize_colon_slices(&normalized);
    for (placeholder, original) in identifiers {
        normalized = normalized.replace(&placeholder, &original);
    }
    normalize_limit_parameter(&normalized)
}

// Extension syntax inside comments is not executable Cypher. Retain line
// breaks, and leave unterminated comments for the lexer to diagnose.
fn without_comments(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'\'' | b'"' | b'`') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' { i = (i + 2).min(bytes.len()); }
                else if bytes[i] == quote { i += 1; break; }
                else { i += 1; }
            }
        } else if bytes[i..].starts_with(b"//") || bytes[i..].starts_with(b"/*") {
            let start = i;
            if bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
            } else {
                let Some(end) = input[i + 2..].find("*/") else { break };
                i += end + 4;
            }
            out.push_str(&input[copied..start]);
            out.extend(input[start..i].chars().map(|ch| if matches!(ch, '\r' | '\n') { ch } else { ' ' }));
            copied = i;
        } else { i += 1; }
    }
    out.push_str(&input[copied..]);
    out
}

/// Extension normalizers operate on source text. Hide escaped identifier
/// tokens while they run, so operator-looking engine function names and
/// punctuation inside property names cannot be interpreted as expressions.
/// Placeholders keep their backticks and cannot collide with user source.
fn protect_escaped_identifiers(input: &str) -> (String, Vec<(String, String)>) {
    let mut prefix = "__graph_escaped_identifier_".to_owned();
    while input.contains(&prefix) {
        prefix.push('_');
    }
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut identifiers = Vec::new();
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            b'`' => {
                let start = i;
                i += 1;
                let mut closed = false;
                while i < bytes.len() {
                    if bytes[i] == b'`' {
                        if bytes.get(i + 1) == Some(&b'`') {
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else {
                        i += 1;
                    }
                }
                if closed {
                    let placeholder = format!("`{prefix}{}`", identifiers.len());
                    output.push_str(&input[copied..start]);
                    output.push_str(&placeholder);
                    identifiers.push((placeholder, input[start..i].to_owned()));
                    copied = i;
                }
            }
            _ => i += 1,
        }
    }
    output.push_str(&input[copied..]);
    (output, identifiers)
}

/// Rewrites `COUNT { ... }` subqueries into
/// `count_subquery(EXISTS { ... })` so the existing existential-subquery
/// grammar production can parse the body; the planner recognizes the
/// wrapper and projects the row count instead of a boolean.
pub(super) fn normalize_count_subqueries(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut copied = 0usize;
    let mut i = 0usize;
    let mut in_quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(quote) = in_quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == quote {
                in_quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'\'' || b == b'"' {
            in_quote = Some(b);
            i += 1;
            continue;
        }
        let is_count = bytes.len() - i >= 5
            && input.is_char_boundary(i)
            && input.is_char_boundary(i + 5)
            && input[i..i + 5].eq_ignore_ascii_case("count")
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_');
        if is_count {
            let mut j = i + 5;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'{' {
                // Find the matching close brace, skipping strings.
                let mut depth = 0i32;
                let mut k = j;
                let mut body_quote: Option<u8> = None;
                let mut end = None;
                while k < bytes.len() {
                    let c = bytes[k];
                    if let Some(quote) = body_quote {
                        if c == b'\\' {
                            k += 2;
                            continue;
                        }
                        if c == quote {
                            body_quote = None;
                        }
                        k += 1;
                        continue;
                    }
                    match c {
                        b'\'' | b'"' => body_quote = Some(c),
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(k);
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                if let Some(end) = end {
                    out.push_str(&input[copied..i]);
                    let body = normalize_count_subqueries(&input[j..=end]);
                    out.push_str("count_subquery(EXISTS ");
                    out.push_str(&body);
                    out.push(')');
                    i = end + 1;
                    copied = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    out.push_str(&input[copied..]);
    out
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/language__cypher__parser__normalization__tests.rs"]
mod tests;

fn normalize_limit_parameter(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        if bytes.get(i) == Some(&quote) { i += 1; } else { break; }
                    } else { i += 1; }
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" { i += 1; }
                i = (i + 2).min(bytes.len());
            }
            b'$' => {
                let start = i + 1;
                i = start;
                while i < bytes.len() {
                    let ch = input[i..].chars().next().unwrap();
                    if ch.is_alphanumeric() || ch == '_' { i += ch.len_utf8(); } else { break; }
                }
                if input[start..i].eq_ignore_ascii_case("limit") {
                    output.push_str(&input[copied..start]);
                    output.push('`');
                    output.push_str(&input[start..i]);
                    output.push('`');
                    copied = i;
                }
            }
            _ => i += 1,
        }
    }
    output.push_str(&input[copied..]);
    output
}
