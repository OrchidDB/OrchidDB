use antlr4rust::parser_rule_context::ParserRuleContext;
use antlr4rust::token::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceSpan {
    pub line: isize,
    pub column: isize,
}

pub(crate) fn start_span<'input, C>(ctx: &C) -> Option<SourceSpan>
where
    C: ParserRuleContext<'input>,
{
    let token = ctx.start();
    let line = token.get_line();
    let column = token.get_column();
    if line <= 0 {
        return None;
    }
    Some(SourceSpan { line, column })
}
