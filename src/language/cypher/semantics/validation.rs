//! Procedure, projection, union, and pattern binding validation.

use super::expression_types::{display_order_expr, projected_expr_kind};
use super::{
    BTreeMap, BTreeSet, BindingKind, CypherPlanError, CypherPlanResult, Expr, PatternElement,
    PatternPart, ProjectionBody, SemanticOutput, SemanticScope,
};
use crate::language::cypher::planner::CypherSemanticError;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcedureMode {
    Read,
    Write,
}

pub(super) fn procedure_mode(name: &str) -> ProcedureMode {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" | "db.relationshiptypes" | "db.propertykeys" => ProcedureMode::Read,
        _ => ProcedureMode::Write,
    }
}

pub(super) fn procedure_yields(
    clause: &crate::language::cypher::ast::ProcedureCallClause,
) -> (Vec<String>, Vec<String>) {
    if !clause.yields.is_empty() {
        return (
            clause
                .yields
                .iter()
                .map(|item| item.field.clone())
                .collect(),
            clause
                .yields
                .iter()
                .map(|item| item.alias.clone())
                .collect(),
        );
    }
    if clause.yield_all || clause.standalone {
        let yields = default_procedure_yields(&clause.name);
        return (yields.clone(), yields);
    }
    (Vec::new(), Vec::new())
}

pub(super) fn default_procedure_yields(name: &str) -> Vec<String> {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" => vec!["label".to_string()],
        "db.relationshiptypes" => vec!["relationshipType".to_string()],
        "db.propertykeys" => vec!["propertyKey".to_string()],
        _ => vec!["value".to_string()],
    }
}

pub(super) fn validate_unique(fields: &[String], message: &str) -> CypherPlanResult<()> {
    let mut seen = BTreeSet::new();
    let duplicates = fields
        .iter()
        .filter(|field| !seen.insert((*field).clone()))
        .cloned()
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "{message}: {}",
            duplicates.join(", ")
        )).classified(CypherSemanticError::ColumnNameConflict))
    }
}

pub(super) fn validate_union_outputs(
    left: &[SemanticOutput],
    right: &[SemanticOutput],
) -> CypherPlanResult<()> {
    if left.len() != right.len() || left.iter().zip(right).any(|(a,b)|a.name!=b.name) {
        return Err(CypherPlanError::Invalid(
            "UNION branches must return the same column names in the same order.".to_string(),
        ).classified(CypherSemanticError::DifferentColumnsInUnion));
    }
    for (expected, actual) in left.iter().zip(right.iter()) {
        if union_output_kinds_compatible(expected.kind, actual.kind) {
            continue;
        }
        return Err(CypherPlanError::Invalid(format!(
            "Binder exception: {} has data type {} but {} was expected.",
            actual.name,
            actual.kind.cypher_type_name(),
            expected.kind.cypher_type_name()
        )));
    }
    Ok(())
}

pub(super) fn validate_order_by_supported(
    expr: &Expr,
    scope: &SemanticScope,
) -> CypherPlanResult<()> {
    let kind = projected_expr_kind(expr, scope);
    let unsupported = matches!(
        kind,
        BindingKind::Node
            | BindingKind::Relationship
            | BindingKind::RecursiveRelationship
            | BindingKind::Path
            | BindingKind::InternalId
            | BindingKind::ListInt
            | BindingKind::FixedListInt
            | BindingKind::StructDescription
            | BindingKind::MapStringInt
            | BindingKind::UnionMovieGrade
    );
    if unsupported {
        return Err(CypherPlanError::Invalid(format!(
            "Binder exception: Cannot order by {}. Order by {} is not supported.",
            display_order_expr(expr),
            order_by_type_name(kind)
        )));
    }
    Ok(())
}

pub(super) fn order_by_type_name(kind: BindingKind) -> &'static str {
    match kind {
        BindingKind::Relationship | BindingKind::RecursiveRelationship => "REL",
        _ => kind.cypher_type_name(),
    }
}

pub(super) fn union_output_kinds_compatible(left: BindingKind, right: BindingKind) -> bool {
    left == right
        || matches!(left, BindingKind::Unknown | BindingKind::Value)
        || matches!(right, BindingKind::Unknown | BindingKind::Value)
        || matches!(
            (left, right),
            (BindingKind::Int, BindingKind::Float) | (BindingKind::Float, BindingKind::Int)
        )
}

pub(super) fn validate_with_projection_aliases(body: &ProjectionBody) -> CypherPlanResult<()> {
    let missing = body
        .items
        .iter()
        .filter(|item| !item.explicit_alias && !matches!(item.expr, Expr::Variable(_)))
        .map(|item| {
            item.alias
                .clone()
                .unwrap_or_else(|| "<expression>".to_string())
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(
            "Binder exception: Expression in WITH must be aliased (use AS).".to_string(),
        ).classified(CypherSemanticError::NoExpressionAlias))
    }
}

pub(super) fn validate_path_binding(
    part: &PatternPart,
    scope: &SemanticScope,
) -> CypherPlanResult<()> {
    let Some(path) = &part.variable else {
        return Ok(());
    };
    if scope.contains(path) || pattern_element_declares(&part.element, path) {
        return Err(
            CypherPlanError::Invalid("SyntaxError: VariableAlreadyBound".to_string())
                .classified(CypherSemanticError::VariableAlreadyBound),
        );
    }
    Ok(())
}

pub(super) fn validate_relationship_binding(
    binding: &str,
    expected: BindingKind,
    kinds: &BTreeMap<String, BindingKind>,
) -> CypherPlanResult<()> {
    match kinds.get(binding).copied() {
        Some(kind) if kind != expected && kind != BindingKind::Unknown
            && !(kind == BindingKind::ListRelationship && expected == BindingKind::RecursiveRelationship) => {
            Err(CypherPlanError::Invalid(format!(
                "Binder exception: {binding} has data type {} but {} was expected.",
                kind.cypher_type_name(),
                expected.cypher_type_name()
            ))
            .classified(CypherSemanticError::VariableTypeConflict))
        }
        _ => Ok(()),
    }
}

pub(super) fn validate_node_binding(
    binding: &str,
    kinds: &BTreeMap<String, BindingKind>,
) -> CypherPlanResult<()> {
    match kinds.get(binding).copied() {
        Some(kind) if !matches!(kind, BindingKind::Unknown | BindingKind::Node) => {
            Err(CypherPlanError::Invalid(format!(
                "Binder exception: Cannot bind {binding} as node pattern."
            ))
            .classified(CypherSemanticError::VariableTypeConflict))
        }
        _ => Ok(()),
    }
}

pub(super) fn validate_pattern_predicate_scope(
    _patterns: &[PatternPart],
    _candidates: &BTreeSet<String>,
) -> CypherPlanResult<()> {
    // Pattern predicates are existential subqueries; fresh variables are
    // locally bound inside the predicate (Kuzu-compatible), so no scope
    // violation is raised here.
    Ok(())
}

pub(super) fn pattern_element_declares(element: &PatternElement, binding: &str) -> bool {
    element.start.variable.as_deref() == Some(binding)
        || element.chains.iter().any(|chain| {
            chain.node.variable.as_deref() == Some(binding)
                || chain.relationship.variable.as_deref() == Some(binding)
        })
}

pub(super) fn pattern_binding_names(pattern: &PatternPart) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(variable) = &pattern.variable {
        names.insert(variable.clone());
    }
    if let Some(variable) = &pattern.element.start.variable {
        names.insert(variable.clone());
    }
    for chain in &pattern.element.chains {
        if let Some(variable) = &chain.relationship.variable {
            names.insert(variable.clone());
        }
        if let Some(variable) = &chain.node.variable {
            names.insert(variable.clone());
        }
    }
    names
}

pub(super) fn is_variable_length(range: &crate::language::cypher::ast::RangeLiteral) -> bool {
    range.explicit || range.min != 1 || range.max != Some(1)
}
