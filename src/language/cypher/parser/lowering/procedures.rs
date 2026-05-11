use crate::grammar::generated::cypher::cypherparser::{
    OC_ExplicitProcedureInvocationContext, OC_ExplicitProcedureInvocationContextAttrs,
    OC_ImplicitProcedureInvocationContext, OC_ImplicitProcedureInvocationContextAttrs,
    OC_InQueryCallContext, OC_InQueryCallContextAttrs, OC_NamespaceContextAttrs,
    OC_ProcedureNameContext, OC_ProcedureNameContextAttrs, OC_ProcedureResultFieldContext,
    OC_ProcedureResultFieldContextAttrs, OC_StandaloneCallContext, OC_StandaloneCallContextAttrs,
    OC_WhereContextAttrs, OC_YieldItemContext, OC_YieldItemContextAttrs, OC_YieldItemsContext,
    OC_YieldItemsContextAttrs,
};
use crate::language::cypher::ast::{Clause, Expr, ProcedureCallClause, ProcedureYieldItem};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

use super::{context, expressions, names};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcedureCallAst {
    pub name: String,
    pub args: Vec<Expr>,
    pub yields: Vec<ProcedureYieldItem>,
    pub yield_all: bool,
    pub predicate: Option<Expr>,
}

pub(crate) fn lower_in_query_call(ctx: &OC_InQueryCallContext<'_>) -> Result<Clause> {
    let Some(invocation) = ctx.oC_ExplicitProcedureInvocation() else {
        return context::missing("CALL missing procedure invocation");
    };
    let mut call = lower_explicit_procedure_invocation(invocation.as_ref())?;
    if let Some(yield_items) = ctx.oC_YieldItems() {
        let (items, predicate) = lower_yield_items(yield_items.as_ref())?;
        call.yields = items;
        call.predicate = predicate;
    } else if ctx.YIELD().is_some() {
        call.yield_all = true;
    }
    Ok(Clause::Call(ProcedureCallClause {
        name: call.name,
        args: call.args,
        yields: call.yields,
        yield_all: call.yield_all,
        predicate: call.predicate,
        standalone: false,
    }))
}

pub(crate) fn lower_standalone_call(
    ctx: &OC_StandaloneCallContext<'_>,
) -> Result<ProcedureCallAst> {
    let mut call = if let Some(invocation) = ctx.oC_ExplicitProcedureInvocation() {
        lower_explicit_procedure_invocation(invocation.as_ref())?
    } else if let Some(invocation) = ctx.oC_ImplicitProcedureInvocation() {
        lower_implicit_procedure_invocation(invocation.as_ref())?
    } else {
        return context::missing("standalone CALL missing procedure invocation");
    };
    if let Some(yield_items) = ctx.oC_YieldItems() {
        let (items, predicate) = lower_yield_items(yield_items.as_ref())?;
        call.yields = items;
        call.predicate = predicate;
    } else if ctx.YIELD().is_some() {
        call.yield_all = true;
    }
    Ok(call)
}

pub(crate) fn lower_explicit_procedure_invocation(
    ctx: &OC_ExplicitProcedureInvocationContext<'_>,
) -> Result<ProcedureCallAst> {
    let Some(name) = ctx.oC_ProcedureName() else {
        return context::missing("explicit procedure invocation missing name");
    };
    let args = ctx
        .oC_Expression_all()
        .into_iter()
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    Ok(ProcedureCallAst {
        name: lower_procedure_name(name.as_ref())?,
        args,
        yields: Vec::new(),
        yield_all: false,
        predicate: None,
    })
}

pub(crate) fn lower_implicit_procedure_invocation(
    ctx: &OC_ImplicitProcedureInvocationContext<'_>,
) -> Result<ProcedureCallAst> {
    let Some(name) = ctx.oC_ProcedureName() else {
        return context::missing("implicit procedure invocation missing name");
    };
    Ok(ProcedureCallAst {
        name: lower_procedure_name(name.as_ref())?,
        args: Vec::new(),
        yields: Vec::new(),
        yield_all: false,
        predicate: None,
    })
}

pub(crate) fn lower_yield_items(
    ctx: &OC_YieldItemsContext<'_>,
) -> Result<(Vec<ProcedureYieldItem>, Option<Expr>)> {
    let items = ctx
        .oC_YieldItem_all()
        .into_iter()
        .map(|item| lower_yield_item(item.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    let predicate = ctx
        .oC_Where()
        .and_then(|where_ctx| where_ctx.oC_Expression())
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?;
    Ok((items, predicate))
}

pub(crate) fn lower_yield_item(ctx: &OC_YieldItemContext<'_>) -> Result<ProcedureYieldItem> {
    let Some(variable) = ctx.oC_Variable() else {
        return context::missing("YIELD item missing output variable");
    };
    let alias = names::clean_identifier(&variable.get_text());
    let field = ctx
        .oC_ProcedureResultField()
        .map(|field| lower_procedure_result_field(field.as_ref()))
        .transpose()?
        .unwrap_or_else(|| alias.clone());
    Ok(ProcedureYieldItem { field, alias })
}

pub(crate) fn lower_procedure_result_field(
    ctx: &OC_ProcedureResultFieldContext<'_>,
) -> Result<String> {
    let Some(name) = ctx.oC_SymbolicName() else {
        return context::missing("procedure result field missing name");
    };
    Ok(names::clean_identifier(&name.get_text()))
}

pub(crate) fn lower_procedure_name(ctx: &OC_ProcedureNameContext<'_>) -> Result<String> {
    let mut parts = Vec::new();
    if let Some(namespace) = ctx.oC_Namespace() {
        parts.extend(
            namespace
                .oC_SymbolicName_all()
                .into_iter()
                .map(|name| names::clean_identifier(&name.get_text())),
        );
    }
    if let Some(name) = ctx.oC_SymbolicName() {
        parts.push(names::clean_identifier(&name.get_text()));
    }
    if parts.is_empty() {
        return context::missing("procedure name missing symbolic name");
    }
    Ok(parts.join("."))
}
