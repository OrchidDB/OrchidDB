use crate::ir::plan::{ApplyKind, Node};
use crate::ir::policy::OptionalMissing;
use crate::language::cypher::ast::{BinaryOp, ExistsSubquery, Expr, PatternPart, UnaryOp};
use crate::language::cypher::planner::error::CypherPlanResult;
use crate::language::cypher::planner::lowering::{
    Lowerer, context::CypherTraversalKind, pattern, project,
};

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
        project::lower_exists_right_plan(lowerer, exists)
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
        lowerer.with_child_traversal(CypherTraversalKind::PatternPredicate, |lowerer| {
            let mut right = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let history = patterns
                .iter()
                .any(|part| !part.element.chains.is_empty())
                .then(|| lowerer.synthetic("pattern_history"));
            for part in patterns {
                right = pattern::lower_pattern_part(
                    lowerer,
                    right,
                    part,
                    false,
                    history.as_deref(),
                    false,
                )?;
            }
            Ok(right)
        })
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
    // Pattern predicates are existential subqueries; fresh variables are
    // locally bound inside the predicate (Kuzu-compatible), so no scope
    // violation is raised here.
    let _ = (lowerer, patterns);
    Ok(())
}
