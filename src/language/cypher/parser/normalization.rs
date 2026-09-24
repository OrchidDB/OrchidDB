//! Syntax extension normalization before the generated Cypher parser runs.

mod operators;
use operators::{
    normalize_bitwise_operators, normalize_not_string_predicates, normalize_postfix_factorial,
    normalize_regex_match_operator, normalize_spaced_unary_signs,
};
mod functions;
use functions::{normalize_keyword_function_names, normalize_named_function_args};
mod lists;
use lists::{
    normalize_colon_slices, normalize_elided_list_elements, normalize_lambda_list_functions,
};

pub(super) fn normalize_cypher_extensions(input: &str) -> String {
    let (protected, identifiers) = protect_escaped_identifiers(input);
    let normalized = normalize_count_subqueries(&protected);
    let normalized = normalize_named_function_args(&normalized);
    let normalized = normalize_keyword_function_names(&normalized);
    let normalized = normalize_not_string_predicates(&normalized);
    let normalized = normalize_regex_match_operator(&normalized);
    let normalized = normalize_lambda_list_functions(&normalized);
    let normalized = normalize_spaced_unary_signs(&normalized);
    let normalized = normalize_postfix_factorial(&normalized);
    let normalized = normalize_bitwise_operators(&normalized);
    let normalized = normalize_elided_list_elements(&normalized);
    let mut normalized = normalize_colon_slices(&normalized);
    for (placeholder, original) in identifiers {
        normalized = normalized.replace(&placeholder, &original);
    }
    normalized
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
mod tests {
    use super::normalize_cypher_extensions;

    #[test]
    fn escaped_function_identifiers_survive_all_extension_passes() {
        for name in [
            "&&",
            "|",
            "<<",
            "!",
            "contains",
            "count { x }",
            "list_transform(x, x->x)",
            "a[1:2]",
            "x := y",
            "a``&&b",
            "__graph_escaped_identifier_0",
        ] {
            let input = format!("RETURN `{name}`(NULL, NULL)");
            assert_eq!(normalize_cypher_extensions(&input), input);
        }
        let input = "RETURN 'literal `&&`', `a[1:2]`(NULL), 1 & 2";
        let normalized = normalize_cypher_extensions(input);
        assert!(normalized.contains("'literal `&&`'"), "{normalized}");
        assert!(normalized.contains("`a[1:2]`(NULL)"), "{normalized}");
        assert!(normalized.contains("bitwise_and(1, 2)"), "{normalized}");
    }
}
