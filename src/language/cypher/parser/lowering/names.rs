use crate::grammar::generated::cypher::cypherparser::{
    OC_LabelNameContext, OC_NodeLabelContext, OC_RelTypeNameContext, OC_SymbolicNameContext,
    OC_VariableContext,
};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

pub(crate) fn lower_variable(ctx: &OC_VariableContext<'_>) -> Result<String> {
    Ok(clean_identifier(&ctx.get_text()))
}

pub(crate) fn lower_symbolic_name(ctx: &OC_SymbolicNameContext<'_>) -> Result<String> {
    Ok(clean_identifier(&ctx.get_text()))
}

pub(crate) fn lower_label_name(ctx: &OC_LabelNameContext<'_>) -> Result<String> {
    Ok(clean_identifier(&ctx.get_text()))
}

pub(crate) fn lower_node_label(ctx: &OC_NodeLabelContext<'_>) -> Result<String> {
    Ok(clean_label(&ctx.get_text()))
}

pub(crate) fn lower_rel_type_name(ctx: &OC_RelTypeNameContext<'_>) -> Result<String> {
    Ok(clean_label(&ctx.get_text()))
}

pub(crate) fn clean_identifier(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(escaped) = trimmed.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
        escaped.replace("``", "`")
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn clean_label(text: &str) -> String {
    clean_identifier(text.trim().trim_start_matches(':').trim_start_matches('|'))
}
