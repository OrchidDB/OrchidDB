//! Structural lexical diagnoses that ANTLR otherwise reports as generic syntax errors.
use super::{CypherParseError, CypherToken, Result};

pub(super) fn validate(tokens: &[CypherToken]) -> Result<()> {
    let significant = tokens
        .iter()
        .filter(|token| {
            !token.text.trim().is_empty() && !matches!(token.symbolic_name, Some("Comment"))
        })
        .collect::<Vec<_>>();
    for (index, token) in significant.iter().enumerate() {
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
