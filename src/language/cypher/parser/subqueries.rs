//! Token-aware CALL subqueries, layered over the openCypher procedure grammar.
use super::*;
use std::collections::BTreeMap;

pub(super) fn extract(input: &str) -> Result<(String, BTreeMap<String, CallSubquery>)> {
    let tokens = tokenize(input)?;
    let tokens: Vec<_> = tokens.iter().filter(|t| t.symbolic_name != Some("SP") && !t.text.trim().is_empty()).collect();
    let mut prefix = "__orchid_call_subquery_".to_owned();
    while input.contains(&prefix) { prefix.push('_'); }
    fn offset(input: &str, token: &CypherToken) -> usize {
        let line_start = input.split_inclusive('\n').take((token.line - 1) as usize).map(str::len).sum::<usize>();
        line_start + input[line_start..].char_indices().nth(token.column as usize).map(|(i, _)| i).unwrap_or(input.len() - line_start)
    }
    let mut output = String::new();
    let mut copied = 0;
    let mut nesting = 0usize;
    for token in &tokens {
        match token.text.as_str() {
            "{" | "[" | "(" => { nesting += 1; if nesting > 256 { return Err(CypherParseError::Unsupported("syntactic nesting exceeds the parser limit of 256".into())); } }
            "}" | "]" | ")" => nesting = nesting.saturating_sub(1),
            _ => {}
        }
    }
    let mut queries = BTreeMap::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].text.eq_ignore_ascii_case("CALL") && tokens.get(i + 1).is_some_and(|t| t.text == "{") {
            let start = i + 2;
            let mut end = start;
            let mut depth = 1;
            while end < tokens.len() {
                match tokens[end].text.as_str() { "{" => depth += 1, "}" => depth -= 1, _ => {} }
                if depth == 0 { break; }
                end += 1;
            }
            if depth != 0 { return Err(CypherParseError::Parse("Unclosed CALL subquery".into())); }
            let body = &input[offset(input, tokens[i + 1]) + 1..offset(input, tokens[end])];
            let query = parse_query(&body)?;
            let imports = if let Some(Clause::With(with)) = query.clauses.first() {
                if with.projection.include_existing || with.projection.distinct || with.predicate.is_some()
                    || !with.projection.order_by.is_empty() || with.projection.skip.is_some() || with.projection.limit.is_some() {
                    return Err(CypherParseError::Unsupported("CALL imports require a plain WITH of variables".into()));
                }
                with.projection.items.iter().map(|item| match &item.expr {
                    Expr::Variable(name) if !item.explicit_alias => Ok(name.clone()),
                    _ => Err(CypherParseError::Unsupported("CALL imports require unaliased variables".into())),
                }).collect::<Result<Vec<_>>>()?
            } else { Vec::new() };
            let Some(Clause::Return(ret)) = query.clauses.last() else {
                return Err(CypherParseError::Unsupported("CALL subqueries require RETURN".into()));
            };
            if ret.projection.include_existing {
                return Err(CypherParseError::Unsupported("CALL subqueries require explicit return columns".into()));
            }
            let outputs = ret.projection.items.iter().map(|item| item.alias.clone().or_else(||
                if let Expr::Variable(name) = &item.expr {Some(name.clone())} else {None})
                .ok_or_else(|| CypherParseError::Unsupported("CALL return expressions require aliases".into())))
                .collect::<Result<Vec<_>>>()?;
            let name = format!("{prefix}{}", queries.len());
            output.push_str(&input[copied..offset(input, tokens[i])]);
            output.push_str(&format!("CALL {name}() YIELD __subquery_placeholder"));
            copied = offset(input, tokens[end]) + 1;
            queries.insert(name, CallSubquery { query: Box::new(query), imports, outputs });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    output.push_str(&input[copied..]);
    Ok((output, queries))
}

pub(super) fn restore(query: &mut Query, calls: &mut BTreeMap<String, CallSubquery>) {
    for clause in &mut query.clauses {
        if let Clause::Call(call) = clause && let Some(subquery) = calls.remove(&call.name) {
            *clause = Clause::Subquery(subquery);
        }
    }
    for branch in &mut query.unions { restore(&mut branch.query, calls); }
}
