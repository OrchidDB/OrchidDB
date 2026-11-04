//! Operator rewrites and their quote-aware scanning helpers.

use super::lists::skip_space;

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

pub(super) fn normalize_spaced_unary_signs(input: &str) -> String {
    if !(input.contains('-') || input.contains('+')) {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let (_, ch) = chars[idx];
        if let Some(q) = quote {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            idx += 1;
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                out.push(ch);
                idx += 1;
            }
            '+' | '-' if unary_sign_context(&out) && !exponent_sign_context(&out) => {
                let mut minus_count = 0usize;
                let mut next_idx = idx;
                loop {
                    let Some((_, sign)) = chars.get(next_idx).copied() else {
                        break;
                    };
                    if !matches!(sign, '+' | '-') {
                        break;
                    }
                    if sign == '-' {
                        minus_count += 1;
                    }
                    next_idx += 1;
                    while chars
                        .get(next_idx)
                        .is_some_and(|(_, next)| next.is_whitespace())
                    {
                        next_idx += 1;
                    }
                }
                if minus_count % 2 == 1 {
                    out.push('-');
                }
                idx = next_idx;
            }
            _ => {
                out.push(ch);
                idx += 1;
            }
        }
    }
    out
}

// A sign after a numeric exponent marker belongs to the literal, not a unary
// expression. Leave malformed sign sequences intact for the lexer to reject:
// collapsing `1e++2` to `1e+2` would silently accept invalid input.
fn exponent_sign_context(prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches(|ch: char| ch.is_whitespace() || matches!(ch, '+' | '-'));
    let Some(mantissa_prefix) = prefix.strip_suffix(['e', 'E']) else { return false };
    let start = mantissa_prefix.char_indices().rev()
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '.')
        .last().map(|(index, _)| index).unwrap_or(mantissa_prefix.len());
    let mantissa = &mantissa_prefix[start..];
    !mantissa.is_empty()
        && mantissa.ends_with(|ch: char| ch.is_ascii_digit())
        && mantissa.bytes().filter(|ch| *ch == b'.').count() <= 1
        && mantissa_prefix[..start].chars().next_back().is_none_or(|ch| !is_ident_continue(ch))
}

pub(super) fn unary_sign_context(prefix: &str) -> bool {
    let trimmed = prefix.trim_end();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if ["return", "with", "where", "then", "else"]
        .iter()
        .any(|keyword| lower.ends_with(keyword))
    {
        return true;
    }
    trimmed
        .chars()
        .next_back()
        .is_some_and(|ch| matches!(ch, '(' | '[' | '{' | ',' | '+' | '-' | '*' | '/' | '%'))
}

pub(super) fn normalize_bitwise_operators(input: &str) -> String {
    if !contains_bitwise_operator(input) {
        return input.to_string();
    }

    let mut out = input.to_string();
    let mut search_from = 0;
    while let Some((_, expr_start)) = find_projection_keyword(&out, search_from) {
        let expr_end = expr_start + projection_body_len(&out[expr_start..]);
        let body = &out[expr_start..expr_end];
        let rewritten = rewrite_projection_bitwise(body);
        out.replace_range(expr_start..expr_end, &rewritten);
        search_from = expr_start + rewritten.len();
    }
    out
}

pub(super) fn contains_bitwise_operator(input: &str) -> bool {
    find_outside_quotes(input, |rest| {
        rest.starts_with('&') || rest.starts_with('|') || rest.starts_with("<<") || rest.starts_with(">>")
    })
    .is_some()
}

pub(super) fn find_projection_keyword(input: &str, start: usize) -> Option<(usize, usize)> {
    let mut offset = start;
    while offset < input.len() {
        let found = find_outside_quotes(&input[offset..], |rest| {
            keyword_at(rest, "RETURN") || keyword_at(rest, "WITH")
        })?;
        let keyword_start = offset + found;
        let rest = &input[keyword_start..];
        let keyword_len = if keyword_at(rest, "RETURN") {
            "RETURN".len()
        } else {
            "WITH".len()
        };
        let before_ok = keyword_start == 0
            || input[..keyword_start]
                .chars()
                .next_back()
                .is_some_and(|ch| !is_ident_continue(ch));
        if before_ok {
            let expr_start = keyword_start + keyword_len;
            return Some((keyword_start, skip_space(input, expr_start)));
        }
        offset = keyword_start + keyword_len;
    }
    None
}

pub(super) fn keyword_at(input: &str, keyword: &str) -> bool {
    let Some(prefix) = input.get(..keyword.len()) else {
        return false;
    };
    if !prefix.eq_ignore_ascii_case(keyword) {
        return false;
    }
    input[keyword.len()..]
        .chars()
        .next()
        .is_none_or(|ch| !is_ident_continue(ch))
}

pub(super) fn projection_body_len(input: &str) -> usize {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0i32;
    for (byte_idx, ch) in input.char_indices() {
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if depth == 0 && projection_clause_boundary(&input[byte_idx..]) => return byte_idx,
            _ => {}
        }
    }
    input.len()
}

pub(super) fn projection_clause_boundary(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        " order by ",
        " skip ",
        " limit ",
        " union ",
        " match ",
        " where ",
        " unwind ",
        " return ",
        " with ",
    ]
    .iter()
    .any(|needle| lower.starts_with(needle))
}

pub(super) fn rewrite_projection_bitwise(input: &str) -> String {
    split_top_level_commas(input)
        .into_iter()
        .map(|part| rewrite_bitwise_expr(part.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0i32;
    let mut start = 0;
    for (byte_idx, ch) in input.char_indices() {
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&input[start..byte_idx]);
                start = byte_idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

pub(super) fn rewrite_bitwise_expr(input: &str) -> String {
    let expr = input.trim();
    if let Some(idx) = top_level_token(expr, |rest| keyword_at(rest, "AS")) {
        return format!("{} {}", rewrite_bitwise_expr(&expr[..idx]), &expr[idx..]);
    }
    if let Some((inner_start, inner_end)) = enclosing_parens(expr) {
        let inner = rewrite_bitwise_expr(&expr[inner_start..inner_end]);
        return format!("({inner})");
    }
    if let Some((idx, op_len, name)) = find_top_level_bitwise(expr, &["|"]) {
        return format!(
            "{name}({}, {})",
            rewrite_bitwise_expr(&expr[..idx]),
            rewrite_bitwise_expr(&expr[idx + op_len..])
        );
    }
    if let Some((idx, op_len, name)) = find_top_level_bitwise(expr, &["&"]) {
        return format!(
            "{name}({}, {})",
            rewrite_bitwise_expr(&expr[..idx]),
            rewrite_bitwise_expr(&expr[idx + op_len..])
        );
    }
    if let Some((idx, op_len, name)) = find_top_level_bitwise(expr, &["<<", ">>"]) {
        return format!(
            "{name}({}, {})",
            rewrite_bitwise_expr(&expr[..idx]),
            rewrite_bitwise_expr(&expr[idx + op_len..])
        );
    }
    // Operators in function arguments and comprehensions are hidden from the
    // top-level precedence scan. Recurse into balanced, quote-aware groups.
    let mut output = String::new();
    let mut copied = 0;
    let mut offset = 0;
    while let Some(relative) = find_outside_quotes(&expr[offset..], |rest| {
        rest.starts_with('(') || rest.starts_with('[') || rest.starts_with('{')
    }) {
        let start = offset + relative;
        let open = expr[start..].chars().next().unwrap();
        let close = match open { '(' => ')', '[' => ']', _ => '}' };
        let Some(end) = super::lists::find_matching(expr, start, open, close) else { break };
        let body = &expr[start + 1..end];
        let pipe = top_level_token(body, |rest| rest.starts_with('|'));
        let pattern = open == '[' && (body.trim_start().starts_with('(')
            || body.split_once('=').is_some_and(|(name, rest)|
                name.trim().chars().all(is_ident_continue) && rest.trim_start().starts_with('(')));
        let comprehension = pipe.filter(|idx| {
            top_level_token(&body[..*idx], |rest| keyword_at(rest, "IN")).is_some()
                || pattern
        });
        let rewritten = if let Some(pipe) = comprehension {
            let prefix = if pattern {
                body[..pipe].trim().to_string()
            } else {
                rewrite_projection_bitwise(&body[..pipe])
            };
            format!("{prefix} | {}", rewrite_bitwise_expr(&body[pipe + 1..]))
        } else {
            rewrite_projection_bitwise(body)
        };
        output.push_str(&expr[copied..start + 1]);
        output.push_str(&rewritten);
        output.push(close);
        copied = end + 1;
        offset = copied;
    }
    output.push_str(&expr[copied..]);
    output
}

fn top_level_token(input: &str, matches: impl Fn(&str) -> bool) -> Option<usize> {
    let mut offset = 0;
    while let Some(relative) = find_outside_quotes(&input[offset..], |rest| {
        rest.starts_with('(') || rest.starts_with('[') || rest.starts_with('{') || matches(rest)
    }) {
        let idx = offset + relative;
        let ch = input[idx..].chars().next().unwrap();
        if matches!(ch, '(' | '[' | '{') {
            let close = match ch { '(' => ')', '[' => ']', _ => '}' };
            offset = super::lists::find_matching(input, idx, ch, close)? + 1;
        } else if keyword_start(input, idx) || !ch.is_ascii_alphabetic() {
            return Some(idx);
        } else {
            offset = idx + ch.len_utf8();
        }
    }
    None
}

pub(super) fn find_top_level_bitwise(
    input: &str,
    operators: &[&str],
) -> Option<(usize, usize, &'static str)> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0i32;
    let mut found = None;
    for (byte_idx, ch) in input.char_indices() {
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if depth == 0 => {
                for op in operators {
                    if input[byte_idx..].starts_with(op) {
                        let name = match *op {
                            "&" => "bitwise_and",
                            "|" => "bitwise_or",
                            "<<" => "bitshift_left",
                            ">>" => "bitshift_right",
                            _ => unreachable!(),
                        };
                        found = Some((byte_idx, op.len(), name));
                    }
                }
            }
            _ => {}
        }
    }
    found
}

pub(super) fn enclosing_parens(input: &str) -> Option<(usize, usize)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return None;
    }
    if super::lists::find_matching(trimmed, 0, '(', ')') == Some(trimmed.len() - 1) {
        Some((1, trimmed.len() - 1))
    } else {
        None
    }
}

pub(super) fn normalize_postfix_factorial(input: &str) -> String {
    if !input.contains('!') {
        return input.to_string();
    }

    let mut replacements = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    for (idx, (byte_idx, ch)) in chars.iter().copied().enumerate() {
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
            '!' => {
                if chars.get(idx + 1).is_some_and(|(_, next)| *next == '=') {
                    continue;
                }
                let Some((previous_idx, previous_ch)) = previous_non_space(input, byte_idx) else {
                    continue;
                };
                let (start_idx, inner) = if previous_ch == ')' {
                    let Some(open_idx) = matching_open_paren(input, previous_idx) else {
                        continue;
                    };
                    (open_idx, input[open_idx + 1..previous_idx].trim())
                } else {
                    let start_idx = factorial_atom_start(input, previous_idx);
                    (start_idx, input[start_idx..=previous_idx].trim())
                };
                replacements.push((
                    start_idx,
                    byte_idx + ch.len_utf8(),
                    format!("factorial({inner})"),
                ));
            }
            _ => {}
        }
    }

    if replacements.is_empty() {
        return input.to_string();
    }

    let mut out = input.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        out.replace_range(start..end, &replacement);
    }
    out
}

pub(super) fn previous_non_space(input: &str, before: usize) -> Option<(usize, char)> {
    input[..before]
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
}

pub(super) fn matching_open_paren(input: &str, close_idx: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (idx, ch) in input[..=close_idx].char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn factorial_atom_start(input: &str, atom_end: usize) -> usize {
    input[..=atom_end]
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.') {
                None
            } else {
                Some(idx + ch.len_utf8())
            }
        })
        .unwrap_or(0)
}

pub(super) fn normalize_not_string_predicates(input: &str) -> String {
    if !input.to_ascii_lowercase().contains(" contains ") {
        return input.to_string();
    }
    let mut out = input.to_string();
    if let Some(op) = find_keyword_outside_quotes(&out, " contains ") {
        let before = out[..op].to_ascii_lowercase();
        let Some(not_start) = before.rfind(" not ") else {
            return out;
        };
        let lhs_start = not_start + " not ".len();
        let rhs_start = op + " contains ".len();
        let rhs_end = rhs_start + find_predicate_rhs_len(&out[rhs_start..]);
        let lhs = out[lhs_start..op].trim();
        let rhs = out[rhs_start..rhs_end].trim();
        let replacement = format!("contains_fn({lhs}, {rhs}) = false");
        out.replace_range(not_start + 1..rhs_end, &replacement);
    }
    out
}

pub(super) fn normalize_regex_match_operator(input: &str) -> String {
    if !input.contains("=~") {
        return input.to_string();
    }
    let mut out = input.to_string();
    while let Some(op) = find_operator_outside_quotes(&out, "=~") {
        let lhs_start = predicate_lhs_start(&out[..op]);
        let rhs_start = op + 2;
        let rhs_end = rhs_start + find_predicate_rhs_len(&out[rhs_start..]);
        let lhs = out[lhs_start..op].trim();
        let rhs = out[rhs_start..rhs_end].trim();
        let replacement = format!("regexp_full_match({lhs}, {rhs})");
        out.replace_range(lhs_start..rhs_end, &replacement);
    }
    out
}

// Keywords start at expression boundaries, never inside names or property access.
fn keyword_start(input: &str, index: usize) -> bool {
    index == 0 || input[..index].chars().next_back().is_some_and(|ch|
        ch.is_whitespace() || matches!(ch, '(' | '[' | '{' | ',' | ':' | '|'))
}

pub(super) fn predicate_lhs_start(prefix: &str) -> usize {
    // Track the operand boundary at each nesting level. A completed function
    // call belongs to its parent operand, even when its arguments contain words
    // such as RETURN or parentheses inside string literals.
    let mut start = 0;
    let mut parents = Vec::new();
    let mut cases = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    for (idx, ch) in prefix.char_indices() {
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
            '\'' | '"' | '`' => quote = Some(ch),
            '(' | '[' | '{' => {
                parents.push(start);
                start = idx + ch.len_utf8();
            }
            ')' | ']' | '}' => start = parents.pop().unwrap_or(start),
            ',' | ':' | '|' => start = idx + ch.len_utf8(),
            _ if ch.is_ascii_alphabetic() && keyword_start(prefix, idx) => {
                let word = prefix[idx..]
                    .split(|c: char| !is_ident_continue(c))
                    .next()
                    .unwrap_or("");
                if word.eq_ignore_ascii_case("case") {
                    cases.push((parents.len(), start));
                } else if word.eq_ignore_ascii_case("end") {
                    if cases.last().is_some_and(|(depth, _)| *depth == parents.len()) {
                        start = cases.pop().unwrap().1;
                    }
                } else if [
                    "where", "and", "or", "xor", "return", "with", "when", "then", "else", "not",
                ].iter().any(|keyword| word.eq_ignore_ascii_case(keyword)) {
                    start = idx + word.len();
                }
            }
            _ => {}
        }
    }
    start + prefix[start..].len() - prefix[start..].trim_start().len()
}

pub(super) fn find_predicate_rhs_len(input: &str) -> usize {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0i32;
    let mut case_depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if ch.is_ascii_alphabetic() && keyword_start(input, byte_idx) {
            if keyword_at(&input[byte_idx..], "CASE") {
                case_depth += 1;
            } else if keyword_at(&input[byte_idx..], "END") {
                case_depth = case_depth.saturating_sub(1);
            }
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            ')' | ']' | '}' | ',' if depth == 0 && case_depth == 0 => return byte_idx,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if depth == 0 && case_depth == 0 && is_clause_boundary(&input[byte_idx..]) => return byte_idx,
            _ => {}
        }
        i += 1;
    }
    input.len()
}

pub(super) fn is_clause_boundary(input: &str) -> bool {
    if !input.starts_with(char::is_whitespace) {
        return false;
    }
    let lower = input.trim_start().to_ascii_lowercase();
    [
        "return", "with", "and", "or", "xor", "order", "limit", "skip", "as", "then", "else", "end",
    ]
    .iter()
    .any(|needle| {
        lower
            .strip_prefix(needle)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
    })
}

pub(super) fn find_operator_outside_quotes(input: &str, operator: &str) -> Option<usize> {
    find_outside_quotes(input, |rest| rest.starts_with(operator))
}

pub(super) fn find_keyword_outside_quotes(input: &str, keyword: &str) -> Option<usize> {
    let lowered_keyword = keyword.to_ascii_lowercase();
    find_outside_quotes(input, |rest| {
        rest.to_ascii_lowercase().starts_with(&lowered_keyword)
    })
}

pub(super) fn find_outside_quotes(input: &str, matches: impl Fn(&str) -> bool) -> Option<usize> {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, (byte_idx, ch)) in chars.iter().copied().enumerate() {
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
        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            continue;
        }
        if matches(&input[byte_idx..]) {
            return Some(byte_idx);
        }
        if i + 1 == chars.len() {
            break;
        }
    }
    None
}

#[cfg(test)]
#[path = "../../../../../tests/unit/extracted/language__cypher__parser__normalization__operators__quote_and_sign_contracts.rs"]
mod quote_and_sign_contracts;
