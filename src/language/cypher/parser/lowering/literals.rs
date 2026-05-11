use crate::grammar::generated::cypher::cypherparser::{
    OC_IntegerLiteralContext, OC_LiteralContext, OC_MapLiteralContext, OC_ParameterContext,
};
use crate::language::cypher::ast::{Expr, Literal};
use crate::language::cypher::parser::Result;

use super::expressions;

pub(crate) fn lower_literal(ctx: &OC_LiteralContext<'_>) -> Result<Literal> {
    match expressions::lower_literal_expr(ctx)? {
        Expr::Literal(lit) => Ok(lit),
        other => Err(crate::language::cypher::parser::CypherParseError::Parse(
            format!("literal rule lowered to non-literal expression: {other:?}"),
        )),
    }
}

pub(crate) fn lower_integer_literal(ctx: &OC_IntegerLiteralContext<'_>) -> Result<String> {
    expressions::lower_integer_literal(ctx)
}

pub(crate) fn lower_map_literal(ctx: &OC_MapLiteralContext<'_>) -> Result<Expr> {
    expressions::lower_map_literal(ctx)
}

pub(crate) fn lower_parameter(ctx: &OC_ParameterContext<'_>) -> Result<Expr> {
    expressions::lower_parameter(ctx)
}
