use crate::grammar::generated::cypher::cypherparser::{
    OC_CypherContext, OC_CypherContextAttrs, OC_StatementContext, OC_StatementContextAttrs,
};
use crate::language::cypher::ast::Query;
use crate::language::cypher::parser::Result;

use super::{context, queries};

pub(crate) fn lower_cypher(ctx: &OC_CypherContext<'_>) -> Result<Query> {
    let Some(statement) = ctx.oC_Statement() else {
        return context::missing("Cypher input did not contain a statement");
    };
    lower_statement(statement.as_ref())
}

pub(crate) fn lower_statement(ctx: &OC_StatementContext<'_>) -> Result<Query> {
    let Some(query) = ctx.oC_Query() else {
        return context::missing("Cypher statement did not contain a query");
    };
    queries::lower_query(query.as_ref())
}
