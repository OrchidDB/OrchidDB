use crate::grammar::generated::cypher::cypherparser::{
    OC_CaseAlternativeContext, OC_CaseExpressionContext,
};
use crate::language::cypher::ast::Expr;
use crate::language::cypher::parser::Result;

use super::expressions;

pub(crate) fn lower_case_expression(ctx: &OC_CaseExpressionContext<'_>) -> Result<Expr> {
    expressions::lower_case_expression(ctx)
}

pub(crate) fn lower_case_alternative(ctx: &OC_CaseAlternativeContext<'_>) -> Result<(Expr, Expr)> {
    expressions::lower_case_alternative(ctx)
}
