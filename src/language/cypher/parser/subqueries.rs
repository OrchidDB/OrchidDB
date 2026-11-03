//! Token-aware CALL subqueries, layered over the openCypher procedure grammar.
use super::*;
use std::collections::BTreeMap;

// Retain source spelling for implicit projection names containing a CALL.
type ExtractedCalls = BTreeMap<String, (CallSubquery, String)>;

pub(super) fn extract(input: &str) -> Result<(String, ExtractedCalls)> {
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
            queries.insert(name, (CallSubquery { query: Box::new(query), imports, outputs },
                input[offset(input, tokens[i])..copied].to_owned()));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    output.push_str(&input[copied..]);
    Ok((output, queries))
}

pub(super) fn restore(query: &mut Query, calls: &mut ExtractedCalls) {
    for clause in &mut query.clauses {
        if let Clause::Call(call) = clause && let Some((subquery, _)) = calls.remove(&call.name) {
            *clause = Clause::Subquery(subquery);
        }
        restore_clause(clause, calls);
    }
    for branch in &mut query.unions { restore(&mut branch.query, calls); }
}

fn restore_clause(clause: &mut Clause, calls: &mut ExtractedCalls) {
    match clause {
        Clause::Subquery(c) => restore(&mut c.query, calls),
        Clause::Match(c) => {
            for p in &mut c.patterns { restore_pattern(p, calls); }
            if let Some(e) = &mut c.predicate { restore_expr(e, calls); }
        }
        Clause::Create(c) => for p in &mut c.patterns { restore_pattern(p, calls); },
        Clause::Merge(c) => {
            restore_pattern(&mut c.pattern, calls);
            for item in c.on_create.iter_mut().chain(&mut c.on_match) { restore_set(item, calls); }
        }
        Clause::Set(c) => for item in &mut c.items { restore_set(item, calls); },
        Clause::Delete(c) => for e in &mut c.expressions { restore_expr(e, calls); },
        Clause::Unwind(c) => restore_expr(&mut c.expr, calls),
        Clause::Call(c) => for e in c.args.iter_mut().chain(&mut c.predicate) { restore_expr(e, calls); },
        Clause::With(c) => {
            restore_projection(&mut c.projection, calls);
            if let Some(e) = &mut c.predicate { restore_expr(e, calls); }
        }
        Clause::Return(c) => restore_projection(&mut c.projection, calls),
    }
}

fn restore_set(item: &mut SetItem, calls: &mut ExtractedCalls) {
    match item {
        SetItem::Property { target, value, .. } => { restore_expr(target, calls); restore_expr(value, calls); }
        SetItem::Replace { value, .. } | SetItem::Merge { value, .. } => restore_expr(value, calls),
        SetItem::Labels { .. } => {}
    }
}

fn restore_projection(body: &mut ProjectionBody, calls: &mut ExtractedCalls) {
    for item in &mut body.items {
        if !item.explicit_alias && let Some(alias) = &mut item.alias {
            for (name, (_, original)) in calls.iter() {
                *alias = alias.replace(&format!("CALL {name}() YIELD __subquery_placeholder"), original);
            }
        }
        restore_expr(&mut item.expr, calls);
    }
    for item in &mut body.order_by { restore_expr(&mut item.expr, calls); }
    for e in body.skip.iter_mut().chain(&mut body.limit) { restore_expr(e, calls); }
}

fn restore_pattern(part: &mut PatternPart, calls: &mut ExtractedCalls) {
    if let Some(e) = &mut part.element.start.properties { restore_expr(e, calls); }
    for chain in &mut part.element.chains {
        for e in chain.node.properties.iter_mut().chain(&mut chain.relationship.properties) { restore_expr(e, calls); }
        if let Some(recursive) = &mut chain.relationship.recursive {
            if let Some(e) = &mut recursive.predicate { restore_expr(e, calls); }
        }
    }
}

fn restore_expr(expr: &mut Expr, calls: &mut ExtractedCalls) {
    match expr {
        Expr::Star | Expr::Variable(_) | Expr::Parameter(_) | Expr::Literal(_) | Expr::CountStar => {}
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => restore_expr(target, calls),
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => restore_expr(expr, calls),
        Expr::Binary { lhs, rhs, .. } => { restore_expr(lhs, calls); restore_expr(rhs, calls); }
        Expr::StringPredicate { target, pattern, .. } => { restore_expr(target, calls); restore_expr(pattern, calls); }
        Expr::List(items) | Expr::Function { args: items, .. } => for e in items { restore_expr(e, calls); },
        Expr::Map(items) => for (_, e) in items { restore_expr(e, calls); },
        Expr::Case { case, arms, otherwise } => {
            for e in case.iter_mut().chain(otherwise) { restore_expr(e, calls); }
            for (a, b) in arms { restore_expr(a, calls); restore_expr(b, calls); }
        }
        Expr::Exists(exists) => {
            if let Some(query) = &mut exists.query { restore(query, calls); }
            for p in &mut exists.patterns { restore_pattern(p, calls); }
            if let Some(e) = &mut exists.predicate { restore_expr(e, calls); }
        }
        Expr::PatternPredicate(parts) => for p in parts { restore_pattern(p, calls); },
        Expr::ListComprehension { collection, predicate, map, .. } => {
            restore_expr(collection, calls);
            if let Some(e) = predicate { restore_expr(e, calls); }
            restore_expr(map, calls);
        }
        Expr::ListReduce { collection, map, .. } | Expr::ListTransform { collection, map, .. } => {
            restore_expr(collection, calls); restore_expr(map, calls);
        }
        Expr::ListFilter { collection, predicate, .. } | Expr::Quantifier { collection, predicate, .. } => {
            restore_expr(collection, calls); restore_expr(predicate, calls);
        }
        Expr::PatternComprehension { pattern, predicate, map, .. } => {
            restore_pattern(pattern, calls);
            if let Some(e) = predicate { restore_expr(e, calls); }
            restore_expr(map, calls);
        }
    }
}
