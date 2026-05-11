use crate::language::cypher::ast::{BinaryOp, StringPredicateOp, UnaryOp};
use crate::language::cypher::parser::{CypherParseError, Result};

pub(crate) fn binary_operator(text: &str) -> Result<BinaryOp> {
    match text.trim().to_ascii_uppercase().as_str() {
        "OR" => Ok(BinaryOp::Or),
        "AND" => Ok(BinaryOp::And),
        "=" => Ok(BinaryOp::Eq),
        "<>" | "!=" => Ok(BinaryOp::Neq),
        "<" => Ok(BinaryOp::Lt),
        "<=" => Ok(BinaryOp::Lte),
        ">" => Ok(BinaryOp::Gt),
        ">=" => Ok(BinaryOp::Gte),
        "+" => Ok(BinaryOp::Add),
        "-" => Ok(BinaryOp::Sub),
        "*" => Ok(BinaryOp::Mul),
        "/" => Ok(BinaryOp::Div),
        other => Err(CypherParseError::Unsupported(format!(
            "unknown Cypher binary operator `{other}`"
        ))),
    }
}

pub(crate) fn unary_operator(text: &str) -> Result<UnaryOp> {
    match text.trim().to_ascii_uppercase().as_str() {
        "NOT" => Ok(UnaryOp::Not),
        "-" => Ok(UnaryOp::Neg),
        other => Err(CypherParseError::Unsupported(format!(
            "unknown Cypher unary operator `{other}`"
        ))),
    }
}

pub(crate) fn string_predicate_operator(text: &str) -> Result<StringPredicateOp> {
    match text.trim().to_ascii_uppercase().as_str() {
        "STARTS WITH" | "STARTSWITH" => Ok(StringPredicateOp::StartsWith),
        "ENDS WITH" | "ENDSWITH" => Ok(StringPredicateOp::EndsWith),
        "CONTAINS" => Ok(StringPredicateOp::Contains),
        "=~" => Ok(StringPredicateOp::Regex),
        other => Err(CypherParseError::Unsupported(format!(
            "unknown Cypher string predicate `{other}`"
        ))),
    }
}
