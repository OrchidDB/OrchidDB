use crate::grammar::generated::cypher::cypherparser::{
    OC_ListPredicateExpressionContext, OC_ListPredicateExpressionContextAttrs,
    OC_NullPredicateExpressionContext, OC_NullPredicateExpressionContextAttrs,
    OC_PartialComparisonExpressionContext, OC_PartialComparisonExpressionContextAttrs,
    OC_PatternPredicateContext, OC_PatternPredicateContextAttrs,
    OC_StringPredicateExpressionContext, OC_StringPredicateExpressionContextAttrs,
};
use crate::language::cypher::ast::{BinaryOp, Expr, Literal, StringPredicateOp};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

use super::{context, expressions, patterns};

pub(crate) fn lower_partial_comparison(
    ctx: &OC_PartialComparisonExpressionContext<'_>,
) -> Result<(BinaryOp, Expr)> {
    let Some(rhs) = ctx.oC_StringListNullPredicateExpression() else {
        return context::missing("partial comparison missing right-hand expression");
    };
    Ok((
        expressions::partial_comparison_op(ctx)?,
        expressions::lower_string_list_null_predicate_expression(rhs.as_ref())?,
    ))
}

pub(crate) fn lower_string_predicate_expression(
    ctx: &OC_StringPredicateExpressionContext<'_>,
) -> Result<(StringPredicateOp, Expr)> {
    let op = if ctx.get_text().trim_start().starts_with("=~") {
        StringPredicateOp::Regex
    } else if ctx.STARTS().is_some() {
        StringPredicateOp::StartsWith
    } else if ctx.ENDS().is_some() {
        StringPredicateOp::EndsWith
    } else {
        StringPredicateOp::Contains
    };
    let Some(rhs) = ctx.oC_AddOrSubtractExpression() else {
        return context::missing("string predicate missing right-hand expression");
    };
    Ok((
        op,
        expressions::lower_add_or_subtract_expression(rhs.as_ref())?,
    ))
}

pub(crate) fn lower_list_predicate_expression(
    ctx: &OC_ListPredicateExpressionContext<'_>,
) -> Result<Expr> {
    let Some(rhs) = ctx.oC_AddOrSubtractExpression() else {
        return context::missing("IN predicate missing collection expression");
    };
    expressions::lower_add_or_subtract_expression(rhs.as_ref())
}

pub(crate) fn lower_null_predicate_expression(
    ctx: &OC_NullPredicateExpressionContext<'_>,
) -> Result<Expr> {
    Ok(Expr::Literal(Literal::Bool(ctx.NOT().is_some())))
}

pub(crate) fn lower_pattern_predicate(ctx: &OC_PatternPredicateContext<'_>) -> Result<Expr> {
    let Some(pattern) = ctx.oC_RelationshipsPattern() else {
        return context::missing("pattern predicate missing relationships pattern");
    };
    Ok(Expr::PatternPredicate(vec![
        patterns::lower_relationships_pattern(pattern.as_ref())?,
    ]))
}
