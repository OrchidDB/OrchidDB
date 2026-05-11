use crate::grammar::generated::cypher::cypherparser::{
    Cypher_T__11, OC_DashContext, OC_IntegerLiteralContext, OC_LeftArrowHeadContext,
    OC_RangeLiteralContext, OC_RangeLiteralContextAttrs, OC_RightArrowHeadContext,
};
use crate::ir::plan::Direction;
use crate::language::cypher::ast::RangeLiteral;
use crate::language::cypher::parser::Result;
use antlr4rust::parser_rule_context::ParserRuleContext;
use antlr4rust::token::Token;

use super::expressions;

pub(crate) fn lower_range_literal<'input>(
    ctx: &OC_RangeLiteralContext<'input>,
) -> Result<RangeLiteral> {
    let bounds = ctx.oC_IntegerLiteral_all();
    let Some(dot_dot) = ctx.get_token(Cypher_T__11, 0) else {
        return match bounds.as_slice() {
            [] => Ok(RangeLiteral { min: 1, max: None }),
            [exact] => {
                let exact = parse_bound(exact.as_ref())?;
                Ok(RangeLiteral {
                    min: exact,
                    max: Some(exact),
                })
            }
            _ => Err(crate::language::cypher::parser::CypherParseError::Parse(
                "relationship range has too many bounds".to_string(),
            )),
        };
    };

    let dot_index = dot_dot.symbol.get_token_index();
    let min = bounds
        .iter()
        .find(|bound| bound.start().get_token_index() < dot_index)
        .map(|bound| parse_bound(bound.as_ref()))
        .transpose()?
        .unwrap_or(1);
    let max = bounds
        .iter()
        .find(|bound| bound.start().get_token_index() > dot_index)
        .map(|bound| parse_bound(bound.as_ref()))
        .transpose()?;
    if let Some(max_bound) = max {
        if min > max_bound {
            return Err(crate::language::cypher::parser::CypherParseError::Parse(
                format!("relationship range lower bound {min} exceeds upper bound {max_bound}"),
            ));
        }
    }
    Ok(RangeLiteral { min, max })
}

pub(crate) fn lower_left_arrow_head(_ctx: &OC_LeftArrowHeadContext<'_>) -> Result<()> {
    Ok(())
}

pub(crate) fn lower_right_arrow_head(_ctx: &OC_RightArrowHeadContext<'_>) -> Result<()> {
    Ok(())
}

pub(crate) fn lower_dash(_ctx: &OC_DashContext<'_>) -> Result<()> {
    Ok(())
}

pub(crate) fn relationship_direction(has_left: bool, has_right: bool) -> Direction {
    match (has_left, has_right) {
        (true, false) => Direction::In,
        (false, true) => Direction::Out,
        _ => Direction::Both,
    }
}

fn parse_bound(ctx: &OC_IntegerLiteralContext<'_>) -> Result<u32> {
    let text = expressions::lower_integer_literal(ctx)?;
    text.parse::<u32>().map_err(|err| {
        crate::language::cypher::parser::CypherParseError::Parse(format!(
            "invalid relationship range bound `{text}`: {err}"
        ))
    })
}
