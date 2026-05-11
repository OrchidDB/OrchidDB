//! Output comparison.
//!
//! TinkerPop case files specify an `ordered` flag in metadata:
//!   - `ordered=true`  → sequence equality.
//!   - `ordered=false` → multiset equality.
//!
//! Each side is normalized first: the actual lines come from
//! `format::lines_from_batch`, the expected lines have any TinkerPop
//! tag prefix stripped so we can match plain scalars cleanly.

use crate::format::strip_expected_tags;

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

    let normalized_actual: Vec<String> = actual
        .iter()
        .map(|s| normalize_numbers(&strip_expected_tags(s).trim().to_string()))
        .collect();
    let normalized_expected: Vec<String> = expected
        .iter()
        .map(|s| normalize_numbers(&strip_expected_tags(s).trim().to_string()))
        .filter(|s| !s.is_empty() || !expected.is_empty()) // keep empties for empty-expected cases
        .collect();

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
