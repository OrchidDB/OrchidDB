use crate::grammar::generated::cypher::cypherparser::{
    OC_ReadingClauseContext, OC_ReturnContext, OC_ReturnContextAttrs, OC_WhereContextAttrs,
    OC_WithContext, OC_WithContextAttrs,
};
use crate::language::cypher::ast::{Clause, ReturnClause, WithClause};
use crate::language::cypher::parser::Result;

use super::{clauses, expressions, projections};

pub(crate) fn lower_reading_clause(ctx: &OC_ReadingClauseContext<'_>) -> Result<Clause> {
    clauses::lower_reading_clause(ctx)
}

pub(crate) fn lower_with_clause(ctx: &OC_WithContext<'_>) -> Result<WithClause> {
    let projection = projections::lower_projection_body_required(ctx.oC_ProjectionBody())?;
    let predicate = ctx
        .oC_Where()
        .and_then(|where_ctx| where_ctx.oC_Expression())
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?;
    Ok(WithClause {
        projection,
        predicate,
    })
}

pub(crate) fn lower_return_clause(ctx: &OC_ReturnContext<'_>) -> Result<ReturnClause> {
    let projection = projections::lower_projection_body_required(ctx.oC_ProjectionBody())?;
    Ok(ReturnClause { projection })
}
