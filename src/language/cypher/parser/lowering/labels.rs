use crate::grammar::generated::cypher::cypherparser::{
    OC_NodeLabelContext, OC_NodeLabelsContext, OC_NodeLabelsContextAttrs,
    OC_RelationshipTypesContext, OC_RelationshipTypesContextAttrs,
};
use crate::language::cypher::parser::Result;

use super::names;

pub(crate) fn lower_node_labels(ctx: &OC_NodeLabelsContext<'_>) -> Result<Vec<String>> {
    ctx.oC_NodeLabel_all()
        .into_iter()
        .map(|label| lower_node_label(label.as_ref()))
        .collect()
}

pub(crate) fn lower_node_label(ctx: &OC_NodeLabelContext<'_>) -> Result<String> {
    names::lower_node_label(ctx)
}

pub(crate) fn lower_relationship_types(
    ctx: &OC_RelationshipTypesContext<'_>,
) -> Result<Vec<String>> {
    ctx.oC_RelTypeName_all()
        .into_iter()
        .map(|ty| names::lower_rel_type_name(ty.as_ref()))
        .collect()
}
