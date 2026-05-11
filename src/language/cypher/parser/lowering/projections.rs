use std::rc::Rc;

use crate::grammar::generated::cypher::cypherparser::{
    Cypher_T__4, OC_LimitContext, OC_LimitContextAttrs, OC_OrderContext, OC_OrderContextAttrs,
    OC_ProjectionBodyContext, OC_ProjectionBodyContextAttrs, OC_ProjectionItemContext,
    OC_ProjectionItemContextAttrs, OC_ProjectionItemsContext, OC_ProjectionItemsContextAttrs,
    OC_SkipContext, OC_SkipContextAttrs, OC_SortItemContext, OC_SortItemContextAttrs,
};
use crate::language::cypher::ast::{Expr, ProjectionBody, ProjectionItem, SortDirection, SortItem};
use crate::language::cypher::parser::Result;
use antlr4rust::parser_rule_context::ParserRuleContext;
use antlr4rust::tree::ParseTree;

use super::{context, expressions, names};

pub(crate) fn lower_projection_body_required(
    ctx: Option<Rc<OC_ProjectionBodyContext<'_>>>,
) -> Result<ProjectionBody> {
    let Some(ctx) = ctx else {
        return context::missing("projection body missing from Cypher clause");
    };
    lower_projection_body(ctx.as_ref())
}

pub(crate) fn lower_projection_body(ctx: &OC_ProjectionBodyContext<'_>) -> Result<ProjectionBody> {
    let items_ctx = ctx.oC_ProjectionItems();
    let include_existing = items_ctx
        .as_ref()
        .and_then(|items| items.get_token(Cypher_T__4, 0))
        .is_some();
    let items = items_ctx
        .map(|items| lower_projection_items(items.as_ref()))
        .transpose()?
        .unwrap_or_default();
    if !include_existing && items.is_empty() {
        return context::missing("projection body missing projection items");
    }
    Ok(ProjectionBody {
        distinct: ctx.DISTINCT().is_some(),
        include_existing,
        items,
        order_by: ctx
            .oC_Order()
            .map(|order| lower_order(order.as_ref()))
            .transpose()?
            .unwrap_or_default(),
        skip: ctx
            .oC_Skip()
            .map(|skip| lower_skip(skip.as_ref()))
            .transpose()?,
        limit: ctx
            .oC_Limit()
            .map(|limit| lower_limit(limit.as_ref()))
            .transpose()?,
    })
}

pub(crate) fn lower_projection_items(
    ctx: &OC_ProjectionItemsContext<'_>,
) -> Result<Vec<ProjectionItem>> {
    ctx.oC_ProjectionItem_all()
        .into_iter()
        .map(|item| lower_projection_item(item.as_ref()))
        .collect()
}

pub(crate) fn lower_projection_item(ctx: &OC_ProjectionItemContext<'_>) -> Result<ProjectionItem> {
    let Some(expr) = ctx.oC_Expression() else {
        return context::missing("projection item missing expression");
    };
    let default_alias = expr.get_text();
    let explicit_alias = ctx.AS().is_some();
    let expr = expressions::lower_expression(expr.as_ref())?;
    let alias = if explicit_alias {
        let Some(alias) = ctx.oC_Variable() else {
            return context::missing("projection item missing alias after AS");
        };
        Some(names::clean_identifier(&alias.get_text()))
    } else {
        expr.variable_name()
            .map(ToString::to_string)
            .or(Some(default_alias))
    };
    Ok(ProjectionItem {
        expr,
        alias,
        explicit_alias,
    })
}

pub(crate) fn lower_order(ctx: &OC_OrderContext<'_>) -> Result<Vec<SortItem>> {
    ctx.oC_SortItem_all()
        .into_iter()
        .map(|item| lower_sort_item(item.as_ref()))
        .collect()
}

pub(crate) fn lower_sort_item(ctx: &OC_SortItemContext<'_>) -> Result<SortItem> {
    let Some(expr) = ctx.oC_Expression() else {
        return context::missing("sort item missing expression");
    };
    let expr = expressions::lower_expression(expr.as_ref())?;
    let direction = if ctx.DESC().is_some() || ctx.DESCENDING().is_some() {
        SortDirection::Desc
    } else {
        SortDirection::Asc
    };
    Ok(SortItem { expr, direction })
}

pub(crate) fn lower_skip(ctx: &OC_SkipContext<'_>) -> Result<Expr> {
    ctx.oC_Expression()
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?
        .ok_or_else(|| {
            crate::language::cypher::parser::CypherParseError::Parse(
                "SKIP missing expression".to_string(),
            )
        })
}

pub(crate) fn lower_limit(ctx: &OC_LimitContext<'_>) -> Result<Expr> {
    ctx.oC_Expression()
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?
        .ok_or_else(|| {
            crate::language::cypher::parser::CypherParseError::Parse(
                "LIMIT missing expression".to_string(),
            )
        })
}
