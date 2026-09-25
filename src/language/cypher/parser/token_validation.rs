//! Structural lexical diagnoses that ANTLR otherwise reports as generic syntax errors.
use super::{CypherParseError, CypherToken, Result};

pub(super) fn validate_source(input: &str) -> Result<()> {
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for ch in chars.by_ref() {
                if ch == '\n' {
                    break;
                }
            }
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(ch) = chars.next() {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else if matches!(ch, '\'' | '"' | '`') {
            let quote = ch;
            while let Some(ch) = chars.next() {
                if ch == quote {
                    if chars.peek() == Some(&quote) {
                        chars.next();
                        continue;
                    }
                    break;
                }
                if ch == '\\' {
                    let escape = chars.next();
                    if quote != '`' && matches!(escape, Some('u' | 'U')) {
                        let digits = if escape == Some('u') { 4 } else { 8 };
                        for _ in 0..digits {
                            if !chars.next().is_some_and(|ch| ch.is_ascii_hexdigit()) {
                                return Err(CypherParseError::InvalidUnicodeLiteral);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn validate(tokens: &[CypherToken]) -> Result<()> {
    let significant = tokens
        .iter()
        .filter(|token| {
            !token.text.trim().is_empty() && !matches!(token.symbolic_name, Some("Comment"))
        })
        .collect::<Vec<_>>();
    for (index, token) in significant.iter().enumerate() {
        if matches!(token.text.as_str(), "—" | "–") {
            return Err(CypherParseError::InvalidUnicodeCharacter);
        }
        if token.text == "["
            && significant
                .get(index + 1)
                .is_some_and(|token| token.text == ",")
        {
            return Err(CypherParseError::Parse(
                "List elements cannot be omitted".into(),
            ));
        }
        if token.symbolic_name == Some("CALL") {
            let rest = &significant[index + 1..];
            let explicit = rest
                .iter()
                .take_while(|token| {
                    !matches!(
                        token.symbolic_name,
                        Some("YIELD" | "RETURN" | "WITH" | "MATCH")
                    )
                })
                .any(|token| token.text == "(");
            let in_query = index > 0
                || rest.iter().any(|token| {
                    matches!(
                        token.symbolic_name,
                        Some("RETURN" | "WITH" | "MATCH" | "CREATE" | "UNWIND")
                    )
                });
            if !explicit && in_query {
                return Err(CypherParseError::InvalidArgumentPassingMode);
            }
        }
        if token.text == "[" && index > 0 && significant[index - 1].text == "-" {
            let mut star = false;
            let mut after_star = false;
            for part in &significant[index + 1..] {
                match part.text.as_str() {
                    "]" | "{" => break,
                    "*" => {
                        star = true;
                        after_star = true;
                    }
                    ".." if !star => return Err(CypherParseError::InvalidRelationshipPattern),
                    "-" if after_star => return Err(CypherParseError::InvalidRelationshipPattern),
                    _ => after_star = false,
                }
            }
        }
    }
    Ok(())
}
