//! String and regular expression functions.

use crate::ir::interpreter::{InterpretError, IrResult};
use crate::ir::value::Value;
use super::casts::cast_to_string;

pub(super) fn runtime_initcap(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first
        .to_uppercase()
        .chain(chars.flat_map(char::to_lowercase))
        .collect()
}

pub(super) fn runtime_split_string(text: &str, delimiter: &str) -> Vec<Value> {
    if delimiter.is_empty() {
        text.chars()
            .map(|ch| Value::String(ch.to_string()))
            .collect()
    } else {
        text.split(delimiter)
            .map(|part| Value::String(part.to_string()))
            .collect()
    }
}

pub(super) fn runtime_split_part(text: &str, delimiter: &str, index: i64) -> Value {
    if index <= 0 {
        return Value::String(String::new());
    }
    if delimiter.is_empty() {
        return text
            .chars()
            .nth((index - 1) as usize)
            .map(|ch| Value::String(ch.to_string()))
            .unwrap_or_else(|| Value::String(String::new()));
    }
    text.split(delimiter)
        .nth((index - 1) as usize)
        .map(|part| Value::String(part.to_string()))
        .unwrap_or_else(|| Value::String(String::new()))
}

pub(super) fn compile_regex(pattern: &str) -> IrResult<regex::Regex> {
    regex::Regex::new(pattern)
        .map_err(|err| InterpretError::Runtime(format!("Invalid Input Error: {err}")))
}

pub(super) fn regex_full_match(text: &str, pattern: &str) -> IrResult<bool> {
    let regex = compile_regex(&format!("^(?:{pattern})$"))?;
    Ok(regex.is_match(text))
}

pub(super) fn regexp_extract(text: &str, pattern: &str, group: i64) -> IrResult<Value> {
    let regex = compile_regex(pattern)?;
    let Some(captures) = regex.captures(text) else {
        return Ok(Value::String(String::new()));
    };
    let index = group.max(0) as usize;
    Ok(captures
        .get(index)
        .map(|matched| Value::String(matched.as_str().to_string()))
        .unwrap_or_else(|| Value::String(String::new())))
}

pub(super) fn regexp_extract_all(text: &str, pattern: &str, group: i64) -> IrResult<Value> {
    let regex = compile_regex(pattern)?;
    let index = group.max(0) as usize;
    Ok(Value::List(
        regex
            .captures_iter(text)
            .map(|captures| {
                captures
                    .get(index)
                    .map(|matched| Value::String(matched.as_str().to_string()))
                    .unwrap_or_else(|| Value::String(String::new()))
            })
            .collect(),
    ))
}

/// Right-pad (or left-pad with `to_right=false`) `s` to length `len`
/// using `pad`. If `s` is already that long the truncated head is
/// returned. Matches Kuzu's `rpad(str, len, pad)` and `lpad(...)`.
pub(super) fn pad_string(s: &str, len: i64, pad: &str, to_right: bool) -> String {
    if len < 0 {
        return s.to_string();
    }
    let len = len as usize;
    let current: Vec<char> = s.chars().collect();
    if current.len() >= len {
        return current.into_iter().take(len).collect();
    }
    let pad_chars: Vec<char> = pad.chars().collect();
    if pad_chars.is_empty() {
        return s.to_string();
    }
    let missing = len - current.len();
    let mut padding = String::with_capacity(missing);
    for i in 0..missing {
        padding.push(pad_chars[i % pad_chars.len()]);
    }
    if to_right {
        format!("{s}{padding}")
    } else {
        format!("{padding}{s}")
    }
}

/// Wagner–Fischer Levenshtein distance over UTF-8 characters. Used as
/// a cheap stand-in for the Kuzu `levenshtein(left, right)` macro the
/// conformance corpus invokes.
pub(super) fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

pub(super) fn string_function_value(value: &Value) -> Value {
    match value {
        // Kuzu capitalises booleans (`string(true)` -> `"True"`) and pads
        // floating-point values to six decimals (`string(11.7)` ->
        // `"11.700000"`). The generic cast_to_string keeps Rust's display
        // form for Gremlin output, so Cypher/Kuzu string coercion stays here.
        Value::Bool(true) => Value::String("True".to_string()),
        Value::Bool(false) => Value::String("False".to_string()),
        Value::Float(value) if value.is_finite() => Value::String(format!("{value:.6}")),
        Value::Float32(value) if value.is_finite() => {
            Value::String(format!("{:.6}", *value as f64))
        }
        other => cast_to_string(other),
    }
}

pub(super) fn left_string_value(text: &str, length: &Value) -> Value {
    length
        .as_i64()
        .map(|length| {
            let chars = unicode_segmentation::UnicodeSegmentation::graphemes(text, true)
                .collect::<Vec<_>>();
            let take = if length < 0 {
                chars.len().saturating_sub(length.unsigned_abs() as usize)
            } else {
                length as usize
            };
            Value::String(chars.into_iter().take(take).collect())
        })
        .unwrap_or(Value::Null)
}

pub(super) fn string_index_1_based(text: &str, index: i64) -> Value {
    if index == 0 {
        return Value::Null;
    }
    let chars =
        unicode_segmentation::UnicodeSegmentation::graphemes(text, true).collect::<Vec<_>>();
    if chars.is_empty() {
        return Value::Null;
    }
    let zero_based = if index < 0 {
        (chars.len() as i64 + index).max(0)
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= chars.len() as i64 {
        Value::Null
    } else {
        Value::String(chars[zero_based as usize].to_string())
    }
}

pub(super) fn string_index_1_based_clamped(text: &str, index: i64) -> Value {
    if index == 0 {
        return Value::Null;
    }
    let chars =
        unicode_segmentation::UnicodeSegmentation::graphemes(text, true).collect::<Vec<_>>();
    if chars.is_empty() {
        return Value::Null;
    }
    let zero_based = if index < 0 {
        (chars.len() as i64 + index).max(0)
    } else {
        (index - 1).min(chars.len() as i64 - 1)
    };
    Value::String(chars[zero_based as usize].to_string())
}

pub(super) fn string_index(text: &str, index: i64) -> Value {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len() as i64;
    if len == 0 {
        return Value::Null;
    }
    let i = if index < 0 { len + index } else { index };
    if i < 0 || i >= len {
        Value::Null
    } else {
        Value::String(chars[i as usize].to_string())
    }
}
