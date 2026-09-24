//! Keyword function names and named argument rewrites.

pub(super) fn normalize_keyword_function_names(input: &str) -> String {
    if !input.to_ascii_lowercase().contains("contains") {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 8);
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut i = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while i < chars.len() {
        let (_, ch) = chars[i];
        if let Some(q) = quote {
            out.push(ch);
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
        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            out.push(ch);
            i += 1;
            continue;
        }
        if is_identifier_start(ch) {
            let start = i;
            let mut end = i + 1;
            while end < chars.len() && is_identifier_continue(chars[end].1) {
                end += 1;
            }
            let ident = &input[chars[start].0..chars[end - 1].0 + chars[end - 1].1.len_utf8()];
            let mut next = end;
            while next < chars.len() && chars[next].1.is_whitespace() {
                next += 1;
            }
            if ident.eq_ignore_ascii_case("contains") && next < chars.len() && chars[next].1 == '('
            {
                out.push_str("contains_fn");
            } else {
                out.push_str(ident);
            }
            i = end;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

pub(super) fn normalize_named_function_args(input: &str) -> String {
    if !input.contains(":=") && !input.contains("\":") && !input.contains("':") {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 8);
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut i = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut named_depths: Vec<i32> = Vec::new();
    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        if let Some(q) = quote {
            out.push(ch);
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
        match ch {
            '\'' | '"' => {
                if let Some((key, next)) = read_quoted_map_key(input, &chars, i) {
                    out.push_str(&key);
                    i = next;
                    continue;
                }
                quote = Some(ch);
                out.push(ch);
                i += 1;
                continue;
            }
            '(' | '[' | '{' => {
                for depth in &mut named_depths {
                    *depth += 1;
                }
            }
            ')' | ']' | '}' => {
                while matches!(named_depths.last(), Some(0)) {
                    out.push('}');
                    named_depths.pop();
                }
                for depth in &mut named_depths {
                    *depth -= 1;
                }
            }
            ',' => {
                while matches!(named_depths.last(), Some(0)) {
                    out.push('}');
                    named_depths.pop();
                }
            }
            _ => {}
        }
        if is_identifier_start(ch) {
            let start = i;
            let mut end = i + 1;
            while end < chars.len() && is_identifier_continue(chars[end].1) {
                end += 1;
            }
            let mut after = end;
            while after < chars.len() && chars[after].1.is_whitespace() {
                after += 1;
            }
            if after + 1 < chars.len() && chars[after].1 == ':' && chars[after + 1].1 == '=' {
                let ident_start = chars[start].0;
                let ident_end = if end < chars.len() {
                    chars[end].0
                } else {
                    input.len()
                };
                out.push('{');
                out.push_str(&input[ident_start..ident_end]);
                out.push(':');
                i = after + 2;
                while i < chars.len() && chars[i].1.is_whitespace() {
                    out.push(chars[i].1);
                    i += 1;
                }
                named_depths.push(0);
                continue;
            }
            out.push_str(
                &input[byte_idx..if end < chars.len() {
                    chars[end].0
                } else {
                    input.len()
                }],
            );
            i = end;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    while named_depths.pop().is_some() {
        out.push('}');
    }
    out
}

pub(super) fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

pub(super) fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

pub(super) fn read_quoted_map_key(
    input: &str,
    chars: &[(usize, char)],
    start: usize,
) -> Option<(String, usize)> {
    let quote = chars[start].1;
    let mut end = start + 1;
    let mut escaped = false;
    while end < chars.len() {
        let ch = chars[end].1;
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            break;
        }
        end += 1;
    }
    if end >= chars.len() {
        return None;
    }
    let mut after = end + 1;
    while after < chars.len() && chars[after].1.is_whitespace() {
        after += 1;
    }
    if after >= chars.len() || chars[after].1 != ':' {
        return None;
    }
    if after + 1 < chars.len() && chars[after + 1].1 == '=' {
        return None;
    }
    let key_start = chars[start].0 + quote.len_utf8();
    let key_end = chars[end].0;
    let key = &input[key_start..key_end];
    if key.is_empty()
        || !key.chars().next().is_some_and(is_identifier_start)
        || !key.chars().all(is_identifier_continue)
    {
        return None;
    }
    Some((key.to_string(), end + 1))
}
