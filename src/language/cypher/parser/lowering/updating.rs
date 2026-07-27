use crate::grammar::generated::cypher::cypherparser::{
    OC_CreateContext, OC_CreateContextAttrs, OC_DeleteContext, OC_DeleteContextAttrs,
    OC_MergeActionContext, OC_MergeActionContextAttrs, OC_MergeContext, OC_MergeContextAttrs,
    OC_NodeLabelsContextAttrs, OC_RemoveContext,
    OC_RemoveItemContext, OC_SetContext, OC_SetContextAttrs, OC_SetItemContext,
    OC_SetItemContextAttrs, OC_UpdatingClauseContext, OC_UpdatingClauseContextAttrs,
};
use crate::language::cypher::ast::{
    Clause, CreateClause, DeleteClause, Expr, MergeClause, SetClause, SetItem,
};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

use super::{context, expressions, names, patterns};

pub(crate) fn lower_updating_clause(ctx: &OC_UpdatingClauseContext<'_>) -> Result<Clause> {
    if let Some(create) = ctx.oC_Create() {
        return lower_create(create.as_ref());
    }
    if let Some(merge) = ctx.oC_Merge() {
        return lower_merge(merge.as_ref());
    }
    if let Some(delete) = ctx.oC_Delete() {
        return lower_delete(delete.as_ref());
    }
    if let Some(set) = ctx.oC_Set() {
        return lower_set(set.as_ref());
    }
    if let Some(remove) = ctx.oC_Remove() {
        return lower_remove(remove.as_ref());
    }
    context::missing("updating clause missing CREATE, MERGE, DELETE, SET, or REMOVE")
}

pub(crate) fn lower_create(ctx: &OC_CreateContext<'_>) -> Result<Clause> {
    let Some(pattern) = ctx.oC_Pattern() else {
        return context::missing("CREATE missing pattern");
    };
    Ok(Clause::Create(CreateClause {
        patterns: patterns::lower_pattern(pattern.as_ref())?,
    }))
}

pub(crate) fn lower_merge(ctx: &OC_MergeContext<'_>) -> Result<Clause> {
    let Some(pattern) = ctx.oC_PatternPart() else {
        return context::missing("MERGE missing pattern");
    };
    let mut on_create = Vec::new();
    let mut on_match = Vec::new();
    for action in ctx.oC_MergeAction_all() {
        let (kind, items) = lower_merge_action(action.as_ref())?;
        match kind {
            MergeActionKind::Create => on_create.extend(items),
            MergeActionKind::Match => on_match.extend(items),
        }
    }
    Ok(Clause::Merge(MergeClause {
        pattern: patterns::lower_pattern_part(pattern.as_ref())?,
        on_create,
        on_match,
    }))
}

pub(crate) enum MergeActionKind {
    Create,
    Match,
}

pub(crate) fn lower_merge_action(
    ctx: &OC_MergeActionContext<'_>,
) -> Result<(MergeActionKind, Vec<SetItem>)> {
    let kind = if ctx.CREATE().is_some() {
        MergeActionKind::Create
    } else if ctx.MATCH().is_some() {
        MergeActionKind::Match
    } else {
        return context::missing("MERGE action missing ON CREATE / ON MATCH");
    };
    let Some(set) = ctx.oC_Set() else {
        return context::missing("MERGE action missing SET");
    };
    let items = set
        .oC_SetItem_all()
        .into_iter()
        .map(|item| lower_set_item(item.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    Ok((kind, items))
}

pub(crate) fn lower_set(ctx: &OC_SetContext<'_>) -> Result<Clause> {
    let items = ctx
        .oC_SetItem_all()
        .into_iter()
        .map(|item| lower_set_item(item.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    Ok(Clause::Set(SetClause { items }))
}

pub(crate) fn lower_set_item(ctx: &OC_SetItemContext<'_>) -> Result<SetItem> {
    if let Some(property) = ctx.oC_PropertyExpression() {
        let lhs = expressions::lower_property_expression(property.as_ref())?;
        let Some(value) = ctx.oC_Expression() else {
            return context::missing("SET property item missing value expression");
        };
        let (target, key) = split_property_target(lhs)?;
        return Ok(SetItem::Property {
            target,
            key,
            value: expressions::lower_expression(value.as_ref())?,
        });
    }

    let Some(variable) = ctx.oC_Variable() else {
        return context::missing("SET item missing variable or property expression");
    };
    let variable = names::clean_identifier(&variable.get_text());
    if let Some(labels) = ctx.oC_NodeLabels() {
        let mut lowered = Vec::new();
        for label in labels.oC_NodeLabel_all() {
            lowered.extend(names::lower_node_label_names(label.as_ref())?);
        }
        return Ok(SetItem::Labels {
            variable,
            labels: lowered,
        });
    }

    let Some(value) = ctx.oC_Expression() else {
        return context::missing("SET variable item missing value expression");
    };
    let value = expressions::lower_expression(value.as_ref())?;
    if ctx.get_text().contains("+=") {
        Ok(SetItem::Merge { variable, value })
    } else {
        Ok(SetItem::Replace { variable, value })
    }
}

pub(crate) fn lower_delete(ctx: &OC_DeleteContext<'_>) -> Result<Clause> {
    let expressions = ctx
        .oC_Expression_all()
        .into_iter()
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    if expressions.is_empty() {
        return context::missing("DELETE missing expression");
    }
    Ok(Clause::Delete(DeleteClause {
        detach: ctx.DETACH().is_some(),
        expressions,
    }))
}

pub(crate) fn lower_remove(_ctx: &OC_RemoveContext<'_>) -> Result<Clause> {
    context::unsupported("REMOVE is outside read and side-effect query lowering")
}

pub(crate) fn lower_remove_item(_ctx: &OC_RemoveItemContext<'_>) -> Result<()> {
    context::unsupported("REMOVE items are outside read and side-effect query lowering")
}

fn split_property_target(expr: Expr) -> Result<(Expr, String)> {
    match expr {
        Expr::Property { target, key } => Ok((*target, key)),
        _ => context::unsupported("SET property assignment requires a property lookup target"),
    }
}
