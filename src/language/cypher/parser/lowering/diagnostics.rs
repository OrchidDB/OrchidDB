use crate::language::cypher::parser::{CypherParseError, Result};

use super::source::SourceSpan;

pub(crate) fn unsupported_at<T>(span: Option<SourceSpan>, message: impl Into<String>) -> Result<T> {
    Err(CypherParseError::Unsupported(format_with_span(
        span,
        message.into(),
    )))
}

pub(crate) fn parse_error_at<T>(span: Option<SourceSpan>, message: impl Into<String>) -> Result<T> {
    Err(CypherParseError::Parse(format_with_span(
        span,
        message.into(),
    )))
}

pub(crate) fn format_with_span(span: Option<SourceSpan>, message: String) -> String {
    match span {
        Some(span) => format!("{message} at {}:{}", span.line, span.column),
        None => message,
    }
}
