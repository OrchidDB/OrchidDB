use crate::grammar::generated::cypher::cypherparser::{
    OC_CreateContext, OC_DeleteContext, OC_MergeActionContext, OC_MergeContext, OC_RemoveContext,
    OC_RemoveItemContext, OC_SetContext, OC_SetItemContext, OC_UpdatingClauseContext,
};
use crate::language::cypher::ast::Clause;
use crate::language::cypher::parser::Result;

use super::context;

pub(crate) fn lower_updating_clause(_ctx: &OC_UpdatingClauseContext<'_>) -> Result<Clause> {
    context::unsupported("updating clauses are outside the read Cypher AST lowering path for now")
}

pub(crate) fn lower_create(_ctx: &OC_CreateContext<'_>) -> Result<Clause> {
    context::unsupported("CREATE is outside read and side-effect query lowering")
}

pub(crate) fn lower_merge(_ctx: &OC_MergeContext<'_>) -> Result<Clause> {
    context::unsupported("MERGE is outside read and side-effect query lowering")
}

pub(crate) fn lower_merge_action(_ctx: &OC_MergeActionContext<'_>) -> Result<()> {
    context::unsupported("MERGE actions are outside read and side-effect query lowering")
}

pub(crate) fn lower_set(_ctx: &OC_SetContext<'_>) -> Result<Clause> {
    context::unsupported("SET is outside read and side-effect query lowering")
}

pub(crate) fn lower_set_item(_ctx: &OC_SetItemContext<'_>) -> Result<()> {
    context::unsupported("SET items are outside read and side-effect query lowering")
}

pub(crate) fn lower_delete(_ctx: &OC_DeleteContext<'_>) -> Result<Clause> {
    context::unsupported("DELETE is outside read and side-effect query lowering")
}

pub(crate) fn lower_remove(_ctx: &OC_RemoveContext<'_>) -> Result<Clause> {
    context::unsupported("REMOVE is outside read and side-effect query lowering")
}

pub(crate) fn lower_remove_item(_ctx: &OC_RemoveItemContext<'_>) -> Result<()> {
    context::unsupported("REMOVE items are outside read and side-effect query lowering")
}
