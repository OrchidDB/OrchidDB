use crate::ir::plan::{ApplyKind, Node};
use crate::ir::policy::OptionalMissing;
use crate::language::cypher::ast::{BinaryOp, ExistsSubquery, Expr, PatternPart, UnaryOp};
use crate::language::cypher::planner::error::{CypherPlanError, CypherPlanResult};
use crate::language::cypher::planner::lowering::{
    Lowerer, context::CypherTraversalKind, pattern, project,
};
use std::collections::BTreeSet;

pub(crate) fn lower_where_predicate(
    lowerer: &mut Lowerer,
    input: Node,
    predicate: &Expr,
) -> CypherPlanResult<Node> {
    project::validate_expression_scope(lowerer, predicate, "WHERE predicate")?;
    match predicate {
        Expr::Binary {
            op: BinaryOp::And,
            lhs,
            rhs,
        } => {
            let input = lower_where_predicate(lowerer, input, lhs)?;
            lower_where_predicate(lowerer, input, rhs)
        }
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
        } => lower_negated_predicate(lowerer, input, expr),
        Expr::Exists(exists) => lower_exists_apply(lowerer, input, exists, ApplyKind::Semi),
        Expr::PatternPredicate(patterns) => {
            validate_pattern_predicate_scope(lowerer, patterns)?;
            lower_pattern_predicate_apply(lowerer, input, patterns, ApplyKind::Semi)
        }
        scalar => {
            let (input, condition) = project::lower_expr_with_input(lowerer, input, scalar)?;
            Ok(Node::GraphFilter {
                condition,
                input: input.boxed(),
            })
        }
    }
}

fn lower_negated_predicate(
    lowerer: &mut Lowerer,
    input: Node,
    predicate: &Expr,
) -> CypherPlanResult<Node> {
    match predicate {
        Expr::Exists(exists) => lower_exists_apply(lowerer, input, exists, ApplyKind::Anti),
        Expr::PatternPredicate(patterns) => {
            validate_pattern_predicate_scope(lowerer, patterns)?;
            lower_pattern_predicate_apply(lowerer, input, patterns, ApplyKind::Anti)
        }
        _ => {
            let (input, condition) = project::lower_expr_with_input(
                lowerer,
                input,
                &Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(predicate.clone()),
                },
            )?;
            Ok(Node::GraphFilter {
                condition,
                input: input.boxed(),
            })
        }
    }
}

pub(crate) fn lower_exists_apply(
    lowerer: &mut Lowerer,
    input: Node,
    exists: &ExistsSubquery,
    kind: ApplyKind,
) -> CypherPlanResult<Node> {
    let right = lowerer.with_preserved_scope(|lowerer| {
        let parent = lowerer.current_traversal().cloned();
        if let Some(parent) = parent.as_ref() {
            let traversal = lowerer.child_traversal(parent, CypherTraversalKind::ExistsSubquery);
            lowerer.push_traversal(traversal);
        }
        let right = if let Some(query) = &exists.query {
            lowerer.lower_query_with_unions(query).map(|(node, _)| node)
        } else {
            let mut right = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let history = exists
                .patterns
                .iter()
                .any(|part| !part.element.chains.is_empty())
                .then(|| lowerer.synthetic("exists_history"));
            let mut history_available = false;
            for part in &exists.patterns {
                right = pattern::lower_pattern_part(
                    lowerer,
                    right,
                    part,
                    false,
                    history.as_deref(),
                    history_available,
                )?;
                if !part.element.chains.is_empty() {
                    history_available = true;
                }
            }
            if let Some(predicate) = &exists.predicate {
                let parent = lowerer.current_traversal().cloned();
                if let Some(parent) = parent.as_ref() {
                    let traversal =
                        lowerer.child_traversal(parent, CypherTraversalKind::WherePredicate);
                    lowerer.push_traversal(traversal);
                }
                right = lower_where_predicate(lowerer, right, predicate)?;
                lowerer.record_current_imports(lowerer.visible_fields());
                lowerer.record_current_correlation(lowerer.visible_fields());
                if parent.is_some() {
                    lowerer.pop_traversal();
                }
            }
            Ok(right)
        };
        if parent.is_some() {
            lowerer.pop_traversal();
        }
        right
    })?;
    Ok(Node::GraphApply {
        kind,
        correlation: lowerer.visible_fields(),
        outputs: Vec::new(),
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: right.boxed(),
    })
}

pub(crate) fn lower_pattern_predicate_apply(
    lowerer: &mut Lowerer,
    input: Node,
    patterns: &[PatternPart],
    kind: ApplyKind,
) -> CypherPlanResult<Node> {
    validate_pattern_predicate_scope(lowerer, patterns)?;
    let right = lowerer.with_preserved_scope(|lowerer| {
        let parent = lowerer.current_traversal().cloned();
        if let Some(parent) = parent.as_ref() {
            let traversal = lowerer.child_traversal(parent, CypherTraversalKind::PatternPredicate);
            lowerer.push_traversal(traversal);
        }
        let mut right = Node::GraphCorrelate {
            bindings: lowerer.visible_fields(),
        };
        let history = patterns
            .iter()
            .any(|part| !part.element.chains.is_empty())
            .then(|| lowerer.synthetic("pattern_history"));
        let mut history_available = false;
        for part in patterns {
            right = pattern::lower_pattern_part(
                lowerer,
                right,
                part,
                false,
                history.as_deref(),
                history_available,
            )?;
            if !part.element.chains.is_empty() {
                history_available = true;
            }
        }
        if parent.is_some() {
            lowerer.pop_traversal();
        }
        Ok(right)
    })?;
    Ok(Node::GraphApply {
        kind,
        correlation: lowerer.visible_fields(),
        outputs: Vec::new(),
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: right.boxed(),
    })
}

pub(crate) fn validate_pattern_predicate_scope(
    lowerer: &Lowerer,
    patterns: &[PatternPart],
) -> CypherPlanResult<()> {
    let visible = lowerer.visible_set();
    let mut named = BTreeSet::new();
    for part in patterns {
        collect_pattern_names(part, &mut named);
    }
    let introduced = named
        .into_iter()
        .filter(|name| !visible.contains(name))
        .collect::<Vec<_>>();
    if introduced.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "pattern predicates may not introduce new variables: {}",
            introduced.join(", ")
        )))
    }
}

fn collect_pattern_names(part: &PatternPart, out: &mut BTreeSet<String>) {
    if let Some(variable) = &part.variable {
        out.insert(variable.clone());
    }
    if let Some(variable) = &part.element.start.variable {
        out.insert(variable.clone());
    }
    for chain in &part.element.chains {
        if let Some(variable) = &chain.relationship.variable {
            out.insert(variable.clone());
        }
        if let Some(variable) = &chain.node.variable {
            out.insert(variable.clone());
        }
    }
}
