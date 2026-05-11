use crate::grammar::generated::cypher::cypherparser::{
    OC_FilterExpressionContext, OC_FilterExpressionContextAttrs, OC_IdInCollContext,
    OC_IdInCollContextAttrs, OC_ListComprehensionContext, OC_ListComprehensionContextAttrs,
    OC_ListLiteralContext, OC_ListLiteralContextAttrs, OC_PatternComprehensionContext,
    OC_PatternComprehensionContextAttrs, OC_QuantifierContext, OC_QuantifierContextAttrs,
    OC_WhereContextAttrs,
};
use crate::language::cypher::ast::{Expr, Literal, QuantifierKind};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

use super::{context, expressions, names, patterns};

pub(crate) fn lower_list_literal(ctx: &OC_ListLiteralContext<'_>) -> Result<Expr> {
    Ok(Expr::List(
        ctx.oC_Expression_all()
            .into_iter()
            .map(|expr| expressions::lower_expression(expr.as_ref()))
            .collect::<Result<Vec<_>>>()?,
    ))
}

pub(crate) fn lower_list_comprehension(ctx: &OC_ListComprehensionContext<'_>) -> Result<Expr> {
    let Some(filter) = ctx.oC_FilterExpression() else {
        return context::missing("list comprehension missing filter expression");
    };
    let (item, collection, predicate) = lower_filter_parts(filter.as_ref())?;
    let map = ctx
        .oC_Expression()
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?
        .unwrap_or_else(|| Expr::Variable(item.clone()));
    Ok(Expr::ListComprehension {
        variable: item,
        collection: Box::new(collection),
        predicate: predicate.map(Box::new),
        map: Box::new(map),
    })
}

pub(crate) fn lower_pattern_comprehension(
    ctx: &OC_PatternComprehensionContext<'_>,
) -> Result<Expr> {
    let path_binding = ctx
        .oC_Variable()
        .map(|var| names::clean_identifier(&var.get_text()))
        .unwrap_or_default();
    let Some(pattern_ctx) = ctx.oC_RelationshipsPattern() else {
        return context::missing("pattern comprehension missing relationships pattern");
    };
    let predicate = ctx
        .oC_Where()
        .and_then(|where_ctx| where_ctx.oC_Expression())
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?;
    let Some(projection) = ctx.oC_Expression() else {
        return context::missing("pattern comprehension missing projection expression");
    };
    let projection = expressions::lower_expression(projection.as_ref())?;
    let mut pattern = patterns::lower_relationships_pattern(pattern_ctx.as_ref())?;
    if !path_binding.is_empty() {
        pattern.variable = Some(path_binding.clone());
    }
    Ok(Expr::PatternComprehension {
        variable: if path_binding.is_empty() {
            None
        } else {
            Some(path_binding)
        },
        pattern: Box::new(pattern),
        predicate: predicate.map(Box::new),
        map: Box::new(projection),
    })
}

pub(crate) fn lower_quantifier(ctx: &OC_QuantifierContext<'_>) -> Result<Expr> {
    let Some(filter) = ctx.oC_FilterExpression() else {
        return context::missing("quantifier missing filter expression");
    };
    let (item, collection, predicate) = lower_filter_parts(filter.as_ref())?;
    let kind = if ctx.ALL().is_some() {
        QuantifierKind::All
    } else if ctx.ANY().is_some() {
        QuantifierKind::Any
    } else if ctx.NONE().is_some() {
        QuantifierKind::None
    } else {
        QuantifierKind::Single
    };
    Ok(Expr::Quantifier {
        kind,
        variable: item,
        collection: Box::new(collection),
        predicate: Box::new(predicate.unwrap_or(Expr::Literal(Literal::Bool(true)))),
    })
}

pub(crate) fn lower_filter_expression(ctx: &OC_FilterExpressionContext<'_>) -> Result<Expr> {
    let (item, collection, predicate) = lower_filter_parts(ctx)?;
    Ok(Expr::Function {
        name: "filter".to_string(),
        distinct: false,
        args: vec![
            Expr::Literal(Literal::String(item)),
            collection,
            predicate.unwrap_or(Expr::Literal(Literal::Bool(true))),
        ],
    })
}

pub(crate) fn lower_id_in_coll(ctx: &OC_IdInCollContext<'_>) -> Result<(String, Expr)> {
    let Some(variable) = ctx.oC_Variable() else {
        return context::missing("id-in-collection missing variable");
    };
    let Some(collection) = ctx.oC_Expression() else {
        return context::missing("id-in-collection missing collection expression");
    };
    Ok((
        names::clean_identifier(&variable.get_text()),
        expressions::lower_expression(collection.as_ref())?,
    ))
}

fn lower_filter_parts(
    ctx: &OC_FilterExpressionContext<'_>,
) -> Result<(String, Expr, Option<Expr>)> {
    let Some(id_in_coll) = ctx.oC_IdInColl() else {
        return context::missing("filter expression missing id-in-collection");
    };
    let (item, collection) = lower_id_in_coll(id_in_coll.as_ref())?;
    let predicate = ctx
        .oC_Where()
        .and_then(|where_ctx| where_ctx.oC_Expression())
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?;
    Ok((item, collection, predicate))
}
