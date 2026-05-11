use crate::grammar::generated::cypher::cypherparser::OC_ParameterContext;
use crate::language::cypher::ast::Expr;
use crate::language::cypher::parser::Result;

use super::expressions;

pub(crate) fn lower_parameter(ctx: &OC_ParameterContext<'_>) -> Result<Expr> {
    expressions::lower_parameter(ctx)
}
