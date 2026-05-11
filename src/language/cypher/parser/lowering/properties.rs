use crate::grammar::generated::cypher::cypherparser::{
    OC_ListOperatorExpressionContext, OC_NonArithmeticOperatorExpressionContext,
    OC_PropertiesContext, OC_PropertiesContextAttrs, OC_PropertyExpressionContext,
    OC_PropertyKeyNameContext, OC_PropertyLookupContext,
};
use crate::language::cypher::ast::Expr;
use crate::language::cypher::parser::Result;

use super::{context, expressions};

pub(crate) fn lower_property_expression(ctx: &OC_PropertyExpressionContext<'_>) -> Result<Expr> {
    expressions::lower_property_expression(ctx)
}

pub(crate) fn lower_non_arithmetic_operator_expression(
    ctx: &OC_NonArithmeticOperatorExpressionContext<'_>,
) -> Result<Expr> {
    expressions::lower_non_arithmetic_operator_expression(ctx)
}

pub(crate) fn lower_list_operator_expression(
    _ctx: &OC_ListOperatorExpressionContext<'_>,
) -> Result<Expr> {
    context::unsupported("list operators require their target expression during lowering")
}

pub(crate) fn lower_property_lookup(ctx: &OC_PropertyLookupContext<'_>) -> Result<String> {
    expressions::lower_property_lookup(ctx)
}

pub(crate) fn lower_property_key_name(ctx: &OC_PropertyKeyNameContext<'_>) -> Result<String> {
    expressions::lower_property_key_name(ctx)
}

pub(crate) fn lower_properties(ctx: &OC_PropertiesContext<'_>) -> Result<Expr> {
    if let Some(map) = ctx.oC_MapLiteral() {
        return expressions::lower_map_literal(map.as_ref());
    }
    if let Some(parameter) = ctx.oC_Parameter() {
        return expressions::lower_parameter(parameter.as_ref());
    }
    context::missing("properties missing map literal or parameter")
}
