//! Lambda, elided element, and slice syntax rewrites.

use super::functions::is_identifier_continue;
pub(super) fn normalize_lambda_list_functions(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        if let Some(q) = quote {
            out.push(ch);
            index += ch.len_utf8();
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        if let Some((replacement, end)) = rewrite_lambda_function_at(input, index) {
            out.push_str(&replacement);
            index = end;
            continue;
        }
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

pub(super) fn rewrite_lambda_function_at(input: &str, index: usize) -> Option<(String, usize)> {
    let (name, after_name) = ["list_transform", "list_filter", "list_reduce"]
        .into_iter()
        .find_map(|name| match_keyword_at(input, index, name).map(|after| (name, after)))?;
    let cursor = skip_space(input, after_name);
    if input[cursor..].chars().next()? != '(' {
        return None;
    }
    let end = find_matching(input, cursor, '(', ')')?;
    let args = split_top_level_args(&input[cursor + 1..end]);
    let replacement = match name {
        "list_transform" => {
            if args.len() != 2 {
                return None;
            }
            let (variable, body) = split_single_lambda(args[1])?;
            format!(
                "__list_transform({}, '{}', {})",
                normalize_lambda_list_functions(args[0].trim()),
                variable,
                normalize_lambda_list_functions(body.trim())
            )
        }
        "list_filter" => {
            if args.len() != 2 {
                return None;
            }
            let (variable, body) = split_single_lambda(args[1])?;
            format!(
                "__list_filter({}, '{}', {})",
                normalize_lambda_list_functions(args[0].trim()),
                variable,
                normalize_lambda_list_functions(body.trim())
            )
        }
        "list_reduce" => {
            if args.len() != 2 {
                return None;
            }
            let (accumulator, variable, body) = split_reduce_lambda(args[1])?;
            format!(
                "__list_reduce({}, '{}', '{}', {})",
                normalize_lambda_list_functions(args[0].trim()),
                accumulator,
                variable,
                normalize_lambda_list_functions(body.trim())
            )
        }
        _ => return None,
    };
    Some((replacement, end + 1))
}

pub(super) fn match_keyword_at(input: &str, index: usize, keyword: &str) -> Option<usize> {
    if index > 0 {
        let prev = input[..index].chars().next_back()?;
        if is_identifier_continue(prev) {
            return None;
        }
    }
    let end = index.checked_add(keyword.len())?;
    let candidate = input.get(index..end)?;
    if !candidate.eq_ignore_ascii_case(keyword) {
        return None;
    }
    if end < input.len() {
        let next = input[end..].chars().next()?;
        if is_identifier_continue(next) {
            return None;
        }
    }
    Some(end)
}

pub(super) fn skip_space(input: &str, mut index: usize) -> usize {
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

pub(super) fn find_matching(
    input: &str,
    open_index: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (offset, ch) in input[open_index..].char_indices() {
        let index = open_index + offset;
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn split_top_level_args(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            ',' if paren == 0 && bracket == 0 && brace == 0 => {
                parts.push(input[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(input[start..].trim());
    parts
}

pub(super) fn split_single_lambda(input: &str) -> Option<(String, &str)> {
    let arrow = top_level_arrow(input)?;
    let variable = strip_wrapping_parens(input[..arrow].trim()).trim();
    if variable.is_empty() || variable.contains(',') {
        return None;
    }
    Some((variable.to_string(), &input[arrow + 2..]))
}

pub(super) fn split_reduce_lambda(input: &str) -> Option<(String, String, &str)> {
    let arrow = top_level_arrow(input)?;
    let params = split_top_level_args(strip_wrapping_parens(input[..arrow].trim()));
    if params.len() != 2 {
        return None;
    }
    Some((
        params[0].trim().to_string(),
        params[1].trim().to_string(),
        &input[arrow + 2..],
    ))
}

pub(super) fn strip_wrapping_parens(input: &str) -> &str {
    let trimmed = input.trim();
    if trimmed.starts_with('(')
        && trimmed.ends_with(')')
        && find_matching(trimmed, 0, '(', ')') == Some(trimmed.len() - 1)
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

pub(super) fn normalize_elided_list_elements(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        if let Some(q) = quote {
            out.push(ch);
            index += ch.len_utf8();
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        if ch == '[' && is_likely_list_literal(input, index) {
            if let Some(end) = find_matching(input, index, '[', ']') {
                let inner = &input[index + 1..end];
                if let Some(rewritten) = rewrite_elided_list_elements(inner) {
                    out.push('[');
                    out.push_str(&rewritten);
                    out.push(']');
                    index = end + 1;
                    continue;
                }
            }
        }
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

pub(super) fn is_likely_list_literal(input: &str, open_index: usize) -> bool {
    if input[..open_index]
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_whitespace())
    {
        return true;
    }
    let Some(previous) = input[..open_index]
        .chars()
        .rev()
        .find(|ch| !ch.is_whitespace())
    else {
        return true;
    };
    !matches!(previous, ')' | ']' | '\'' | '"') && !is_identifier_continue(previous)
}

pub(super) fn rewrite_elided_list_elements(input: &str) -> Option<String> {
    if input.trim().is_empty() {
        return None;
    }
    let parts = split_top_level_args(input);
    if parts.iter().all(|part| !part.is_empty()) {
        return None;
    }
    Some(
        parts
            .into_iter()
            .map(|part| {
                if part.is_empty() {
                    "NULL".to_string()
                } else {
                    normalize_elided_list_elements(part)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
    )
}

pub(super) fn top_level_arrow(input: &str) -> Option<usize> {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            '-' if paren == 0
                && bracket == 0
                && brace == 0
                && input[index + ch.len_utf8()..].starts_with('>') =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

pub(super) fn normalize_colon_slices(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        if let Some(q) = quote {
            out.push(ch);
            index += ch.len_utf8();
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        if ch == '[' {
            if let Some(end) = find_matching(input, index, '[', ']') {
                let inner = &input[index + 1..end];
                if let Some(colon) = top_level_colon_slice(inner) {
                    out.push('[');
                    out.push_str(&normalize_colon_slices(&inner[..colon]));
                    out.push_str("..");
                    out.push_str(&normalize_colon_slices(&inner[colon + 1..]));
                    out.push(']');
                    index = end + 1;
                    continue;
                }
            }
        }
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

pub(super) fn top_level_colon_slice(input: &str) -> Option<usize> {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut colon = None;
    for (index, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            ':' if paren == 0 && bracket == 0 && brace == 0 => {
                if colon.replace(index).is_some() {
                    return None;
                }
            }
            '.' if paren == 0 && bracket == 0 && brace == 0 && input[index..].starts_with("..") => {
                return None;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let left = input[..colon].trim();
    let right = input[colon + 1..].trim();
    let right_starts_name = right
        .chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic());
    if (left.is_empty()
        || left
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric()))
        && right_starts_name
    {
        return None;
    }
    Some(colon)
}
