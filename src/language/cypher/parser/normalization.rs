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
    let normalized = normalize_count_subqueries(input);
    let normalized = normalize_named_function_args(&normalized);
    let normalized = normalize_keyword_function_names(&normalized);
    let normalized = normalize_not_string_predicates(&normalized);
    let normalized = normalize_regex_match_operator(&normalized);
    let normalized = normalize_lambda_list_functions(&normalized);
    let normalized = normalize_spaced_unary_signs(&normalized);
    let normalized = normalize_postfix_factorial(&normalized);
    let normalized = normalize_bitwise_operators(&normalized);
    let normalized = normalize_elided_list_elements(&normalized);
    normalize_colon_slices(&normalized)
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
