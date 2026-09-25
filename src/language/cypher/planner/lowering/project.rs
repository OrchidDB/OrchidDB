mod aggregate;
use aggregate::{contains_aggregate, lower_aggregate};
mod expression;
pub use expression::lower_expr;

mod scope;
use scope::{
    add_candidate_pattern_bindings, free_variable_names_for_sort, projection_output_names,
    remove_local_exists_bindings,
};
pub(crate) use scope::{
    collect_free_variables, expression_candidate_refs, validate_expression_refs,
    validate_expression_scope,
};
mod aliases;
use aliases::{projection_aliases, shadowed_projection_fields, substitute_projection_aliases};
mod materialize;
pub(crate) use materialize::lower_expr_with_input;
use materialize::requires_scoped_materialization;

use crate::ir::expr::{AggCall, AggKind, BinaryOp as IrBinaryOp, IrExpr, Lit, StringOp};
use crate::ir::plan::{
    ApplyKind, DistinctBulk, DistinctMode, Node, NullsOrder, ProjectErrorPolicy, ProjectMode,
    ProjectionItem, QuantifierKind as IrQuantifierKind, Slice, SortDir, SortKey,
};
use crate::ir::policy::{OptionalMissing, PropertyMissing, ResultForm};
use crate::language::cypher::ast::{
    BinaryOp, Clause, ExistsSubquery, Expr, Literal, PatternPart, ProjectionBody, QuantifierKind,
    Query, ReturnClause, SortDirection, UnaryOp, WithClause,
};
use crate::language::cypher::planner::error::{CypherPlanError, CypherPlanResult};
use crate::language::cypher::planner::lowering::{
    Lowerer, context::CypherTraversalKind, pattern, predicate,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn lower_with(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &WithClause,
) -> CypherPlanResult<Node> {
    validate_with_projection_aliases(&clause.projection)?;
    let predicate_placement = clause
        .predicate
        .as_ref()
        .map(|predicate| lower_with_predicate_placement(lowerer, &clause.projection, predicate))
        .transpose()?;

    let input = match &predicate_placement {
        Some(WithPredicatePlacement::BeforeProjection(predicate)) => {
            lower_with_where_predicate(lowerer, input, &predicate)?
        }
        Some(WithPredicatePlacement::AfterProjection) | None => input,
    };
    let (node, fields) = lower_with_projection_body(lowerer, input, &clause.projection)?;
    lowerer.replace_scope(fields.clone());
    let node = match (&clause.predicate, predicate_placement) {
        (Some(filter), Some(WithPredicatePlacement::AfterProjection)) => {
            lower_with_where_predicate(lowerer, node, filter)?
        }
        _ => node,
    };
    Ok(node)
}

enum WithPredicatePlacement {
    BeforeProjection(Expr),
    AfterProjection,
}

fn lower_with_projection_body(
    lowerer: &mut Lowerer,
    input: Node,
    projection: &ProjectionBody,
) -> CypherPlanResult<(Node, Vec<String>)> {
    lowerer.with_child_traversal(CypherTraversalKind::WithProjection, |lowerer| {
        let result = lower_projection_body(lowerer, input, projection, true);
        if let Ok((_, fields)) = &result {
            lowerer.record_current_imports(lowerer.visible_fields());
            lowerer.record_current_outputs(fields.clone());
        }
        result
    })
}

fn lower_with_where_predicate(
    lowerer: &mut Lowerer,
    input: Node,
    filter: &Expr,
) -> CypherPlanResult<Node> {
    lowerer.with_child_traversal(CypherTraversalKind::WherePredicate, |lowerer| {
        let result = predicate::lower_where_predicate(lowerer, input, filter);
        if result.is_ok() {
            lowerer.record_current_imports(lowerer.visible_fields());
            lowerer.record_current_correlation(lowerer.visible_fields());
        }
        result
    })
}

fn lower_with_predicate_placement(
    lowerer: &Lowerer,
    body: &ProjectionBody,
    predicate: &Expr,
) -> CypherPlanResult<WithPredicatePlacement> {
    let source_fields = lowerer.visible_set();
    let projected_fields = projection_output_names(body, &source_fields);
    if validate_expression_refs(predicate, &projected_fields, "WHERE predicate").is_ok() {
        return Ok(WithPredicatePlacement::AfterProjection);
    }

    let has_aggregate = body.items.iter().any(|item| contains_aggregate(&item.expr));
    if has_aggregate {
        validate_expression_refs(predicate, &projected_fields, "WHERE predicate")?;
        return Ok(WithPredicatePlacement::AfterProjection);
    }

    let substituted = substitute_projection_aliases(predicate, &projection_aliases(body));
    validate_expression_refs(&substituted, &source_fields, "WHERE predicate")?;
    Ok(WithPredicatePlacement::BeforeProjection(substituted))
}

fn validate_with_projection_aliases(body: &ProjectionBody) -> CypherPlanResult<()> {
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
        ))
    }
}

pub fn lower_return(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &ReturnClause,
) -> CypherPlanResult<Node> {
    let (input, fields) =
        lowerer.with_child_traversal(CypherTraversalKind::ReturnProjection, |lowerer| {
            let (input, fields) = lower_projection_body(lowerer, input, &clause.projection, true)?;
            lowerer.record_current_imports(lowerer.visible_fields());
            lowerer.record_current_outputs(fields.clone());
            Ok((input, fields))
        })?;
    lowerer.replace_scope(fields.clone());
    lowerer.set_result_fields(fields.clone());
    Ok(Node::GraphReturn {
        fields,
        result_form: ResultForm::RowSet,
        input: input.boxed(),
    })
}

fn lower_projection_body(
    lowerer: &mut Lowerer,
    input: Node,
    body: &ProjectionBody,
    replace_scope: bool,
) -> CypherPlanResult<(Node, Vec<String>)> {
    let mut fields = Vec::new();
    let mut projection_items = Vec::new();
    let existing_fields = if body.include_existing {
        lowerer.visible_fields()
    } else {
        Vec::new()
    };
    if body.include_existing && existing_fields.is_empty() {
        return Err(CypherPlanError::Invalid(
            "RETURN or WITH * is not allowed when there are no variables in scope".to_string(),
        ));
    }
    let source_fields = lowerer.visible_fields();

    let has_aggregate = body.items.iter().any(|item| contains_aggregate(&item.expr));
    let mut precomputed_sort_keys = None;
    let mut planned_sort_keys = None;
    let mut hidden_sort_fields = Vec::new();
    if body.include_existing && !has_aggregate {
        for visible in &existing_fields {
            fields.push(visible.clone());
            projection_items.push(ProjectionItem {
                alias: visible.clone(),
                expr: IrExpr::Binding(visible.clone()),
            });
        }
    }

    let mut node = if has_aggregate {
        let aggregate = lower_aggregate(lowerer, input, body, &existing_fields, &mut fields)?;
        precomputed_sort_keys = aggregate.sort_keys;
        hidden_sort_fields = aggregate.hidden_sort_fields;
        aggregate.node
    } else {
        let mut node = input;
        for item in &body.items {
            validate_expression_scope(lowerer, &item.expr, "projection expression")?;
            let alias = item
                .alias
                .clone()
                .or_else(|| item.expr.variable_name().map(ToString::to_string))
                .unwrap_or_else(|| lowerer.synthetic("expr"));
            let (next, expr) = lower_expr_with_input(lowerer, node, &item.expr)?;
            node = next;
            fields.push(alias.clone());
            projection_items.push(ProjectionItem { alias, expr });
        }
        let shadowed_source_fields = shadowed_projection_fields(body, &source_fields);
        let projection_aliases = projection_aliases(body);
        if !body.order_by.is_empty() {
            let mut plans = Vec::new();
            let order_candidates = source_fields
                .iter()
                .chain(fields.iter())
                .cloned()
                .collect::<BTreeSet<_>>();
            for item in &body.order_by {
                validate_expression_refs(&item.expr, &order_candidates, "ORDER BY expression")?;
                if let Some(expr) =
                    order_expr_after_cardinality_projection(lowerer, body, &fields, &item.expr)?
                {
                    plans.push(ProjectionSortPlan::Ready(sort_key(expr, item.direction)));
                    continue;
                }
                if body.distinct {
                    return Err(invalid_order_scope());
                }
                if contains_aggregate(&item.expr) {
                    return Err(CypherPlanError::Invalid(
                        "Binder exception: Cannot evaluate expression with type AGGREGATE_FUNCTION."
                            .to_string(),
                    ).classified(crate::language::cypher::planner::CypherSemanticError::InvalidAggregation));
                }
                let refs = free_variable_names_for_sort(&item.expr, &source_fields, &fields);
                let has_projection_only_ref = refs.iter().any(|name| {
                    fields.iter().any(|field| field == name)
                        && (!source_fields.iter().any(|field| field == name)
                            || shadowed_source_fields.contains(name))
                });
                let sort_expr = if has_projection_only_ref {
                    let refs_in_projection = refs
                        .iter()
                        .all(|name| fields.iter().any(|field| field == name));
                    if has_projection_only_ref && refs_in_projection {
                        plans.push(ProjectionSortPlan::Deferred {
                            expr: item.expr.clone(),
                            direction: item.direction,
                        });
                        continue;
                    }
                    if requires_scoped_materialization(&item.expr) {
                        return Err(CypherPlanError::Invalid(
                            "ORDER BY scoped expressions may not mix projected aliases with unprojected source variables"
                                .to_string(),
                        ));
                    }
                    substitute_projection_aliases(&item.expr, &projection_aliases)
                } else {
                    item.expr.clone()
                };
                let (next, expr) = lower_expr_with_input(lowerer, node, &sort_expr)?;
                node = next;
                let alias = lowerer.synthetic("sort");
                projection_items.push(ProjectionItem {
                    alias: alias.clone(),
                    expr,
                });
                hidden_sort_fields.push(alias.clone());
                plans.push(ProjectionSortPlan::Ready(sort_key(
                    IrExpr::Binding(alias),
                    item.direction,
                )));
            }
            planned_sort_keys = Some(plans);
        }
        Node::GraphProject {
            mode: if replace_scope {
                ProjectMode::ReplaceScope
            } else {
                ProjectMode::PreserveVisible
            },
            items: projection_items,
            error_policy: ProjectErrorPolicy::PropagateError,
            input: node.boxed(),
        }
    };

    validate_unique_fields(&fields)?;

    if body.distinct {
        node = Node::GraphDistinct {
            keys: fields.clone(),
            mode: DistinctMode::Row,
            bulk: DistinctBulk::NotApplicable,
            input: node.boxed(),
        };
    }
    if !body.order_by.is_empty() {
        let keys = if let Some(plans) = planned_sort_keys {
            let (next, keys, deferred_hidden) =
                lower_planned_sort_keys(lowerer, node, &fields, plans)?;
            node = next;
            hidden_sort_fields.extend(deferred_hidden);
            keys
        } else if let Some(keys) = precomputed_sort_keys {
            keys
        } else {
            let mut keys = Vec::new();
            for item in &body.order_by {
                let (next, expr) = lower_expr_with_input(lowerer, node, &item.expr)?;
                node = next;
                keys.push(sort_key(expr, item.direction));
            }
            keys
        };
        node = Node::GraphSort {
            keys,
            input: node.boxed(),
        };
    }
    if !hidden_sort_fields.is_empty() {
        let items = fields
            .iter()
            .map(|field| ProjectionItem {
                alias: field.clone(),
                expr: IrExpr::Binding(field.clone()),
            })
            .collect();
        node = Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items,
            error_policy: ProjectErrorPolicy::PropagateError,
            input: node.boxed(),
        };
    }
    match slice_from_projection(lowerer, body, &source_fields)? {
        ProjectionSlice::None => {}
        ProjectionSlice::Static(slice) => {
            node = Node::GraphSlice {
                slice,
                input: node.boxed(),
            };
        }
        ProjectionSlice::Dynamic { offset, fetch } => {
            node = Node::GraphSliceExpr {
                offset,
                fetch,
                input: node.boxed(),
            };
        }
    }
    Ok((node, fields))
}

enum ProjectionSortPlan {
    Ready(SortKey),
    Deferred {
        expr: Expr,
        direction: SortDirection,
    },
}

fn lower_planned_sort_keys(
    lowerer: &mut Lowerer,
    input: Node,
    fields: &[String],
    plans: Vec<ProjectionSortPlan>,
) -> CypherPlanResult<(Node, Vec<SortKey>, Vec<String>)> {
    lowerer.with_preserved_scope(|lowerer| {
        lowerer.replace_scope(fields.to_vec());
        let mut node = input;
        let mut keys = Vec::with_capacity(plans.len());
        let mut hidden = Vec::new();
        for plan in plans {
            match plan {
                ProjectionSortPlan::Ready(key) => keys.push(key),
                ProjectionSortPlan::Deferred { expr, direction } => {
                    let (next, expr) = lower_expr_with_input(lowerer, node, &expr)?;
                    node = next;
                    let alias = lowerer.synthetic("sort");
                    node = Node::GraphProject {
                        mode: ProjectMode::PreserveVisible,
                        items: vec![ProjectionItem {
                            alias: alias.clone(),
                            expr,
                        }],
                        error_policy: ProjectErrorPolicy::PropagateError,
                        input: node.boxed(),
                    };
                    hidden.push(alias.clone());
                    keys.push(sort_key(IrExpr::Binding(alias), direction));
                }
            }
        }
        Ok((node, keys, hidden))
    })
}

fn sort_key(expr: IrExpr, direction: SortDirection) -> SortKey {
    SortKey {
        expr,
        dir: match direction {
            SortDirection::Asc => SortDir::Asc,
            SortDirection::Desc => SortDir::Desc,
        },
        nulls: match direction {
            SortDirection::Asc => NullsOrder::Last,
            SortDirection::Desc => NullsOrder::First,
        },
    }
}

fn order_expr_after_cardinality_projection(
    lowerer: &Lowerer,
    body: &ProjectionBody,
    fields: &[String],
    expr: &Expr,
) -> CypherPlanResult<Option<IrExpr>> {
    if let Expr::Variable(name) = expr {
        if fields.iter().any(|field| field == name) {
            return Ok(Some(IrExpr::Binding(name.clone())));
        }
    }

    let item_offset = fields.len().saturating_sub(body.items.len());
    for (index, item) in body.items.iter().enumerate() {
        if item.expr == *expr {
            if let Some(field) = fields.get(item_offset + index) {
                return Ok(Some(IrExpr::Binding(field.clone())));
            }
        }
    }

    // ORDER BY may compose already projected grouping keys and aggregates.
    // Rewrite each matching subexpression to its output binding before
    // checking scope; an unprojected aggregate must still be rejected.
    let rewritten = rewrite_projected_sort_expr(body, fields, expr);
    if contains_aggregate(&rewritten) || requires_scoped_materialization(&rewritten) {
        return Ok(None);
    }
    let mut refs = BTreeSet::new();
    collect_free_variables(&rewritten, &mut BTreeSet::new(), &mut refs);
    if refs.iter().all(|name| fields.contains(name)) {
        return Ok(Some(lower_expr(lowerer, &rewritten)?));
    }
    Ok(None)
}

fn rewrite_projected_sort_expr(body: &ProjectionBody, fields: &[String], expr: &Expr) -> Expr {
    if let Expr::Variable(name) = expr {
        if fields.contains(name) {
            return expr.clone();
        }
    }
    let offset = fields.len().saturating_sub(body.items.len());
    for (index, item) in body.items.iter().enumerate() {
        if item.expr == *expr {
            if let Some(field) = fields.get(offset + index) {
                return Expr::Variable(field.clone());
            }
        }
    }
    let rewrite = |expr: &Expr| rewrite_projected_sort_expr(body, fields, expr);
    let boxed = |expr: &Expr| Box::new(rewrite(expr));
    match expr {
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: *op,
            lhs: boxed(lhs),
            rhs: boxed(rhs),
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op: *op,
            expr: boxed(expr),
        },
        Expr::Property { target, key } => Expr::Property {
            target: boxed(target),
            key: key.clone(),
        },
        Expr::IsNull(expr) => Expr::IsNull(boxed(expr)),
        Expr::IsNotNull(expr) => Expr::IsNotNull(boxed(expr)),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => Expr::StringPredicate {
            op: *op,
            target: boxed(target),
            pattern: boxed(pattern),
        },
        Expr::Function {
            name,
            distinct,
            args,
        } => Expr::Function {
            name: name.clone(),
            distinct: *distinct,
            args: args.iter().map(rewrite).collect(),
        },
        Expr::List(items) => Expr::List(items.iter().map(rewrite).collect()),
        Expr::Map(items) => Expr::Map(
            items
                .iter()
                .map(|(key, value)| (key.clone(), rewrite(value)))
                .collect(),
        ),
        Expr::Case {
            case,
            arms,
            otherwise,
        } => Expr::Case {
            case: case.as_ref().map(|value| boxed(value)),
            arms: arms
                .iter()
                .map(|(when, then)| (rewrite(when), rewrite(then)))
                .collect(),
            otherwise: otherwise.as_ref().map(|value| boxed(value)),
        },
        _ => expr.clone(),
    }
}

fn invalid_order_scope() -> CypherPlanError {
    CypherPlanError::Invalid(
        "ORDER BY after DISTINCT or aggregation may only reference projected variables or projected expressions".to_string(),
    )
}

fn validate_unique_fields(fields: &[String]) -> CypherPlanResult<()> {
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
            "projection contains duplicate column names: {}",
            duplicates.join(", ")
        )))
    }
}

fn validate_slice_expr_scope(
    clause: &str,
    expr: &Expr,
    source_fields: &[String],
) -> CypherPlanResult<()> {
    let mut refs = BTreeSet::new();
    collect_free_variables(expr, &mut BTreeSet::new(), &mut refs);
    let candidates = source_fields.iter().cloned().collect::<BTreeSet<_>>();
    remove_local_exists_bindings(expr, &candidates, &mut refs);
    add_candidate_pattern_bindings(expr, &candidates, &mut refs);
    if refs.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "{clause} expressions may not depend on graph variables"
        )).classified(crate::language::cypher::planner::CypherSemanticError::NonConstantExpression))
    }
}

enum ProjectionSlice {
    None,
    Static(Slice),
    Dynamic {
        offset: Option<IrExpr>,
        fetch: Option<IrExpr>,
    },
}

fn slice_from_projection(
    lowerer: &Lowerer,
    body: &ProjectionBody,
    source_fields: &[String],
) -> CypherPlanResult<ProjectionSlice> {
    if let Some(expr) = &body.skip {
        validate_slice_expr_scope("SKIP", expr, source_fields)?;
    }
    if let Some(expr) = &body.limit {
        validate_slice_expr_scope("LIMIT", expr, source_fields)?;
    }
    if body.skip.is_none() && body.limit.is_none() {
        return Ok(ProjectionSlice::None);
    }

    let offset = body.skip.as_ref().map(literal_u64).transpose()?;
    let fetch = body.limit.as_ref().map(literal_u64).transpose()?;
    let dynamic = matches!(offset, Some(None)) || matches!(fetch, Some(None));
    if !dynamic {
        let slice = Slice {
            offset: offset.flatten().unwrap_or(0),
            fetch: fetch.flatten(),
            tail: None,
        };
        if slice == Slice::NONE {
            return Ok(ProjectionSlice::None);
        }
        return Ok(ProjectionSlice::Static(slice));
    }

    Ok(ProjectionSlice::Dynamic {
        offset: body
            .skip
            .as_ref()
            .map(|expr| lower_expr(lowerer, expr))
            .transpose()?,
        fetch: body
            .limit
            .as_ref()
            .map(|expr| lower_expr(lowerer, expr))
            .transpose()?,
    })
}

fn literal_u64(expr: &Expr) -> CypherPlanResult<Option<u64>> {
    match expr {
        Expr::Literal(Literal::Integer(value)) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| CypherPlanError::Invalid("slice bound is outside u64 range".to_string())),
        Expr::Literal(Literal::Float(_)) => Err(CypherPlanError::Invalid(
            "Runtime exception: The number of rows to skip/limit must be a non-negative integer."
                .to_string(),
        ).classified(crate::language::cypher::planner::CypherSemanticError::InvalidArgumentType)),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => {
            if let Some(value) = literal_u64(expr)? {
                if value == 0 { return Ok(Some(0)); }
                Err(CypherPlanError::Invalid(
                    "Runtime exception: The number of rows to skip/limit must be a non-negative integer."
                        .to_string(),
                ).classified(crate::language::cypher::planner::CypherSemanticError::NegativeIntegerArgument))
            } else {
                Ok(None)
            }
        }
        Expr::Unary { .. } => Ok(None),
        Expr::Binary { op, lhs, rhs } => {
            let Some(lhs) = literal_u64(lhs)? else {
                return Ok(None);
            };
            let Some(rhs) = literal_u64(rhs)? else {
                return Ok(None);
            };
            let value = match op {
                BinaryOp::Add => lhs.checked_add(rhs),
                BinaryOp::Sub => lhs.checked_sub(rhs),
                BinaryOp::Mul => lhs.checked_mul(rhs),
                BinaryOp::Div if rhs != 0 => Some(lhs / rhs),
                _ => None,
            };
            Ok(value)
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::cypher::ast::ProjectionItem as AstProjectionItem;

    fn projection_body(items: Vec<AstProjectionItem>) -> ProjectionBody {
        ProjectionBody {
            distinct: false,
            include_existing: false,
            items,
            order_by: Vec::new(),
            skip: None,
            limit: None,
        }
    }

    fn property(target: Expr, key: &str) -> Expr {
        Expr::Property {
            target: Box::new(target),
            key: key.to_string(),
        }
    }

    fn aliased(expr: Expr, alias: &str) -> AstProjectionItem {
        AstProjectionItem {
            expr,
            alias: Some(alias.to_string()),
            explicit_alias: true,
        }
    }

    #[test]
    fn with_where_using_incoming_variable_filters_before_projection() {
        let mut lowerer = Lowerer::new();
        lowerer.add_visible("a");
        let body = projection_body(vec![aliased(
            property(Expr::Variable("a".to_string()), "name"),
            "name",
        )]);
        let predicate = Expr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(property(Expr::Variable("a".to_string()), "name")),
            rhs: Box::new(Expr::Literal(Literal::String("B".to_string()))),
        };

        assert!(matches!(
            lower_with_predicate_placement(&lowerer, &body, &predicate).unwrap(),
            WithPredicatePlacement::BeforeProjection(_)
        ));
    }

    #[test]
    fn with_where_using_aggregate_alias_filters_after_projection() {
        let lowerer = Lowerer::new();
        let body = projection_body(vec![aliased(Expr::CountStar, "count")]);
        let predicate = Expr::Binary {
            op: BinaryOp::Gt,
            lhs: Box::new(Expr::Variable("count".to_string())),
            rhs: Box::new(Expr::Literal(Literal::Integer("0".to_string()))),
        };

        assert!(matches!(
            lower_with_predicate_placement(&lowerer, &body, &predicate).unwrap(),
            WithPredicatePlacement::AfterProjection
        ));
    }

    #[test]
    fn literal_float_slice_bound_is_invalid() {
        let err = literal_u64(&Expr::Literal(Literal::Float(1.5))).unwrap_err();
        assert!(format!("{err}").contains(
            "Runtime exception: The number of rows to skip/limit must be a non-negative integer."
        ));
    }
}
