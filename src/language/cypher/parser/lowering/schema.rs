use crate::grammar::generated::cypher::cypherparser::{
    OC_NamespaceContext, OC_NamespaceContextAttrs, OC_ReservedWordContext, OC_SchemaNameContext,
};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

use super::names;

pub(crate) fn lower_schema_name(ctx: &OC_SchemaNameContext<'_>) -> Result<String> {
    Ok(names::clean_identifier(&ctx.get_text()))
}

pub(crate) fn lower_namespace(ctx: &OC_NamespaceContext<'_>) -> Result<Vec<String>> {
    Ok(ctx
        .oC_SymbolicName_all()
        .into_iter()
        .map(|name| names::clean_identifier(&name.get_text()))
        .collect())
}

pub(crate) fn lower_reserved_word(ctx: &OC_ReservedWordContext<'_>) -> Result<String> {
    Ok(names::clean_identifier(&ctx.get_text()))
}
