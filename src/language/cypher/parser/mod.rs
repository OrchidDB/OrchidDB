use crate::ParsedGraphProgram;
use crate::grammar::generated::cypher::cypherlexer::CypherLexer;
use crate::grammar::generated::cypher::cypherparser as c;
use crate::grammar::generated::cypher::cypherparser::*;
use crate::grammar::generated::cypher::cyphervisitor::CypherVisitor;
use crate::language::cypher::ast::*;
use antlr4rust::InputStream;
use antlr4rust::common_token_stream::CommonTokenStream;
use antlr4rust::error_listener::ErrorListener;
use antlr4rust::errors::ANTLRError;
use antlr4rust::parser::Parser;
use antlr4rust::recognizer::Recognizer;
use antlr4rust::token::{TOKEN_DEFAULT_CHANNEL, TOKEN_EOF, Token};
use antlr4rust::token_factory::TokenFactory;
use antlr4rust::token_stream::UnbufferedTokenStream;
use antlr4rust::tree::{ParseTree, ParseTreeVisitor};
use std::cell::RefCell;
use std::rc::Rc;

pub mod lowering;

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum CypherParseError {
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported cypher construct: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, CypherParseError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CypherToken {
    pub token_type: i32,
    pub symbolic_name: Option<&'static str>,
    pub literal_name: Option<&'static str>,
    pub text: String,
    pub line: isize,
    pub column: isize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CypherSyntax {
    pub parse_tree: String,
    pub tokens: Vec<CypherToken>,
}

pub fn parse_cypher(input: &str) -> Result<CypherProgram> {
    let query = parse_query(input)?;
    Ok(CypherProgram::new(
        ParsedGraphProgram {
            entry_rule: "oC_Cypher".to_string(),
        },
        query,
    ))
}

pub fn parse_query(input: &str) -> Result<Query> {
    let normalized = normalize_cypher_extensions(input);
    let (root, _syntax) = parse_root(&normalized)?;
    let mut visitor = lowering::visitor::AstLoweringVisitor::new();
    visitor.visit_oC_Cypher(&root);
    visitor.finish()
}

pub fn parse_syntax(input: &str) -> Result<CypherSyntax> {
    let normalized = normalize_cypher_extensions(input);
    let (_root, syntax) = parse_root(&normalized)?;
    Ok(syntax)
}

fn normalize_cypher_extensions(input: &str) -> String {
    let normalized = normalize_named_function_args(input);
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

fn normalize_spaced_unary_signs(input: &str) -> String {
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
            '+' | '-' if unary_sign_context(&out) => {
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

fn unary_sign_context(prefix: &str) -> bool {
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

fn normalize_bitwise_operators(input: &str) -> String {
    if !contains_bitwise_operator(input) {
        return input.to_string();
    }

    let mut out = input.to_string();
    let mut search_from = 0;
    while let Some((keyword_start, expr_start)) = find_projection_keyword(&out, search_from) {
        let expr_end = expr_start + projection_body_len(&out[expr_start..]);
        let body = &out[expr_start..expr_end];
        let rewritten = rewrite_projection_bitwise(body);
        out.replace_range(expr_start..expr_end, &rewritten);
        search_from = keyword_start + rewritten.len();
    }
    out
}

fn contains_bitwise_operator(input: &str) -> bool {
    find_outside_quotes(input, |rest| {
        rest.starts_with('&') || rest.starts_with("<<") || rest.starts_with(">>")
    })
    .is_some()
}

fn find_projection_keyword(input: &str, start: usize) -> Option<(usize, usize)> {
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

fn keyword_at(input: &str, keyword: &str) -> bool {
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

fn projection_body_len(input: &str) -> usize {
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

fn projection_clause_boundary(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        " order by ",
        " skip ",
        " limit ",
        " union ",
        " match ",
        " where ",
        " unwind ",
    ]
    .iter()
    .any(|needle| lower.starts_with(needle))
}

fn rewrite_projection_bitwise(input: &str) -> String {
    split_top_level_commas(input)
        .into_iter()
        .map(|part| rewrite_bitwise_expr(part.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
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

fn rewrite_bitwise_expr(input: &str) -> String {
    let expr = input.trim();
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
    expr.to_string()
}

fn find_top_level_bitwise(input: &str, operators: &[&str]) -> Option<(usize, usize, &'static str)> {
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

fn enclosing_parens(input: &str) -> Option<(usize, usize)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return None;
    }
    let mut depth = 0i32;
    for (byte_idx, ch) in trimmed.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && byte_idx != trimmed.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }
    if depth == 0 {
        Some((1, trimmed.len() - 1))
    } else {
        None
    }
}

fn normalize_postfix_factorial(input: &str) -> String {
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

fn previous_non_space(input: &str, before: usize) -> Option<(usize, char)> {
    input[..before]
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
}

fn matching_open_paren(input: &str, close_idx: usize) -> Option<usize> {
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

fn factorial_atom_start(input: &str, atom_end: usize) -> usize {
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

fn normalize_not_string_predicates(input: &str) -> String {
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

fn normalize_regex_match_operator(input: &str) -> String {
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

fn predicate_lhs_start(prefix: &str) -> usize {
    let lower = prefix.to_ascii_lowercase();
    [" where ", " and ", " or ", "("]
        .iter()
        .filter_map(|needle| lower.rfind(needle).map(|idx| idx + needle.len()))
        .max()
        .unwrap_or(0)
}

fn find_predicate_rhs_len(input: &str) -> usize {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0i32;
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
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if depth == 0 && is_clause_boundary(&input[byte_idx..]) => return byte_idx,
            _ => {}
        }
        i += 1;
    }
    input.len()
}

fn is_clause_boundary(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        " return ", " with ", " and ", " or ", " order ", " limit ", " skip ",
    ]
    .iter()
    .any(|needle| lower.starts_with(needle))
}

fn find_operator_outside_quotes(input: &str, operator: &str) -> Option<usize> {
    find_outside_quotes(input, |rest| rest.starts_with(operator))
}

fn find_keyword_outside_quotes(input: &str, keyword: &str) -> Option<usize> {
    let lowered_keyword = keyword.to_ascii_lowercase();
    find_outside_quotes(input, |rest| {
        rest.to_ascii_lowercase().starts_with(&lowered_keyword)
    })
}

fn find_outside_quotes(input: &str, matches: impl Fn(&str) -> bool) -> Option<usize> {
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

fn normalize_keyword_function_names(input: &str) -> String {
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

fn normalize_named_function_args(input: &str) -> String {
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

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn read_quoted_map_key(
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

fn normalize_lambda_list_functions(input: &str) -> String {
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

fn rewrite_lambda_function_at(input: &str, index: usize) -> Option<(String, usize)> {
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

fn match_keyword_at(input: &str, index: usize, keyword: &str) -> Option<usize> {
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

fn skip_space(input: &str, mut index: usize) -> usize {
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn find_matching(input: &str, open_index: usize, open: char, close: char) -> Option<usize> {
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

fn split_top_level_args(input: &str) -> Vec<&str> {
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

fn split_single_lambda(input: &str) -> Option<(String, &str)> {
    let arrow = top_level_arrow(input)?;
    let variable = strip_wrapping_parens(input[..arrow].trim()).trim();
    if variable.is_empty() || variable.contains(',') {
        return None;
    }
    Some((variable.to_string(), &input[arrow + 2..]))
}

fn split_reduce_lambda(input: &str) -> Option<(String, String, &str)> {
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

fn strip_wrapping_parens(input: &str) -> &str {
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

fn normalize_elided_list_elements(input: &str) -> String {
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

fn is_likely_list_literal(input: &str, open_index: usize) -> bool {
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

fn rewrite_elided_list_elements(input: &str) -> Option<String> {
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

fn top_level_arrow(input: &str) -> Option<usize> {
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

fn normalize_colon_slices(input: &str) -> String {
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

fn top_level_colon_slice(input: &str) -> Option<usize> {
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

fn parse_root(input: &str) -> Result<(Rc<OC_CypherContextAll<'_>>, CypherSyntax)> {
    let errors = SyntaxErrors::default();
    let mut lexer = CypherLexer::new(InputStream::new(input));
    lexer.remove_error_listeners();
    lexer.add_error_listener(Box::new(errors.listener()));

    let token_stream = CommonTokenStream::new(lexer);
    let mut parser = CypherParser::new(token_stream);
    parser.remove_error_listeners();
    parser.add_error_listener(Box::new(errors.listener()));

    let root = parser
        .oC_Cypher()
        .map_err(|err| CypherParseError::Parse(err.to_string()))?;
    errors.into_result()?;

    let syntax = CypherSyntax {
        parse_tree: root.to_string_tree(&*parser),
        tokens: tokenize(input)?,
    };
    Ok((root, syntax))
}

pub fn tokenize(input: &str) -> Result<Vec<CypherToken>> {
    let errors = SyntaxErrors::default();
    let mut lexer = CypherLexer::new(InputStream::new(input));
    lexer.remove_error_listeners();
    lexer.add_error_listener(Box::new(errors.listener()));

    let mut token_stream = UnbufferedTokenStream::new_buffered(lexer);
    let mut tokens = Vec::new();
    for token in token_stream.token_iter() {
        let token_type = token.get_token_type();
        if token_type == TOKEN_EOF {
            break;
        }
        if token.get_channel() != TOKEN_DEFAULT_CHANNEL {
            continue;
        }
        tokens.push(CypherToken {
            token_type,
            symbolic_name: token_name(&c::_SYMBOLIC_NAMES, token_type),
            literal_name: token_name(&c::_LITERAL_NAMES, token_type),
            text: token.get_text().to_string(),
            line: token.get_line(),
            column: token.get_column(),
        });
    }
    errors.into_result()?;
    Ok(tokens)
}

fn token_name(names: &[Option<&'static str>], token_type: i32) -> Option<&'static str> {
    if token_type < 0 {
        return None;
    }
    names
        .get(token_type as usize)
        .and_then(|name| name.as_ref().copied())
}

#[derive(Clone, Default)]
struct SyntaxErrors {
    messages: Rc<RefCell<Vec<String>>>,
}

impl SyntaxErrors {
    fn listener(&self) -> Self {
        self.clone()
    }

    fn into_result(self) -> Result<()> {
        let messages = self.messages.borrow();
        if messages.is_empty() {
            Ok(())
        } else {
            Err(CypherParseError::Parse(messages.join("; ")))
        }
    }
}

impl<'a, T> ErrorListener<'a, T> for SyntaxErrors
where
    T: Recognizer<'a>,
{
    fn syntax_error(
        &self,
        _recognizer: &T,
        offending_symbol: Option<&<T::TF as TokenFactory<'a>>::Inner>,
        line: isize,
        column: isize,
        msg: &str,
        _error: Option<&ANTLRError>,
    ) {
        let offending = offending_symbol
            .map(ToString::to_string)
            .unwrap_or_else(|| "<unknown>".to_string());
        self.messages
            .borrow_mut()
            .push(format!("line {line}:{column} {msg} near {offending}"));
    }
}

#[derive(Default)]
struct AstVisitor {
    query: Option<Query>,
    errors: Vec<CypherParseError>,
}

impl AstVisitor {
    fn finish(mut self) -> Result<Query> {
        if let Some(err) = self.errors.drain(..).next() {
            return Err(err);
        }
        self.query
            .ok_or_else(|| CypherParseError::Parse("no query found".to_string()))
    }

    fn fail(&mut self, err: CypherParseError) {
        self.errors.push(err);
    }

    fn build_query(&mut self, ctx: &OC_QueryContext<'_>) -> Option<Query> {
        let regular = ctx.oC_RegularQuery()?;
        if !regular.oC_Union_all().is_empty() {
            self.fail(CypherParseError::Unsupported(
                "UNION parsing is recognized but not lowered yet".to_string(),
            ));
            return None;
        }
        self.build_regular_query(&regular)
    }

    fn build_regular_query(&mut self, ctx: &OC_RegularQueryContext<'_>) -> Option<Query> {
        let single = ctx.oC_SingleQuery()?;
        self.build_single_query(&single)
    }

    fn build_single_query(&mut self, ctx: &OC_SingleQueryContext<'_>) -> Option<Query> {
        if let Some(single) = ctx.oC_SinglePartQuery() {
            return Some(self.build_single_part_query(&single));
        }
        if let Some(multi) = ctx.oC_MultiPartQuery() {
            return Some(self.build_multi_part_query(&multi));
        }
        None
    }

    fn build_multi_part_query(&mut self, ctx: &OC_MultiPartQueryContext<'_>) -> Query {
        let mut clauses = Vec::new();
        for reading in ctx.oC_ReadingClause_all() {
            if let Some(clause) = self.build_reading_clause(&reading) {
                clauses.push(clause);
            }
        }
        for with in ctx.oC_With_all() {
            if let Some(body) = with.oC_ProjectionBody() {
                clauses.push(Clause::With(WithClause {
                    projection: self.build_projection_body(&body),
                    predicate: with
                        .oC_Where()
                        .and_then(|where_ctx| self.build_where(&where_ctx)),
                }));
            }
        }
        if let Some(single) = ctx.oC_SinglePartQuery() {
            clauses.extend(self.build_single_part_query(&single).clauses);
        }
        Query::new(clauses)
    }

    fn build_single_part_query(&mut self, ctx: &OC_SinglePartQueryContext<'_>) -> Query {
        let mut clauses = Vec::new();
        for reading in ctx.oC_ReadingClause_all() {
            if let Some(clause) = self.build_reading_clause(&reading) {
                clauses.push(clause);
            }
        }
        if !ctx.oC_UpdatingClause_all().is_empty() {
            self.fail(CypherParseError::Unsupported(
                "mutating Cypher clauses are outside the read IR".to_string(),
            ));
        }
        if let Some(ret) = ctx.oC_Return() {
            if let Some(body) = ret.oC_ProjectionBody() {
                clauses.push(Clause::Return(ReturnClause {
                    projection: self.build_projection_body(&body),
                }));
            }
        }
        Query::new(clauses)
    }

    fn build_reading_clause(&mut self, ctx: &OC_ReadingClauseContext<'_>) -> Option<Clause> {
        if let Some(m) = ctx.oC_Match() {
            return Some(Clause::Match(self.build_match(&m)));
        }
        if let Some(unwind) = ctx.oC_Unwind() {
            let expr = unwind
                .oC_Expression()
                .map(|expr| parse_expr_text(&expr.get_text()))
                .unwrap_or_else(|| Expr::Literal(Literal::Null));
            let alias = unwind
                .oC_Variable()
                .map(|var| clean_identifier(&var.get_text()))
                .unwrap_or_else(|| "_unwind".to_string());
            return Some(Clause::Unwind(UnwindClause { expr, alias }));
        }
        self.fail(CypherParseError::Unsupported(format!(
            "reading clause `{}` is not implemented yet",
            ctx.get_text()
        )));
        None
    }

    fn build_match(&mut self, ctx: &OC_MatchContext<'_>) -> MatchClause {
        let patterns = ctx
            .oC_Pattern()
            .map(|pattern| self.build_pattern(&pattern))
            .unwrap_or_default();
        MatchClause {
            optional: ctx.OPTIONAL().is_some(),
            patterns,
            predicate: ctx
                .oC_Where()
                .and_then(|where_ctx| self.build_where(&where_ctx)),
        }
    }

    fn build_where(&self, ctx: &OC_WhereContext<'_>) -> Option<Expr> {
        ctx.oC_Expression()
            .map(|expr| parse_expr_text(&expr.get_text()))
    }

    fn build_pattern(&mut self, ctx: &OC_PatternContext<'_>) -> Vec<PatternPart> {
        ctx.oC_PatternPart_all()
            .into_iter()
            .filter_map(|part| self.build_pattern_part(&part))
            .collect()
    }

    fn build_pattern_part(&mut self, ctx: &OC_PatternPartContext<'_>) -> Option<PatternPart> {
        let variable = ctx
            .oC_Variable()
            .map(|var| clean_identifier(&var.get_text()));
        let anon = ctx.oC_AnonymousPatternPart()?;
        let element = anon.oC_PatternElement()?;
        Some(PatternPart {
            variable,
            element: self.build_pattern_element(&element)?,
        })
    }

    fn build_pattern_element(
        &mut self,
        ctx: &OC_PatternElementContext<'_>,
    ) -> Option<PatternElement> {
        if let Some(nested) = ctx.oC_PatternElement() {
            return self.build_pattern_element(&nested);
        }
        let start_ctx = ctx.oC_NodePattern()?;
        let start = self.build_node_pattern(start_ctx.as_ref())?;
        let chains = ctx
            .oC_PatternElementChain_all()
            .into_iter()
            .filter_map(|chain| self.build_pattern_chain(&chain))
            .collect();
        Some(PatternElement { start, chains })
    }

    fn build_pattern_chain(
        &mut self,
        ctx: &OC_PatternElementChainContext<'_>,
    ) -> Option<PatternElementChain> {
        Some(PatternElementChain {
            relationship: self
                .build_relationship_pattern(ctx.oC_RelationshipPattern()?.as_ref())?,
            node: self.build_node_pattern(ctx.oC_NodePattern()?.as_ref())?,
        })
    }

    fn build_node_pattern(&mut self, ctx: &OC_NodePatternContext<'_>) -> Option<NodePattern> {
        Some(NodePattern {
            variable: ctx
                .oC_Variable()
                .map(|var| clean_identifier(&var.get_text())),
            labels: ctx
                .oC_NodeLabels()
                .map(|labels| {
                    labels
                        .oC_NodeLabel_all()
                        .into_iter()
                        .map(|label| clean_label(&label.get_text()))
                        .collect()
                })
                .unwrap_or_default(),
            properties: ctx
                .oC_Properties()
                .map(|props| parse_expr_text(&props.get_text())),
        })
    }

    fn build_relationship_pattern(
        &mut self,
        ctx: &OC_RelationshipPatternContext<'_>,
    ) -> Option<RelationshipPattern> {
        let direction = match (
            ctx.oC_LeftArrowHead().is_some(),
            ctx.oC_RightArrowHead().is_some(),
        ) {
            (true, false) => crate::ir::plan::Direction::In,
            (false, true) => crate::ir::plan::Direction::Out,
            _ => crate::ir::plan::Direction::Both,
        };
        let detail = ctx.oC_RelationshipDetail();
        Some(RelationshipPattern {
            variable: detail
                .as_ref()
                .and_then(|detail| detail.oC_Variable())
                .map(|var| clean_identifier(&var.get_text())),
            types: detail
                .as_ref()
                .and_then(|detail| detail.oC_RelationshipTypes())
                .map(|types| {
                    types
                        .oC_RelTypeName_all()
                        .into_iter()
                        .map(|ty| clean_label(&ty.get_text()))
                        .collect()
                })
                .unwrap_or_default(),
            range: detail
                .as_ref()
                .and_then(|detail| detail.oC_RangeLiteral())
                .map(|range| parse_range(&range.get_text()))
                .unwrap_or_default(),
            direction,
            properties: detail
                .as_ref()
                .and_then(|detail| detail.oC_Properties())
                .map(|props| parse_expr_text(&props.get_text())),
            recursive: None,
        })
    }

    fn build_projection_body(&mut self, ctx: &OC_ProjectionBodyContext<'_>) -> ProjectionBody {
        let items_ctx = ctx.oC_ProjectionItems();
        ProjectionBody {
            distinct: ctx.DISTINCT().is_some(),
            include_existing: items_ctx
                .as_ref()
                .map(|items| items.get_text().trim_start().starts_with('*'))
                .unwrap_or(false),
            items: items_ctx
                .map(|items| {
                    items
                        .oC_ProjectionItem_all()
                        .into_iter()
                        .map(|item| self.build_projection_item(&item))
                        .collect()
                })
                .unwrap_or_default(),
            order_by: ctx
                .oC_Order()
                .map(|order| {
                    order
                        .oC_SortItem_all()
                        .into_iter()
                        .map(|item| SortItem {
                            expr: item
                                .oC_Expression()
                                .map(|expr| parse_expr_text(&expr.get_text()))
                                .unwrap_or_else(|| Expr::Literal(Literal::Null)),
                            direction: if item.DESC().is_some() || item.DESCENDING().is_some() {
                                SortDirection::Desc
                            } else {
                                SortDirection::Asc
                            },
                        })
                        .collect()
                })
                .unwrap_or_default(),
            skip: ctx
                .oC_Skip()
                .and_then(|skip| skip.oC_Expression())
                .map(|expr| parse_expr_text(&expr.get_text())),
            limit: ctx
                .oC_Limit()
                .and_then(|limit| limit.oC_Expression())
                .map(|expr| parse_expr_text(&expr.get_text())),
        }
    }

    fn build_projection_item(&mut self, ctx: &OC_ProjectionItemContext<'_>) -> ProjectionItem {
        ProjectionItem {
            expr: ctx
                .oC_Expression()
                .map(|expr| parse_expr_text(&expr.get_text()))
                .unwrap_or_else(|| Expr::Literal(Literal::Null)),
            alias: ctx
                .oC_Variable()
                .map(|var| clean_identifier(&var.get_text())),
            explicit_alias: ctx.AS().is_some(),
        }
    }
}

impl<'input> ParseTreeVisitor<'input, CypherParserContextType> for AstVisitor {}

impl<'input> CypherVisitor<'input> for AstVisitor {
    fn visit_oC_Cypher(&mut self, ctx: &OC_CypherContext<'input>) {
        let Some(statement) = ctx.oC_Statement() else {
            self.fail(CypherParseError::Parse("missing statement".to_string()));
            return;
        };
        self.visit_oC_Statement(&statement);
    }

    fn visit_oC_Statement(&mut self, ctx: &OC_StatementContext<'input>) {
        let Some(query_ctx) = ctx.oC_Query() else {
            self.fail(CypherParseError::Parse("missing query".to_string()));
            return;
        };
        self.query = self.build_query(&query_ctx);
    }
}

fn clean_identifier(text: &str) -> String {
    let trimmed = text.trim();
    trimmed
        .strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(trimmed)
        .to_string()
}

fn clean_label(text: &str) -> String {
    clean_identifier(text.trim().trim_start_matches(':').trim_start_matches('|'))
}

fn parse_range(text: &str) -> RangeLiteral {
    let body = text.trim().trim_start_matches('*');
    if body.is_empty() {
        return RangeLiteral {
            min: 1,
            max: None,
            explicit: true,
        };
    }
    if let Some((min, max)) = body.split_once("..") {
        return RangeLiteral {
            min: min.parse().unwrap_or(1),
            max: if max.is_empty() {
                None
            } else {
                max.parse().ok()
            },
            explicit: true,
        };
    }
    let exact = body.parse().unwrap_or(1);
    RangeLiteral {
        min: exact,
        max: Some(exact),
        explicit: true,
    }
}

fn parse_expr_text(text: &str) -> Expr {
    ExprParser::new(text).parse_expression()
}

#[derive(Debug, Clone, PartialEq)]
enum ExprToken {
    Ident(String),
    Int(String),
    Float(f64),
    String(String),
    Param(String),
    Symbol(char),
    Op(&'static str),
    Eof,
}

struct ExprParser {
    tokens: Vec<ExprToken>,
    pos: usize,
}

impl ExprParser {
    fn new(input: &str) -> Self {
        Self {
            tokens: lex_expr(input),
            pos: 0,
        }
    }

    fn parse_expression(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut expr = self.parse_and();
        while self.consume_keyword("OR") {
            let rhs = self.parse_and();
            expr = Expr::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_and(&mut self) -> Expr {
        let mut expr = self.parse_not();
        while self.consume_keyword("AND") {
            let rhs = self.parse_not();
            expr = Expr::Binary {
                op: BinaryOp::And,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_not(&mut self) -> Expr {
        if self.consume_keyword("NOT") {
            return Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(self.parse_not()),
            };
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut expr = self.parse_add_sub();
        loop {
            if self.consume_keyword("ISNULL")
                || (self.consume_keyword("IS") && self.consume_keyword("NULL"))
            {
                expr = Expr::IsNull(Box::new(expr));
            } else if self.consume_keyword("ISNOTNULL")
                || (self.consume_keyword("IS")
                    && self.consume_keyword("NOT")
                    && self.consume_keyword("NULL"))
            {
                expr = Expr::IsNotNull(Box::new(expr));
            } else if self.consume_keyword("STARTSWITH") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::StartsWith,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if self.consume_keyword("ENDSWITH") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::EndsWith,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if self.consume_keyword("CONTAINS") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::Contains,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if self.consume_op("=~") {
                let rhs = self.parse_add_sub();
                expr = Expr::StringPredicate {
                    op: StringPredicateOp::Regex,
                    target: Box::new(expr),
                    pattern: Box::new(rhs),
                };
            } else if let Some(op) = self.consume_comparison_op() {
                let rhs = self.parse_add_sub();
                expr = Expr::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        expr
    }

    fn parse_add_sub(&mut self) -> Expr {
        let mut expr = self.parse_mul_div();
        loop {
            let op = if self.consume_symbol('+') {
                Some(BinaryOp::Add)
            } else if self.consume_symbol('-') {
                Some(BinaryOp::Sub)
            } else {
                None
            };
            let Some(op) = op else { break };
            let rhs = self.parse_mul_div();
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_mul_div(&mut self) -> Expr {
        let mut expr = self.parse_unary();
        loop {
            let op = if self.consume_symbol('*') {
                Some(BinaryOp::Mul)
            } else if self.consume_symbol('/') {
                Some(BinaryOp::Div)
            } else {
                None
            };
            let Some(op) = op else { break };
            let rhs = self.parse_unary();
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        expr
    }

    fn parse_unary(&mut self) -> Expr {
        if self.consume_symbol('-') {
            return Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(self.parse_unary()),
            };
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        while self.consume_symbol('.') {
            let key = match self.next() {
                ExprToken::Ident(name) => name,
                _ => "_".to_string(),
            };
            expr = Expr::Property {
                target: Box::new(expr),
                key,
            };
        }
        expr
    }

    fn parse_primary(&mut self) -> Expr {
        match self.next() {
            ExprToken::Ident(name) => {
                let upper = name.to_ascii_uppercase();
                match upper.as_str() {
                    "NULL" => Expr::Literal(Literal::Null),
                    "TRUE" => Expr::Literal(Literal::Bool(true)),
                    "FALSE" => Expr::Literal(Literal::Bool(false)),
                    "COUNT" if self.consume_symbol('(') && self.consume_symbol('*') => {
                        self.consume_symbol(')');
                        Expr::CountStar
                    }
                    _ if self.consume_symbol('(') => {
                        let distinct = self.consume_keyword("DISTINCT");
                        let mut args = Vec::new();
                        if !self.check_symbol(')') {
                            loop {
                                args.push(self.parse_expression());
                                if !self.consume_symbol(',') {
                                    break;
                                }
                            }
                        }
                        self.consume_symbol(')');
                        Expr::Function {
                            name,
                            distinct,
                            args,
                        }
                    }
                    _ => Expr::Variable(clean_identifier(&name)),
                }
            }
            ExprToken::Int(value) => Expr::Literal(Literal::Integer(value)),
            ExprToken::Float(value) => Expr::Literal(Literal::Float(value)),
            ExprToken::String(value) => Expr::Literal(Literal::String(value)),
            ExprToken::Param(name) => Expr::Parameter(clean_identifier(&name)),
            ExprToken::Symbol('*') => Expr::Star,
            ExprToken::Symbol('(') => {
                let expr = self.parse_expression();
                self.consume_symbol(')');
                expr
            }
            ExprToken::Symbol('[') => {
                let mut items = Vec::new();
                if !self.check_symbol(']') {
                    loop {
                        items.push(self.parse_expression());
                        if !self.consume_symbol(',') {
                            break;
                        }
                    }
                }
                self.consume_symbol(']');
                Expr::List(items)
            }
            ExprToken::Symbol('{') => {
                let mut items = Vec::new();
                if !self.check_symbol('}') {
                    loop {
                        let key = match self.next() {
                            ExprToken::Ident(name) | ExprToken::String(name) => {
                                clean_identifier(&name)
                            }
                            _ => "_".to_string(),
                        };
                        self.consume_symbol(':');
                        let value = self.parse_expression();
                        items.push((key, value));
                        if !self.consume_symbol(',') {
                            break;
                        }
                    }
                }
                self.consume_symbol('}');
                Expr::Map(items)
            }
            _ => Expr::Literal(Literal::Null),
        }
    }

    fn consume_comparison_op(&mut self) -> Option<BinaryOp> {
        let op = match self.peek() {
            ExprToken::Op("=") => BinaryOp::Eq,
            ExprToken::Op("<>") => BinaryOp::Neq,
            ExprToken::Op("<") => BinaryOp::Lt,
            ExprToken::Op("<=") => BinaryOp::Lte,
            ExprToken::Op(">") => BinaryOp::Gt,
            ExprToken::Op(">=") => BinaryOp::Gte,
            _ => return None,
        };
        self.pos += 1;
        Some(op)
    }

    fn consume_op(&mut self, expected: &str) -> bool {
        if matches!(self.peek(), ExprToken::Op(op) if *op == expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_keyword(&mut self, expected: &str) -> bool {
        match self.peek() {
            ExprToken::Ident(name) if name.eq_ignore_ascii_case(expected) => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    fn consume_symbol(&mut self, expected: char) -> bool {
        if self.check_symbol(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn check_symbol(&self, expected: char) -> bool {
        matches!(self.peek(), ExprToken::Symbol(actual) if *actual == expected)
    }

    fn next(&mut self) -> ExprToken {
        let token = self.peek().clone();
        if !matches!(token, ExprToken::Eof) {
            self.pos += 1;
        }
        token
    }

    fn peek(&self) -> &ExprToken {
        self.tokens.get(self.pos).unwrap_or(&ExprToken::Eof)
    }
}

fn lex_expr(input: &str) -> Vec<ExprToken> {
    let mut chars = input.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(ch) = chars.peek().copied() {
        match ch {
            c if c.is_whitespace() => {
                chars.next();
            }
            '\'' | '"' => {
                let quote = chars.next().unwrap();
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    if c == quote {
                        break;
                    }
                    if c == '\\' {
                        if let Some(next) = chars.next() {
                            value.push(next);
                        }
                    } else {
                        value.push(c);
                    }
                }
                tokens.push(ExprToken::String(value));
            }
            '`' => {
                chars.next();
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    if c == '`' {
                        break;
                    }
                    value.push(c);
                }
                tokens.push(ExprToken::Ident(value));
            }
            '$' => {
                chars.next();
                tokens.push(ExprToken::Param(read_identifier(&mut chars)));
            }
            '0'..='9' => {
                let mut value = String::new();
                let mut is_float = false;
                while let Some(c) = chars.peek().copied() {
                    if c.is_ascii_digit() {
                        value.push(c);
                        chars.next();
                    } else if c == '.' && !is_float {
                        is_float = true;
                        value.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if is_float {
                    tokens.push(ExprToken::Float(value.parse().unwrap_or(0.0)));
                } else {
                    tokens.push(ExprToken::Int(value));
                }
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'~') {
                    chars.next();
                    tokens.push(ExprToken::Op("=~"));
                } else {
                    tokens.push(ExprToken::Op("="));
                }
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ExprToken::Op("<="));
                } else if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(ExprToken::Op("<>"));
                } else {
                    tokens.push(ExprToken::Op("<"));
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ExprToken::Op(">="));
                } else {
                    tokens.push(ExprToken::Op(">"));
                }
            }
            '.' => {
                chars.next();
                tokens.push(ExprToken::Symbol('.'));
            }
            ',' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '-' | '*' | '/' => {
                tokens.push(ExprToken::Symbol(ch));
                chars.next();
            }
            c if is_ident_start(c) => {
                let ident = read_identifier(&mut chars);
                tokens.push(ExprToken::Ident(ident));
            }
            _ => {
                chars.next();
            }
        }
    }
    tokens.push(ExprToken::Eof);
    tokens
}

fn read_identifier<I>(chars: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = char>,
{
    let mut value = String::new();
    while let Some(c) = chars.peek().copied() {
        if is_ident_continue(c) {
            value.push(c);
            chars.next();
        } else {
            break;
        }
    }
    value
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}
