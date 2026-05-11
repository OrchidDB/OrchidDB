use crate::grammar::generated::cypher::cypherparser::{
    OC_MatchContext, OC_MatchContextAttrs, OC_ReadingClauseContext, OC_ReadingClauseContextAttrs,
    OC_UnwindContext, OC_UnwindContextAttrs, OC_WhereContextAttrs,
};
use crate::language::cypher::ast::{Clause, MatchClause, UnwindClause};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

use super::{context, expressions, names, patterns, procedures};

pub(crate) fn lower_reading_clause(ctx: &OC_ReadingClauseContext<'_>) -> Result<Clause> {
    if let Some(match_ctx) = ctx.oC_Match() {
        return lower_match(match_ctx.as_ref()).map(Clause::Match);
    }
    if let Some(unwind_ctx) = ctx.oC_Unwind() {
        return lower_unwind(unwind_ctx.as_ref()).map(Clause::Unwind);
    }
    if let Some(call_ctx) = ctx.oC_InQueryCall() {
        return procedures::lower_in_query_call(call_ctx.as_ref());
    }
    context::missing("reading clause missing MATCH, UNWIND, or CALL")
}

pub(crate) fn lower_match(ctx: &OC_MatchContext<'_>) -> Result<MatchClause> {
    let Some(pattern) = ctx.oC_Pattern() else {
        return context::missing("MATCH missing pattern");
    };
    let patterns = patterns::lower_pattern(pattern.as_ref())?;
    let predicate = ctx
        .oC_Where()
        .and_then(|where_ctx| where_ctx.oC_Expression())
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?;
    Ok(MatchClause {
        optional: ctx.OPTIONAL().is_some(),
        patterns,
        predicate,
    })
}

pub(crate) fn lower_unwind(ctx: &OC_UnwindContext<'_>) -> Result<UnwindClause> {
    let Some(expr) = ctx.oC_Expression() else {
        return context::missing("UNWIND missing expression");
    };
    let expr = expressions::lower_expression(expr.as_ref())?;
    let Some(alias) = ctx.oC_Variable() else {
        return context::missing("UNWIND missing alias");
    };
    let alias = names::clean_identifier(&alias.get_text());
    Ok(UnwindClause { expr, alias })
}
