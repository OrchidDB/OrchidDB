use crate::grammar::generated::cypher::cypherparser::{
    OC_AnonymousPatternPartContextAttrs, OC_NodeLabelsContextAttrs, OC_NodePatternContext,
    OC_NodePatternContextAttrs, OC_PatternContext, OC_PatternContextAttrs,
    OC_PatternElementChainContext, OC_PatternElementChainContextAttrs, OC_PatternElementContext,
    OC_PatternElementContextAttrs, OC_PatternPartContext, OC_PatternPartContextAttrs,
    OC_RelationshipDetailContextAttrs, OC_RelationshipPatternContext,
    OC_RelationshipPatternContextAttrs, OC_RelationshipTypesContextAttrs,
    OC_RelationshipsPatternContext, OC_RelationshipsPatternContextAttrs,
};
use crate::language::cypher::ast::{
    NodePattern, PatternElement, PatternElementChain, PatternPart, RelationshipPattern,
};
use crate::language::cypher::parser::Result;
use antlr4rust::tree::ParseTree;

use super::{context, names, properties, ranges};

pub(crate) fn lower_pattern(ctx: &OC_PatternContext<'_>) -> Result<Vec<PatternPart>> {
    ctx.oC_PatternPart_all()
        .into_iter()
        .map(|part| lower_pattern_part(part.as_ref()))
        .collect()
}

pub(crate) fn lower_relationships_pattern(
    ctx: &OC_RelationshipsPatternContext<'_>,
) -> Result<PatternPart> {
    let Some(start) = ctx.oC_NodePattern() else {
        return context::missing("relationships pattern missing start node");
    };
    let chains = ctx
        .oC_PatternElementChain_all()
        .into_iter()
        .map(|chain| lower_pattern_chain(chain.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    Ok(PatternPart {
        variable: None,
        element: PatternElement {
            start: lower_node_pattern(start.as_ref())?,
            chains,
        },
    })
}

pub(crate) fn lower_pattern_part(ctx: &OC_PatternPartContext<'_>) -> Result<PatternPart> {
    let variable = ctx
        .oC_Variable()
        .map(|var| names::clean_identifier(&var.get_text()));
    let Some(anonymous) = ctx.oC_AnonymousPatternPart() else {
        return context::missing("pattern part missing anonymous pattern");
    };
    let Some(element) = anonymous.oC_PatternElement() else {
        return context::missing("anonymous pattern part missing pattern element");
    };
    Ok(PatternPart {
        variable,
        element: lower_pattern_element(element.as_ref())?,
    })
}

pub(crate) fn lower_pattern_element(ctx: &OC_PatternElementContext<'_>) -> Result<PatternElement> {
    if let Some(nested) = ctx.oC_PatternElement() {
        return lower_pattern_element(nested.as_ref());
    }
    let Some(start) = ctx.oC_NodePattern() else {
        return context::missing("pattern element missing start node");
    };
    let chains = ctx
        .oC_PatternElementChain_all()
        .into_iter()
        .map(|chain| lower_pattern_chain(chain.as_ref()))
        .collect::<Result<_>>()?;
    Ok(PatternElement {
        start: lower_node_pattern(start.as_ref())?,
        chains,
    })
}

pub(crate) fn lower_pattern_chain(
    ctx: &OC_PatternElementChainContext<'_>,
) -> Result<PatternElementChain> {
    let Some(relationship) = ctx.oC_RelationshipPattern() else {
        return context::missing("pattern chain missing relationship");
    };
    let Some(node) = ctx.oC_NodePattern() else {
        return context::missing("pattern chain missing target node");
    };
    Ok(PatternElementChain {
        relationship: lower_relationship_pattern(relationship.as_ref())?,
        node: lower_node_pattern(node.as_ref())?,
    })
}

pub(crate) fn lower_node_pattern(ctx: &OC_NodePatternContext<'_>) -> Result<NodePattern> {
    let variable = ctx
        .oC_Variable()
        .map(|var| names::clean_identifier(&var.get_text()));
    let labels = ctx
        .oC_NodeLabels()
        .map(|labels| {
            labels
                .oC_NodeLabel_all()
                .into_iter()
                .map(|label| names::lower_node_label(label.as_ref()))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let properties = ctx
        .oC_Properties()
        .map(|props| properties::lower_properties(props.as_ref()))
        .transpose()?;
    Ok(NodePattern {
        variable,
        labels,
        properties,
    })
}

pub(crate) fn lower_relationship_pattern(
    ctx: &OC_RelationshipPatternContext<'_>,
) -> Result<RelationshipPattern> {
    let direction = ranges::relationship_direction(
        ctx.oC_LeftArrowHead().is_some(),
        ctx.oC_RightArrowHead().is_some(),
    );
    let detail = ctx.oC_RelationshipDetail();
    let variable = detail
        .as_ref()
        .and_then(|detail| detail.oC_Variable())
        .map(|var| names::clean_identifier(&var.get_text()));
    let types = detail
        .as_ref()
        .and_then(|detail| detail.oC_RelationshipTypes())
        .map(|types| {
            types
                .oC_RelTypeName_all()
                .into_iter()
                .map(|ty| names::lower_rel_type_name(ty.as_ref()))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let range = detail
        .as_ref()
        .and_then(|detail| detail.oC_RangeLiteral())
        .map(|range| ranges::lower_range_literal(range.as_ref()))
        .transpose()?
        .unwrap_or_default();
    let properties = detail
        .as_ref()
        .and_then(|detail| detail.oC_Properties())
        .map(|props| properties::lower_properties(props.as_ref()))
        .transpose()?;
    Ok(RelationshipPattern {
        variable,
        types,
        range,
        direction,
        properties,
    })
}
