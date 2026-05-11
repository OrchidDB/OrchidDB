use crate::grammar::generated::cypher::cypherparser::{
    OC_AddOrSubtractExpressionContext, OC_MultiplyDivideModuloExpressionContext,
    OC_PowerOfExpressionContext, OC_UnaryAddOrSubtractExpressionContext,
};
use crate::language::cypher::ast::Expr;
use crate::language::cypher::parser::Result;

use super::expressions;

pub(crate) fn lower_add_or_subtract_expression(
    ctx: &OC_AddOrSubtractExpressionContext<'_>,
) -> Result<Expr> {
    expressions::lower_add_or_subtract_expression(ctx)
}

pub(crate) fn lower_multiply_divide_modulo_expression(
    ctx: &OC_MultiplyDivideModuloExpressionContext<'_>,
) -> Result<Expr> {
    expressions::lower_multiply_divide_modulo_expression(ctx)
}

pub(crate) fn lower_power_of_expression(ctx: &OC_PowerOfExpressionContext<'_>) -> Result<Expr> {
    expressions::lower_power_expression(ctx)
}

pub(crate) fn lower_unary_add_or_subtract_expression(
    ctx: &OC_UnaryAddOrSubtractExpressionContext<'_>,
) -> Result<Expr> {
    expressions::lower_unary_add_or_subtract_expression(ctx)
}
