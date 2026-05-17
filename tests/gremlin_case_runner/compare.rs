//! Output comparison.
//!
//! TinkerPop case files specify an `ordered` flag in metadata:
//!   - `ordered=true`  → sequence equality.
//!   - `ordered=false` → multiset equality.
//!
//! Each side is normalized first: the actual lines come from
//! `format::lines_from_batch`, the expected lines have any TinkerPop
//! tag prefix stripped so we can match plain scalars cleanly.

use crate::format::{ignore_unrepresented_empty_rows, strip_expected_tags};

/// Treat numeric literals as equal regardless of trailing-zero / decimal
/// formatting (`1049.0` ≡ `1049`, `1.50` ≡ `1.5`). Applied to both sides
/// before equality so the comparison is symmetric. Operates on numeric
/// runs in-place — non-numeric characters are passed through.
fn normalize_numbers(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            i += 1;
        }
        if bytes[i].is_ascii_digit() {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len()
                && bytes[i] == b'.'
                && i + 1 < bytes.len()
                && bytes[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push_str(&canonicalize_number(&input[start..i]));
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn canonicalize_number(token: &str) -> String {
    if !token.contains('.') {
        return token.to_string();
    }
    let trimmed = token.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug)]
pub enum Verdict {
    Match,
    Mismatch { reason: String },
}

pub fn matches(
    actual: &[String],
    expected: &[String],
    ordered: bool,
    expected_kind: &str,
) -> Verdict {
    if expected_kind == "count" {
        let expected_count = expected
            .iter()
            .find_map(|line| line.trim().parse::<usize>().ok());
        return match expected_count {
            Some(n) if actual.len() == n => Verdict::Match,
            Some(n) => Verdict::Mismatch {
                reason: format!("row count: actual {}, expected {n}", actual.len()),
            },
            None => Verdict::Mismatch {
                reason: "count expectation did not contain a numeric count".to_string(),
            },
        };
    }

    let set_shaped_expected = expected
        .iter()
        .any(|line| line.trim_start().starts_with("s["));
    let normalized_actual: Vec<String> = actual
        .iter()
        .map(|s| {
            let line = normalize_numbers(&strip_expected_tags(s).trim().to_string());
            if set_shaped_expected {
                canonicalize_set_items(&line)
            } else {
                line
            }
        })
        .collect();
    let normalized_expected: Vec<String> = expected
        .iter()
        .map(|s| {
            let line = normalize_numbers(&strip_expected_tags(s).trim().to_string());
            if set_shaped_expected {
                canonicalize_set_items(&line)
            } else {
                line
            }
        })
        .filter(|s| !s.is_empty() || !expected.is_empty()) // keep empties for empty-expected cases
        .collect();
    let normalized_actual = if normalized_actual.len() == 1 && normalized_expected.len() > 1 {
        let expanded = split_top_level_items(&normalized_actual[0])
            .into_iter()
            .map(|s| normalize_numbers(&strip_expected_tags(s.trim()).trim().to_string()))
            .collect::<Vec<_>>();
        if expanded.len() == normalized_expected.len()
            && rows_match(&expanded, &normalized_expected, ordered)
        {
            expanded
        } else {
            normalized_actual
        }
    } else {
        normalized_actual
    };

    let normalized_actual = if ignore_unrepresented_empty_rows()
        && !normalized_actual.is_empty()
        && !normalized_expected.iter().any(|line| line.is_empty())
    {
        let compact = normalized_actual
            .iter()
            .filter(|line| !line.is_empty())
            .cloned()
            .collect::<Vec<_>>();
        if compact.len() == normalized_expected.len()
            && rows_match(&compact, &normalized_expected, ordered)
        {
            compact
        } else {
            normalized_actual
        }
    } else {
        normalized_actual
    };

    if normalized_actual.len() != normalized_expected.len() {
        return Verdict::Mismatch {
            reason: format!(
                "row count: actual {}, expected {}",
                normalized_actual.len(),
                normalized_expected.len()
            ),
        };
    }

    if ordered {
        for (idx, (a, e)) in normalized_actual
            .iter()
            .zip(&normalized_expected)
            .enumerate()
        {
            if a != e {
                return Verdict::Mismatch {
                    reason: format!("row {idx}: `{a}` != `{e}`"),
                };
            }
        }
        Verdict::Match
    } else {
        let mut a_sorted = normalized_actual.clone();
        let mut e_sorted = normalized_expected.clone();
        a_sorted.sort();
        e_sorted.sort();
        if a_sorted == e_sorted {
            Verdict::Match
        } else {
            Verdict::Mismatch {
                reason: format!(
                    "multiset mismatch: actual={:?}, expected={:?}",
                    a_sorted, e_sorted
                ),
            }
        }
    }
}

fn split_top_level_items(input: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '[' | '{' | '(' if !in_string => depth += 1,
            ']' | '}' | ')' if !in_string => depth -= 1,
            ',' if !in_string && depth == 0 => {
                out.push(input[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start == 0 {
        return Vec::new();
    }
    out.push(input[start..].trim());
    out
}

fn rows_match(actual: &[String], expected: &[String], ordered: bool) -> bool {
    if ordered {
        return actual == expected;
    }
    let mut a_sorted = actual.to_vec();
    let mut e_sorted = expected.to_vec();
    a_sorted.sort();
    e_sorted.sort();
    a_sorted == e_sorted
}

fn canonicalize_set_items(line: &str) -> String {
    if line.starts_with("m[") || line.starts_with("p[") || !line.contains(',') {
        return line.to_string();
    }
    let mut items = split_top_level_items(line)
        .into_iter()
        .map(|item| item.trim().to_string())
        .collect::<Vec<_>>();
    if items.len() <= 1 {
        return line.to_string();
    }
    items.sort();
    items.join(",")
}
