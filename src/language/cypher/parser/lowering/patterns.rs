use crate::grammar::generated::cypher::cypherparser::{
    OC_AnonymousPatternPartContextAttrs, OC_NodeLabelsContextAttrs, OC_NodePatternContext,
    OC_NodePatternContextAttrs, OC_PatternContext, OC_PatternContextAttrs,
    OC_PatternElementChainContext, OC_PatternElementChainContextAttrs, OC_PatternElementContext,
    OC_PatternElementContextAttrs, OC_PatternPartContext, OC_PatternPartContextAttrs,
    OC_RecursiveRelationshipFilterContext, OC_RelationshipDetailContextAttrs,
    OC_RelationshipPatternContext, OC_RelationshipPatternContextAttrs,
    OC_RelationshipTypesContextAttrs, OC_RelationshipsPatternContext,
    OC_RelationshipsPatternContextAttrs,
};
use crate::language::cypher::ast::{
    Clause, Expr, NodePattern, PatternElement, PatternElementChain, PatternPart,
    RecursiveRelationshipPattern, RelationshipPattern,
};
use crate::language::cypher::parser::{CypherParseError, Result, parse_query};
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
            let mut lowered = Vec::new();
            for label in labels.oC_NodeLabel_all() {
                lowered.extend(names::lower_node_label_names(label.as_ref())?);
            }
            Ok(lowered)
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
    let recursive = detail
        .as_ref()
        .and_then(|detail| detail.oC_RecursiveRelationshipFilter())
        .map(|filter| lower_recursive_relationship_filter(filter.as_ref()))
        .transpose()?;
    Ok(RelationshipPattern {
        variable,
        types,
        range,
        direction,
        properties,
        recursive,
    })
}

fn lower_recursive_relationship_filter(
    ctx: &OC_RecursiveRelationshipFilterContext<'_>,
) -> Result<RecursiveRelationshipPattern> {
    let text = ctx.get_text();
    let body = text
        .strip_prefix('(')
        .and_then(|body| body.strip_suffix(')'))
        .ok_or_else(|| CypherParseError::Parse("recursive relationship filter".to_string()))?;
    let parts = split_top_level(body, '|');
    let variables = split_top_level(parts.first().copied().unwrap_or_default(), ',');
    let rel_variable = variables.first().copied().unwrap_or("r").trim().to_string();
    let node_variable = variables.get(1).copied().unwrap_or("n").trim().to_string();
    let mut predicate = None;
    let mut rel_projection_keys = None;
    let mut node_projection_keys = None;

    for part in parts.iter().skip(1).copied() {
        let trimmed = part.trim();
        if starts_with_keyword(trimmed, "WHERE") {
            let expr = trimmed["WHERE".len()..].trim();
            if !expr.is_empty() {
                predicate = Some(parse_expression_text(expr)?);
            }
            continue;
        }
        let projections = split_top_level(trimmed, ',');
        if let Some(rel_projection) = projections.first() {
            rel_projection_keys = Some(projection_keys(rel_projection, &rel_variable));
        }
        if let Some(node_projection) = projections.get(1) {
            node_projection_keys = Some(projection_keys(node_projection, &node_variable));
        }
    }

    Ok(RecursiveRelationshipPattern {
        rel_variable,
        node_variable,
        predicate,
        rel_projection_keys,
        node_projection_keys,
    })
}

fn parse_expression_text(text: &str) -> Result<Expr> {
    let query = parse_query(&format!("RETURN {text}"))?;
    let Some(Clause::Return(ret)) = query.clauses.first() else {
        return Err(CypherParseError::Parse(
            "recursive relationship expression".to_string(),
        ));
    };
    ret.projection
        .items
        .first()
        .map(|item| item.expr.clone())
        .ok_or_else(|| CypherParseError::Parse("recursive relationship expression".to_string()))
}

fn projection_keys(text: &str, variable: &str) -> Vec<String> {
    let Some(body) = text
        .trim()
        .strip_prefix('{')
        .and_then(|body| body.strip_suffix('}'))
    else {
        return Vec::new();
    };
    split_top_level(body, ',')
        .into_iter()
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            let value = item
                .split_once(':')
                .map(|(_, value)| value.trim())
                .unwrap_or(item);
            value
                .strip_prefix(variable)
                .and_then(|suffix| suffix.strip_prefix('.'))
                .map(|key| key.trim().to_string())
        })
        .collect()
}

fn starts_with_keyword(text: &str, keyword: &str) -> bool {
    text.len() >= keyword.len() && text[..keyword.len()].eq_ignore_ascii_case(keyword)
}

fn split_top_level(input: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if ch == delimiter && depth == 0 => {
                parts.push(input[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(input[start..].trim());
    parts
}
