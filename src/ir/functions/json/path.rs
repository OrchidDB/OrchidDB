//! Parsed JSON paths shared by native document functions and row expansion.
//! Array indices are zero based. Negative indices address from the end.
use datafusion::common::{DataFusionError, Result};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Field(String),
    Pointer(String),
    Index(i64),
    Wildcard,
    Recursive(Box<Step>),
    Union(Vec<Step>),
    Slice(Option<i64>, Option<i64>, i64),
    Filter(Predicate),
}
#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    Exists(Vec<Step>),
    Compare(Vec<Step>, String, Value),
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
}
#[derive(Debug, Clone)]
pub struct Match<'a> {
    pub value: &'a Value,
    pub path: String,
    pub parent_path: Option<String>,
    pub key: Option<String>,
    pub index: Option<i64>,
    pub depth: i64,
}
fn error(message: impl std::fmt::Display) -> DataFusionError {
    DataFusionError::Execution(format!("JSON path: {message}"))
}
pub fn field_path(parent: &str, key: &str) -> String {
    format!("{parent}[{}]", serde_json::to_string(key).unwrap())
}
fn split_top(text: &str, token: &str) -> Vec<String> {
    let mut out = vec![];
    let (mut start, mut at, mut depth, mut quote, mut escape) = (0, 0, 0_i64, None, false);
    while at < text.len() {
        let ch = text[at..].chars().next().unwrap();
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if matches!(ch, '(' | '[' | '{') {
            depth += 1;
        } else if matches!(ch, ')' | ']' | '}') {
            depth -= 1;
        } else if depth == 0 && text[at..].starts_with(token) {
            out.push(text[start..at].trim().into());
            at += token.len();
            start = at;
            continue;
        }
        at += ch.len_utf8();
    }
    out.push(text[start..].trim().into());
    out
}
fn quoted(text: &str) -> Result<String> {
    if text.starts_with('"') {
        return serde_json::from_str(text).map_err(error);
    }
    if text.starts_with('\'') && text.ends_with('\'') && text.len() >= 2 {
        let mut json = String::from("\"");
        let mut chars = text[1..text.len() - 1].chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('\'') => json.push('\''),
                    Some(c) => {
                        json.push('\\');
                        json.push(c);
                    }
                    None => return Err(error("unterminated quoted key")),
                },
                '"' => json.push_str("\\\""),
                c => json.push(c),
            }
        }
        json.push('"');
        return serde_json::from_str(&json).map_err(error);
    }
    Err(error("expected quoted key"))
}
fn bracket(text: &str) -> Result<Step> {
    let text = text.trim();
    if text == "*" {
        return Ok(Step::Wildcard);
    }
    if text.starts_with('?') {
        return Ok(Step::Filter(predicate(text[1..].trim())?));
    }
    let union = split_top(text, ",");
    if union.len() > 1 {
        return Ok(Step::Union(
            union.iter().map(|s| bracket(s)).collect::<Result<_>>()?,
        ));
    }
    if text.starts_with('"') || text.starts_with('\'') {
        return Ok(Step::Field(quoted(text)?));
    }
    let slice = split_top(text, ":");
    if slice.len() > 1 {
        if slice.len() > 3 {
            return Err(error("slice requires start:end[:step]"));
        }
        let number = |s: &str| {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<i64>().map(Some).map_err(error)
            }
        };
        let stride = if slice.len() == 3 {
            number(&slice[2])?.unwrap_or(1)
        } else {
            1
        };
        if stride == 0 {
            return Err(error("slice step cannot be zero"));
        }
        return Ok(Step::Slice(number(&slice[0])?, number(&slice[1])?, stride));
    }
    let text = text.strip_prefix('#').unwrap_or(text);
    Ok(Step::Index(text.parse::<i64>().map_err(|_| {
        error(format!("unsupported array selector {text:?}"))
    })?))
}
fn predicate(text: &str) -> Result<Predicate> {
    let mut text = text.trim();
    while text.starts_with('(') && text.ends_with(')') {
        let (mut depth, mut quote, mut escaped, mut outer) = (0, None, false, true);
        for (i, c) in text.char_indices() {
            if let Some(q) = quote {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            } else if c == '"' || c == '\'' {
                quote = Some(c);
            } else if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 && i != text.len() - 1 {
                    outer = false;
                    break;
                }
            }
        }
        if !outer {
            break;
        }
        text = text[1..text.len() - 1].trim();
    }
    for (operator, and) in [("||", false), ("&&", true)] {
        let parts = split_top(text, operator);
        if parts.len() > 1 {
            let mut expressions = parts
                .iter()
                .map(|s| predicate(s))
                .collect::<Result<Vec<_>>>()?
                .into_iter();
            let mut result = expressions.next().unwrap();
            for expression in expressions {
                result = if and {
                    Predicate::And(Box::new(result), Box::new(expression))
                } else {
                    Predicate::Or(Box::new(result), Box::new(expression))
                };
            }
            return Ok(result);
        }
    }
    if let Some(rest) = text.strip_prefix('!') {
        return Ok(Predicate::Not(Box::new(predicate(rest)?)));
    }
    for op in ["==", "!=", "<=", ">=", "<", ">"] {
        let parts = split_top(text, op);
        if parts.len() == 2 {
            let path = parts[0]
                .strip_prefix('@')
                .ok_or_else(|| error("filter left operand must begin with @"))?;
            let rhs = if parts[1].starts_with('\'') {
                Value::String(quoted(&parts[1])?)
            } else {
                serde_json::from_str(&parts[1]).map_err(error)?
            };
            return Ok(Predicate::Compare(
                parse(&format!("${path}"))?,
                op.into(),
                rhs,
            ));
        }
    }
    let path = text
        .strip_prefix('@')
        .ok_or_else(|| error("filter needs @ path and optional comparison"))?;
    Ok(Predicate::Exists(parse(&format!("${path}"))?))
}
pub fn parse(text: &str) -> Result<Vec<Step>> {
    if text.is_empty() {
        return Ok(vec![]);
    }
    if text.starts_with('/') {
        return text[1..]
            .split('/')
            .map(|part| {
                let mut decoded = String::new();
                let mut chars = part.chars();
                while let Some(c) = chars.next() {
                    if c == '~' {
                        decoded.push(match chars.next() {
                            Some('0') => '~',
                            Some('1') => '/',
                            _ => return Err(error("invalid JSON Pointer escape")),
                        });
                    } else {
                        decoded.push(c);
                    }
                }
                // Pointer segments select object keys or array indices at evaluation.
                Ok(Step::Pointer(decoded))
            })
            .collect();
    }
    let text = text
        .strip_prefix('$')
        .ok_or_else(|| error("path must begin with $ or /"))?;
    let mut steps = vec![];
    let mut at = 0;
    while at < text.len() {
        match text.as_bytes()[at] {
            b'.' => {
                at += 1;
                let recursive = at < text.len() && text.as_bytes()[at] == b'.';
                if recursive {
                    at += 1;
                }
                if text[at..].starts_with('"') {
                    let mut stream =
                        serde_json::Deserializer::from_str(&text[at..]).into_iter::<String>();
                    let field = stream
                        .next()
                        .ok_or_else(|| error("unterminated quoted field"))?
                        .map_err(error)?;
                    at += stream.byte_offset();
                    let step = Step::Field(field);
                    steps.push(if recursive {
                        Step::Recursive(Box::new(step))
                    } else {
                        step
                    });
                    continue;
                }
                let start = at;
                while at < text.len() && !matches!(text.as_bytes()[at], b'.' | b'[' | b'?') {
                    at += 1;
                }
                let word = &text[start..at];
                if word.is_empty() {
                    return Err(error("empty field selector"));
                }
                let step = if word == "*" {
                    Step::Wildcard
                } else if word
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '$')
                {
                    Step::Field(word.into())
                } else {
                    return Err(error("quote field names containing punctuation"));
                };
                steps.push(if recursive {
                    Step::Recursive(Box::new(step))
                } else {
                    step
                });
            }
            b'[' => {
                let start = at + 1;
                at += 1;
                let (mut depth, mut quote, mut escape) = (1, None, false);
                while at < text.len() {
                    let ch = text[at..].chars().next().unwrap();
                    if let Some(q) = quote {
                        if escape {
                            escape = false;
                        } else if ch == '\\' {
                            escape = true;
                        } else if ch == q {
                            quote = None;
                        }
                    } else if ch == '\'' || ch == '"' {
                        quote = Some(ch);
                    } else if ch == '[' {
                        depth += 1;
                    } else if ch == ']' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    at += ch.len_utf8();
                }
                if depth != 0 {
                    return Err(error("unclosed selector"));
                }
                steps.push(bracket(&text[start..at])?);
                at += 1;
            }
            _ => {
                return Err(error(format!(
                    "unsupported path syntax near {:?}",
                    &text[at..]
                )));
            }
        }
    }
    Ok(steps)
}
pub fn singular(steps: &[Step]) -> bool {
    steps
        .iter()
        .all(|s| matches!(s, Step::Field(_) | Step::Pointer(_) | Step::Index(_)))
}
pub fn select<'a>(value: &'a Value, path: &str) -> Result<Vec<Match<'a>>> {
    select_steps(value, &parse(path)?)
}
pub fn select_steps<'a>(value: &'a Value, steps: &[Step]) -> Result<Vec<Match<'a>>> {
    let mut values = vec![Match {
        value,
        path: "$".into(),
        parent_path: None,
        key: None,
        index: None,
        depth: 0,
    }];
    for step in steps {
        let mut next = vec![];
        for value in values {
            apply(&value, step, &mut next)?;
        }
        values = next;
    }
    Ok(values)
}
fn index(value: i64, len: usize) -> Option<usize> {
    let n = if value < 0 { len as i64 + value } else { value };
    (n >= 0 && (n as usize) < len).then_some(n as usize)
}
fn child<'a>(
    parent: &Match<'a>,
    value: &'a Value,
    key: Option<String>,
    index: Option<i64>,
) -> Match<'a> {
    Match {
        value,
        path: if let Some(key) = &key {
            field_path(&parent.path, key)
        } else {
            format!("{}[{}]", parent.path, index.unwrap())
        },
        parent_path: Some(parent.path.clone()),
        key,
        index,
        depth: parent.depth + 1,
    }
}
pub fn children<'a>(parent: &Match<'a>) -> Vec<Match<'a>> {
    match parent.value {
        Value::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, v)| child(parent, v, None, Some(i as i64)))
            .collect(),
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| child(parent, v, Some(k.clone()), None))
            .collect(),
        _ => vec![],
    }
}
fn apply<'a>(m: &Match<'a>, step: &Step, out: &mut Vec<Match<'a>>) -> Result<()> {
    match step {
        Step::Field(k) | Step::Pointer(k) => {
            if let Some(v) = m.value.as_object().and_then(|o| o.get(k)) {
                out.push(child(m, v, Some(k.clone()), None));
            } else if let (Step::Pointer(_), Some(a), Ok(i)) =
                (step, m.value.as_array(), k.parse::<i64>())
            {
                if i >= 0 && i.to_string() == *k {
                    if let Some(i) = index(i, a.len()) {
                        out.push(child(m, &a[i], None, Some(i as i64)));
                    }
                }
            }
        }
        Step::Index(i) => {
            if let Some(a) = m.value.as_array() {
                if let Some(i) = index(*i, a.len()) {
                    out.push(child(m, &a[i], None, Some(i as i64)));
                }
            }
        }
        Step::Wildcard => out.extend(children(m)),
        Step::Recursive(s) => {
            apply(m, s, out)?;
            for c in children(m) {
                apply(&c, step, out)?;
            }
        }
        Step::Union(steps) => {
            for s in steps {
                apply(m, s, out)?;
            }
        }
        Step::Slice(start, end, stride) => {
            if let Some(a) = m.value.as_array() {
                let len = a.len() as i64;
                let clamp = |n: i64| {
                    if n < 0 {
                        (n + len).max(if *stride < 0 { -1 } else { 0 })
                    } else {
                        n.min(if *stride < 0 { len - 1 } else { len })
                    }
                };
                let mut at = start
                    .map(clamp)
                    .unwrap_or(if *stride < 0 { len - 1 } else { 0 });
                let end = end.map(clamp).unwrap_or(if *stride < 0 { -1 } else { len });
                while if *stride > 0 { at < end } else { at > end } {
                    if at >= 0 && at < len {
                        out.push(child(m, &a[at as usize], None, Some(at)));
                    }
                    at += stride;
                }
            }
        }
        Step::Filter(p) => {
            for c in children(m) {
                if matches(c.value, p)? {
                    out.push(c);
                }
            }
        }
    }
    Ok(())
}
fn matches(value: &Value, p: &Predicate) -> Result<bool> {
    Ok(match p {
        Predicate::Exists(path) => !select_steps(value, path)?.is_empty(),
        Predicate::Compare(path, op, rhs) => select_steps(value, path)?.iter().any(|m| {
            let cmp = super::compare(m.value, rhs);
            match op.as_str() {
                "==" => cmp == Some(std::cmp::Ordering::Equal),
                "!=" => cmp != Some(std::cmp::Ordering::Equal),
                "<" => cmp == Some(std::cmp::Ordering::Less),
                ">" => cmp == Some(std::cmp::Ordering::Greater),
                "<=" => matches!(
                    cmp,
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                ),
                ">=" => matches!(
                    cmp,
                    Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                ),
                _ => false,
            }
        }),
        Predicate::And(a, b) => matches(value, a)? && matches(value, b)?,
        Predicate::Or(a, b) => matches(value, a)? || matches(value, b)?,
        Predicate::Not(p) => !matches(value, p)?,
    })
}
