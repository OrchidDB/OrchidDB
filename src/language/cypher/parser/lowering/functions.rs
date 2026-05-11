use crate::grammar::generated::cypher::cypherparser::{
    OC_FunctionInvocationContext, OC_FunctionNameContext,
};
use crate::language::cypher::ast::Expr;
use crate::language::cypher::parser::Result;

use super::expressions;

pub(crate) fn lower_function_invocation(ctx: &OC_FunctionInvocationContext<'_>) -> Result<Expr> {
    expressions::lower_function_invocation(ctx)
}

pub(crate) fn lower_function_name(ctx: &OC_FunctionNameContext<'_>) -> Result<String> {
    expressions::lower_function_name(ctx)
}
