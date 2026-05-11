use crate::grammar::generated::cypher::cypherparser::{
    OC_ExistentialSubqueryContext, OC_ExistentialSubqueryContextAttrs, OC_WhereContextAttrs,
};
use crate::language::cypher::ast::{ExistsSubquery, Expr};
use crate::language::cypher::parser::Result;

use super::{expressions, patterns, queries};

pub(crate) fn lower_existential_subquery(ctx: &OC_ExistentialSubqueryContext<'_>) -> Result<Expr> {
    let query = ctx
        .oC_RegularQuery()
        .map(|query| queries::lower_regular_query(query.as_ref()).map(Box::new))
        .transpose()?;
    let patterns = ctx
        .oC_Pattern()
        .map(|pattern| patterns::lower_pattern(pattern.as_ref()))
        .transpose()?
        .unwrap_or_default();
    let predicate = ctx
        .oC_Where()
        .and_then(|where_ctx| where_ctx.oC_Expression())
        .map(|expr| expressions::lower_expression(expr.as_ref()))
        .transpose()?
        .map(Box::new);
    Ok(Expr::Exists(ExistsSubquery {
        query,
        patterns,
        predicate,
    }))
}
