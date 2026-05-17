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
        Err(CypherPlanError::Invalid(format!(
            "non-variable expressions in WITH must be aliased with AS: {}",
            missing.join(", ")
        )))
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
                        "ORDER BY aggregate expressions must be part of an aggregate projection"
                            .to_string(),
                    ));
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

fn lower_aggregate(
    lowerer: &mut Lowerer,
    input: Node,
    body: &ProjectionBody,
    existing_fields: &[String],
    fields: &mut Vec<String>,
) -> CypherPlanResult<AggregateLowering> {
    let (aggregate_fields, final_exprs, sort_keys, mut aggregate) =
        lowerer.with_child_traversal(CypherTraversalKind::Aggregation, |lowerer| {
            let mut rewrite = AggregateRewrite::default();
            let mut input = input;
            let mut final_exprs = Vec::new();
            for visible in existing_fields {
                fields.push(visible.clone());
                rewrite.group.push(ProjectionItem {
                    alias: visible.clone(),
                    expr: IrExpr::Binding(visible.clone()),
                });
                final_exprs.push((visible.clone(), Expr::Variable(visible.clone())));
            }
            for item in &body.items {
                validate_expression_scope(lowerer, &item.expr, "aggregate projection expression")?;
                let (next, expr) = materialize_pre_aggregate_expr(lowerer, input, &item.expr)?;
                input = next;
                let alias = item
                    .alias
                    .clone()
                    .or_else(|| expr.variable_name().map(ToString::to_string))
                    .unwrap_or_else(|| lowerer.synthetic("agg"));
                fields.push(alias.clone());
                let expr = rewrite_aggregate_projection_expr(
                    lowerer,
                    &expr,
                    &mut rewrite,
                    Some(&alias),
                    &mut BTreeSet::new(),
                )?;
                final_exprs.push((alias, expr));
            }
            let mut sort_keys = Vec::new();
            for item in &body.order_by {
                let Some(expr) =
                    order_expr_after_cardinality_projection(lowerer, body, fields, &item.expr)?
                else {
                    return Err(invalid_order_scope());
                };
                sort_keys.push(sort_key(expr, item.direction));
            }
            lowerer.record_current_outputs(fields.clone());
            let aggregate_fields: Vec<String> = rewrite
                .group
                .iter()
                .map(|item| item.alias.clone())
                .chain(rewrite.aggs.iter().map(|agg| agg.alias.clone()))
                .collect();
            let aggregate = Node::GraphAggregate {
                group: rewrite.group,
                aggs: rewrite.aggs,
                fields: aggregate_fields.clone(),
                input: input.boxed(),
            };
            Ok((aggregate_fields, final_exprs, sort_keys, aggregate))
        })?;
    let final_items = lowerer.with_preserved_scope(|lowerer| {
        lowerer.replace_scope(aggregate_fields);
        let mut items = Vec::with_capacity(final_exprs.len());
        for (alias, expr) in final_exprs {
            let current = std::mem::replace(&mut aggregate, Node::GraphOneRow);
            let (next, expr) = lower_expr_with_input(lowerer, current, &expr)?;
            aggregate = next;
            items.push(ProjectionItem { alias, expr });
        }
        Ok(items)
    })?;
    Ok(AggregateLowering {
        node: Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items: final_items,
            error_policy: ProjectErrorPolicy::PropagateError,
            input: aggregate.boxed(),
        },
        sort_keys: (!sort_keys.is_empty()).then_some(sort_keys),
        hidden_sort_fields: Vec::new(),
    })
}

struct AggregateLowering {
    node: Node,
    sort_keys: Option<Vec<SortKey>>,
    hidden_sort_fields: Vec<String>,
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

#[derive(Default)]
struct AggregateRewrite {
    group: Vec<ProjectionItem>,
    aggs: Vec<AggCall>,
}

fn rewrite_aggregate_projection_expr(
    lowerer: &mut Lowerer,
    expr: &Expr,
    rewrite: &mut AggregateRewrite,
    preferred_alias: Option<&str>,
    bound: &mut BTreeSet<String>,
) -> CypherPlanResult<Expr> {
    match expr {
        Expr::CountStar => {
            let alias = preferred_alias
                .map(ToString::to_string)
                .unwrap_or_else(|| lowerer.synthetic("agg"));
            rewrite.aggs.push(AggCall {
                kind: AggKind::CountRows,
                alias: alias.clone(),
                arg: None,
                distinct: false,
            });
            Ok(Expr::Variable(alias))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } if aggregate_kind(name).is_some() => {
            if args.iter().any(contains_aggregate) {
                return Err(CypherPlanError::Unsupported(
                    "nested aggregate expressions are not valid Cypher".to_string(),
                ));
            }
            for arg in args {
                let local_refs = free_variable_names_for_local_scope(arg, bound)
                    .into_iter()
                    .filter(|name| bound.contains(name))
                    .collect::<Vec<_>>();
                if !local_refs.is_empty() {
                    return Err(CypherPlanError::Invalid(format!(
                        "aggregate function `{name}` may not reference variables local to a scoped expression: {}",
                        local_refs.join(", ")
                    )));
                }
            }
            let alias = preferred_alias
                .map(ToString::to_string)
                .unwrap_or_else(|| lowerer.synthetic("agg"));
            let name_lower = name.to_ascii_lowercase();
            let kind = match name_lower.as_str() {
                "count" => {
                    if *distinct {
                        AggKind::CountDistinct
                    } else {
                        AggKind::CountRows
                    }
                }
                "count_if" => AggKind::CountIf,
                "sum" => AggKind::SumOrZero,
                "avg" => AggKind::AvgOrNull,
                "min" => AggKind::MinOrNull,
                "max" => AggKind::MaxOrNull,
                "stdev" => AggKind::StDev,
                "stdevp" => AggKind::StDevP,
                "percentilecont" => AggKind::PercentileCont,
                "percentiledisc" => AggKind::PercentileDisc,
                "collect" => AggKind::CollectRows,
                _ => unreachable!("aggregate_kind filtered aggregate functions"),
            };
            let arg = match kind {
                AggKind::PercentileCont | AggKind::PercentileDisc => {
                    if args.len() != 2 {
                        return Err(CypherPlanError::Invalid(format!(
                            "aggregate function `{name}` requires exactly two arguments"
                        )));
                    }
                    Some(IrExpr::List(vec![
                        lower_expr(lowerer, &args[0])?,
                        lower_expr(lowerer, &args[1])?,
                    ]))
                }
                _ => {
                    if args.len() != 1 {
                        return Err(CypherPlanError::Invalid(format!(
                            "aggregate function `{name}` requires exactly one argument"
                        )));
                    }
                    Some(lower_expr(lowerer, &args[0])?)
                }
            };
            rewrite.aggs.push(AggCall {
                kind,
                alias: alias.clone(),
                arg,
                distinct: *distinct,
            });
            Ok(Expr::Variable(alias))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            if !contains_aggregate(expr) {
                return rewrite_non_aggregate_projection_expr(
                    lowerer,
                    expr,
                    rewrite,
                    preferred_alias,
                    bound,
                );
            }
            if *distinct {
                return Err(CypherPlanError::Invalid(format!(
                    "DISTINCT is only valid for aggregate function `{name}`"
                )));
            }
            Ok(Expr::Function {
                name: name.clone(),
                distinct: false,
                args: args
                    .iter()
                    .map(|arg| {
                        rewrite_aggregate_projection_expr(lowerer, arg, rewrite, None, bound)
                    })
                    .collect::<CypherPlanResult<_>>()?,
            })
        }
        _ if !contains_aggregate(expr) => {
            rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, preferred_alias, bound)
        }
        Expr::Unary { op, expr } => Ok(Expr::Unary {
            op: *op,
            expr: Box::new(rewrite_aggregate_projection_expr(
                lowerer, expr, rewrite, None, bound,
            )?),
        }),
        Expr::Binary { op, lhs, rhs } => Ok(Expr::Binary {
            op: *op,
            lhs: Box::new(rewrite_aggregate_projection_expr(
                lowerer, lhs, rewrite, None, bound,
            )?),
            rhs: Box::new(rewrite_aggregate_projection_expr(
                lowerer, rhs, rewrite, None, bound,
            )?),
        }),
        Expr::Property { target, key } => Ok(Expr::Property {
            target: Box::new(rewrite_aggregate_projection_expr(
                lowerer, target, rewrite, None, bound,
            )?),
            key: key.clone(),
        }),
        Expr::LabelPredicate { target, labels } => Ok(Expr::LabelPredicate {
            target: Box::new(rewrite_aggregate_projection_expr(
                lowerer, target, rewrite, None, bound,
            )?),
            labels: labels.clone(),
        }),
        Expr::IsNull(expr) => Ok(Expr::IsNull(Box::new(rewrite_aggregate_projection_expr(
            lowerer, expr, rewrite, None, bound,
        )?))),
        Expr::IsNotNull(expr) => Ok(Expr::IsNotNull(Box::new(
            rewrite_aggregate_projection_expr(lowerer, expr, rewrite, None, bound)?,
        ))),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => Ok(Expr::StringPredicate {
            op: *op,
            target: Box::new(rewrite_aggregate_projection_expr(
                lowerer, target, rewrite, None, bound,
            )?),
            pattern: Box::new(rewrite_aggregate_projection_expr(
                lowerer, pattern, rewrite, None, bound,
            )?),
        }),
        Expr::Case {
            case,
            arms,
            otherwise,
        } => Ok(Expr::Case {
            case: case
                .as_ref()
                .map(|expr| rewrite_aggregate_projection_expr(lowerer, expr, rewrite, None, bound))
                .transpose()?
                .map(Box::new),
            arms: arms
                .iter()
                .map(|(when, then)| {
                    Ok((
                        rewrite_aggregate_projection_expr(lowerer, when, rewrite, None, bound)?,
                        rewrite_aggregate_projection_expr(lowerer, then, rewrite, None, bound)?,
                    ))
                })
                .collect::<CypherPlanResult<_>>()?,
            otherwise: otherwise
                .as_ref()
                .map(|expr| rewrite_aggregate_projection_expr(lowerer, expr, rewrite, None, bound))
                .transpose()?
                .map(Box::new),
        }),
        Expr::List(items) => Ok(Expr::List(
            items
                .iter()
                .map(|item| rewrite_aggregate_projection_expr(lowerer, item, rewrite, None, bound))
                .collect::<CypherPlanResult<_>>()?,
        )),
        Expr::Map(items) => Ok(Expr::Map(
            items
                .iter()
                .map(|(key, value)| {
                    Ok((
                        key.clone(),
                        rewrite_aggregate_projection_expr(lowerer, value, rewrite, None, bound)?,
                    ))
                })
                .collect::<CypherPlanResult<_>>()?,
        )),
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            let collection =
                rewrite_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = predicate
                .as_ref()
                .map(|expr| rewrite_aggregate_projection_expr(lowerer, expr, rewrite, None, bound))
                .transpose()?
                .map(Box::new);
            let map = rewrite_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListComprehension {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate,
                map: Box::new(map),
            })
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let collection =
                rewrite_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let acc_was_bound = bound.contains(accumulator);
            let variable_was_bound = bound.contains(variable);
            bound.insert(accumulator.clone());
            bound.insert(variable.clone());
            let map = rewrite_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            if !acc_was_bound {
                bound.remove(accumulator);
            }
            if !variable_was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListReduce {
                accumulator: accumulator.clone(),
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            })
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let collection =
                rewrite_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let map = rewrite_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListTransform {
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            })
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let collection =
                rewrite_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate =
                rewrite_aggregate_projection_expr(lowerer, predicate, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListFilter {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            })
        }
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => {
            let collection =
                rewrite_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate =
                rewrite_aggregate_projection_expr(lowerer, predicate, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::Quantifier {
                kind: *kind,
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            })
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let pattern = rewrite_aggregate_pattern(lowerer, pattern, rewrite, bound)?;
            let local_names = pattern_binding_names(&pattern);
            let previously_bound = local_names
                .iter()
                .filter(|name| bound.contains(*name))
                .cloned()
                .collect::<BTreeSet<_>>();
            for name in &local_names {
                bound.insert(name.clone());
            }
            let variable_was_bound = variable.as_ref().is_some_and(|name| bound.contains(name));
            if let Some(variable) = variable {
                bound.insert(variable.clone());
            }
            let predicate = predicate
                .as_ref()
                .map(|expr| rewrite_aggregate_projection_expr(lowerer, expr, rewrite, None, bound))
                .transpose()?
                .map(Box::new);
            let map = rewrite_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            for name in &local_names {
                if !previously_bound.contains(name) {
                    bound.remove(name);
                }
            }
            if let Some(variable) = variable {
                if !variable_was_bound {
                    bound.remove(variable);
                }
            }
            Ok(Expr::PatternComprehension {
                variable: variable.clone(),
                pattern: Box::new(pattern),
                predicate,
                map: Box::new(map),
            })
        }
        Expr::Exists(_) | Expr::PatternPredicate(_) => {
            rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, preferred_alias, bound)
        }
        Expr::Star | Expr::Variable(_) | Expr::Parameter(_) | Expr::Literal(_) => {
            rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, preferred_alias, bound)
        }
    }
}

fn rewrite_non_aggregate_projection_expr(
    lowerer: &mut Lowerer,
    expr: &Expr,
    rewrite: &mut AggregateRewrite,
    preferred_alias: Option<&str>,
    bound: &mut BTreeSet<String>,
) -> CypherPlanResult<Expr> {
    let candidates = lowerer.visible_set();
    let mut refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, &candidates, &mut refs);
    add_candidate_pattern_bindings(expr, &candidates, &mut refs);
    refs.retain(|name| candidates.contains(name) || bound.contains(name));
    if refs.is_empty() || refs.iter().all(|name| bound.contains(name)) {
        return Ok(expr.clone());
    }
    if requires_scoped_materialization(expr) {
        ensure_scoped_outer_refs_grouped(lowerer, rewrite, &refs, bound)?;
        return Ok(expr.clone());
    }
    if refs.iter().all(|name| !bound.contains(name)) && !requires_scoped_materialization(expr) {
        return ensure_group_key(lowerer, rewrite, expr, preferred_alias).map(Expr::Variable);
    }
    match expr {
        Expr::Variable(name) if !bound.contains(name) => {
            ensure_group_key(lowerer, rewrite, expr, preferred_alias).map(Expr::Variable)
        }
        Expr::Property { target, key } => Ok(Expr::Property {
            target: Box::new(rewrite_non_aggregate_projection_expr(
                lowerer, target, rewrite, None, bound,
            )?),
            key: key.clone(),
        }),
        Expr::LabelPredicate { target, labels } => Ok(Expr::LabelPredicate {
            target: Box::new(rewrite_non_aggregate_projection_expr(
                lowerer, target, rewrite, None, bound,
            )?),
            labels: labels.clone(),
        }),
        Expr::Unary { op, expr } => Ok(Expr::Unary {
            op: *op,
            expr: Box::new(rewrite_non_aggregate_projection_expr(
                lowerer, expr, rewrite, None, bound,
            )?),
        }),
        Expr::Binary { op, lhs, rhs } => Ok(Expr::Binary {
            op: *op,
            lhs: Box::new(rewrite_non_aggregate_projection_expr(
                lowerer, lhs, rewrite, None, bound,
            )?),
            rhs: Box::new(rewrite_non_aggregate_projection_expr(
                lowerer, rhs, rewrite, None, bound,
            )?),
        }),
        Expr::IsNull(expr) => Ok(Expr::IsNull(Box::new(
            rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, None, bound)?,
        ))),
        Expr::IsNotNull(expr) => Ok(Expr::IsNotNull(Box::new(
            rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, None, bound)?,
        ))),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => Ok(Expr::StringPredicate {
            op: *op,
            target: Box::new(rewrite_non_aggregate_projection_expr(
                lowerer, target, rewrite, None, bound,
            )?),
            pattern: Box::new(rewrite_non_aggregate_projection_expr(
                lowerer, pattern, rewrite, None, bound,
            )?),
        }),
        Expr::Function {
            name,
            distinct,
            args,
        } => Ok(Expr::Function {
            name: name.clone(),
            distinct: *distinct,
            args: args
                .iter()
                .map(|arg| {
                    rewrite_non_aggregate_projection_expr(lowerer, arg, rewrite, None, bound)
                })
                .collect::<CypherPlanResult<_>>()?,
        }),
        Expr::Case {
            case,
            arms,
            otherwise,
        } => Ok(Expr::Case {
            case: case
                .as_ref()
                .map(|expr| {
                    rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, None, bound)
                })
                .transpose()?
                .map(Box::new),
            arms: arms
                .iter()
                .map(|(when, then)| {
                    Ok((
                        rewrite_non_aggregate_projection_expr(lowerer, when, rewrite, None, bound)?,
                        rewrite_non_aggregate_projection_expr(lowerer, then, rewrite, None, bound)?,
                    ))
                })
                .collect::<CypherPlanResult<_>>()?,
            otherwise: otherwise
                .as_ref()
                .map(|expr| {
                    rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, None, bound)
                })
                .transpose()?
                .map(Box::new),
        }),
        Expr::List(items) => Ok(Expr::List(
            items
                .iter()
                .map(|item| {
                    rewrite_non_aggregate_projection_expr(lowerer, item, rewrite, None, bound)
                })
                .collect::<CypherPlanResult<_>>()?,
        )),
        Expr::Map(items) => Ok(Expr::Map(
            items
                .iter()
                .map(|(key, value)| {
                    Ok((
                        key.clone(),
                        rewrite_non_aggregate_projection_expr(
                            lowerer, value, rewrite, None, bound,
                        )?,
                    ))
                })
                .collect::<CypherPlanResult<_>>()?,
        )),
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            let collection =
                rewrite_non_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = predicate
                .as_ref()
                .map(|expr| {
                    rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, None, bound)
                })
                .transpose()?
                .map(Box::new);
            let map = rewrite_non_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListComprehension {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate,
                map: Box::new(map),
            })
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let collection =
                rewrite_non_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let acc_was_bound = bound.contains(accumulator);
            let variable_was_bound = bound.contains(variable);
            bound.insert(accumulator.clone());
            bound.insert(variable.clone());
            let map = rewrite_non_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            if !acc_was_bound {
                bound.remove(accumulator);
            }
            if !variable_was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListReduce {
                accumulator: accumulator.clone(),
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            })
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let collection =
                rewrite_non_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let map = rewrite_non_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListTransform {
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            })
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let collection =
                rewrite_non_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate =
                rewrite_non_aggregate_projection_expr(lowerer, predicate, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::ListFilter {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            })
        }
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => {
            let collection =
                rewrite_non_aggregate_projection_expr(lowerer, collection, rewrite, None, bound)?;
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate =
                rewrite_non_aggregate_projection_expr(lowerer, predicate, rewrite, None, bound)?;
            if !was_bound {
                bound.remove(variable);
            }
            Ok(Expr::Quantifier {
                kind: *kind,
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            })
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let pattern = rewrite_aggregate_pattern(lowerer, pattern, rewrite, bound)?;
            let local_names = pattern_binding_names(&pattern);
            let previously_bound = local_names
                .iter()
                .filter(|name| bound.contains(*name))
                .cloned()
                .collect::<BTreeSet<_>>();
            for name in &local_names {
                bound.insert(name.clone());
            }
            let variable_was_bound = variable.as_ref().is_some_and(|name| bound.contains(name));
            if let Some(variable) = variable {
                bound.insert(variable.clone());
            }
            let predicate = predicate
                .as_ref()
                .map(|expr| {
                    rewrite_non_aggregate_projection_expr(lowerer, expr, rewrite, None, bound)
                })
                .transpose()?
                .map(Box::new);
            let map = rewrite_non_aggregate_projection_expr(lowerer, map, rewrite, None, bound)?;
            for name in &local_names {
                if !previously_bound.contains(name) {
                    bound.remove(name);
                }
            }
            if let Some(variable) = variable {
                if !variable_was_bound {
                    bound.remove(variable);
                }
            }
            Ok(Expr::PatternComprehension {
                variable: variable.clone(),
                pattern: Box::new(pattern),
                predicate,
                map: Box::new(map),
            })
        }
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => Ok(expr.clone()),
    }
}

fn ensure_scoped_outer_refs_grouped(
    lowerer: &mut Lowerer,
    rewrite: &mut AggregateRewrite,
    refs: &BTreeSet<String>,
    bound: &BTreeSet<String>,
) -> CypherPlanResult<()> {
    for name in refs.iter().filter(|name| !bound.contains(*name)) {
        ensure_group_key(lowerer, rewrite, &Expr::Variable(name.clone()), Some(name))?;
    }
    Ok(())
}

fn rewrite_aggregate_pattern(
    lowerer: &mut Lowerer,
    pattern: &PatternPart,
    rewrite: &mut AggregateRewrite,
    bound: &mut BTreeSet<String>,
) -> CypherPlanResult<PatternPart> {
    let mut pattern = pattern.clone();
    if let Some(properties) = &pattern.element.start.properties {
        pattern.element.start.properties = Some(rewrite_aggregate_projection_expr(
            lowerer, properties, rewrite, None, bound,
        )?);
    }
    for chain in &mut pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            chain.relationship.properties = Some(rewrite_aggregate_projection_expr(
                lowerer, properties, rewrite, None, bound,
            )?);
        }
        if let Some(properties) = &chain.node.properties {
            chain.node.properties = Some(rewrite_aggregate_projection_expr(
                lowerer, properties, rewrite, None, bound,
            )?);
        }
    }
    Ok(pattern)
}

#[allow(dead_code)]
fn rewrite_aggregate_projection(
    lowerer: &mut Lowerer,
    expr: &Expr,
    rewrite: &mut AggregateRewrite,
    preferred_alias: Option<&str>,
) -> CypherPlanResult<IrExpr> {
    if !contains_aggregate(expr) {
        let alias = ensure_group_key(lowerer, rewrite, expr, preferred_alias)?;
        return Ok(IrExpr::Binding(alias));
    }

    match expr {
        Expr::CountStar => {
            let alias = preferred_alias
                .map(ToString::to_string)
                .unwrap_or_else(|| lowerer.synthetic("agg"));
            rewrite.aggs.push(AggCall {
                kind: AggKind::CountRows,
                alias: alias.clone(),
                arg: None,
                distinct: false,
            });
            Ok(IrExpr::Binding(alias))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } if aggregate_kind(name).is_some() => {
            if args.iter().any(contains_aggregate) {
                return Err(CypherPlanError::Unsupported(
                    "nested aggregate expressions are not valid Cypher".to_string(),
                ));
            }
            let alias = preferred_alias
                .map(ToString::to_string)
                .unwrap_or_else(|| lowerer.synthetic("agg"));
            let name_lower = name.to_ascii_lowercase();
            let kind = match name_lower.as_str() {
                "count" => {
                    if *distinct {
                        AggKind::CountDistinct
                    } else {
                        AggKind::CountRows
                    }
                }
                "count_if" => AggKind::CountIf,
                "sum" => AggKind::SumOrZero,
                "avg" => AggKind::AvgOrNull,
                "min" => AggKind::MinOrNull,
                "max" => AggKind::MaxOrNull,
                "stdev" => AggKind::StDev,
                "stdevp" => AggKind::StDevP,
                "percentilecont" => AggKind::PercentileCont,
                "percentiledisc" => AggKind::PercentileDisc,
                "collect" => AggKind::CollectRows,
                _ => unreachable!("aggregate_kind filtered aggregate functions"),
            };
            let arg = match kind {
                AggKind::PercentileCont | AggKind::PercentileDisc => {
                    if args.len() != 2 {
                        return Err(CypherPlanError::Invalid(format!(
                            "aggregate function `{name}` requires exactly two arguments"
                        )));
                    }
                    Some(IrExpr::List(vec![
                        lower_expr(lowerer, &args[0])?,
                        lower_expr(lowerer, &args[1])?,
                    ]))
                }
                _ => {
                    if args.len() != 1 {
                        return Err(CypherPlanError::Invalid(format!(
                            "aggregate function `{name}` requires exactly one argument"
                        )));
                    }
                    Some(lower_expr(lowerer, &args[0])?)
                }
            };
            rewrite.aggs.push(AggCall {
                kind,
                alias: alias.clone(),
                arg,
                distinct: *distinct,
            });
            Ok(IrExpr::Binding(alias))
        }
        Expr::Unary { op, expr } => {
            let expr = rewrite_aggregate_projection(lowerer, expr, rewrite, None)?;
            Ok(match op {
                UnaryOp::Not => IrExpr::Not(Box::new(expr)),
                UnaryOp::Neg => IrExpr::Binary {
                    op: IrBinaryOp::Sub,
                    lhs: Box::new(IrExpr::Lit(Lit::Int(0))),
                    rhs: Box::new(expr),
                },
            })
        }
        Expr::Binary { op, lhs, rhs } => {
            let lhs = rewrite_aggregate_projection(lowerer, lhs, rewrite, None)?;
            let rhs = rewrite_aggregate_projection(lowerer, rhs, rewrite, None)?;
            Ok(lower_cypher_binary_expr(*op, lhs, rhs))
        }
        Expr::Property { target, key } => {
            let target = rewrite_aggregate_projection(lowerer, target, rewrite, None)?;
            Ok(IrExpr::Call {
                name: "property".to_string(),
                args: vec![target, IrExpr::Lit(Lit::String(key.clone()))],
            })
        }
        Expr::LabelPredicate { target, labels } => {
            let target = rewrite_aggregate_projection(lowerer, target, rewrite, None)?;
            Ok(lower_label_predicate_expr(target, labels))
        }
        Expr::IsNull(expr) => {
            let expr = rewrite_aggregate_projection(lowerer, expr, rewrite, None)?;
            Ok(IrExpr::IsNull(Box::new(expr)))
        }
        Expr::IsNotNull(expr) => {
            let expr = rewrite_aggregate_projection(lowerer, expr, rewrite, None)?;
            Ok(IrExpr::IsNotNull(Box::new(expr)))
        }
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => {
            let target = rewrite_aggregate_projection(lowerer, target, rewrite, None)?;
            let pattern = rewrite_aggregate_projection(lowerer, pattern, rewrite, None)?;
            Ok(lower_string_predicate_expr(*op, target, pattern))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            if *distinct {
                return Err(CypherPlanError::Invalid(format!(
                    "DISTINCT is only valid for aggregate function `{name}`"
                )));
            }
            let args = args
                .iter()
                .map(|arg| rewrite_aggregate_projection(lowerer, arg, rewrite, None))
                .collect::<CypherPlanResult<Vec<_>>>()?;
            Ok(IrExpr::Call {
                name: name.clone(),
                args,
            })
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            let case_expr = case
                .as_ref()
                .map(|expr| rewrite_aggregate_projection(lowerer, expr, rewrite, None))
                .transpose()?;
            let mut lowered_arms = Vec::new();
            for (when, then) in arms {
                let when = rewrite_aggregate_projection(lowerer, when, rewrite, None)?;
                let condition = if let Some(case_expr) = &case_expr {
                    IrExpr::Binary {
                        op: IrBinaryOp::Eq,
                        lhs: Box::new(case_expr.clone()),
                        rhs: Box::new(when),
                    }
                } else {
                    when
                };
                let then = rewrite_aggregate_projection(lowerer, then, rewrite, None)?;
                lowered_arms.push((condition, then));
            }
            Ok(IrExpr::Case {
                arms: lowered_arms,
                otherwise: otherwise
                    .as_ref()
                    .map(|expr| rewrite_aggregate_projection(lowerer, expr, rewrite, None))
                    .transpose()?
                    .map(Box::new),
            })
        }
        Expr::List(items) => {
            let args = items
                .iter()
                .map(|item| rewrite_aggregate_projection(lowerer, item, rewrite, None))
                .collect::<CypherPlanResult<Vec<_>>>()?;
            Ok(IrExpr::List(args))
        }
        Expr::Map(items) => {
            let mut args = Vec::new();
            for (key, value) in items {
                args.push(IrExpr::Lit(Lit::String(key.clone())));
                args.push(rewrite_aggregate_projection(lowerer, value, rewrite, None)?);
            }
            Ok(IrExpr::Call {
                name: "map".to_string(),
                args,
            })
        }
        _ => Err(CypherPlanError::Unsupported(
            "aggregate expression contains an unsupported Cypher expression".to_string(),
        )),
    }
}

fn ensure_group_key(
    lowerer: &mut Lowerer,
    rewrite: &mut AggregateRewrite,
    expr: &Expr,
    preferred_alias: Option<&str>,
) -> CypherPlanResult<String> {
    let ir = lower_expr(lowerer, expr)?;
    if let Some(existing) = rewrite.group.iter().find(|item| item.expr == ir) {
        return Ok(existing.alias.clone());
    }
    let alias = preferred_alias
        .map(ToString::to_string)
        .or_else(|| expr.variable_name().map(ToString::to_string))
        .unwrap_or_else(|| lowerer.synthetic("group"));
    rewrite.group.push(ProjectionItem {
        alias: alias.clone(),
        expr: ir,
    });
    Ok(alias)
}

fn aggregate_kind(name: &str) -> Option<AggKind> {
    match name.to_ascii_lowercase().as_str() {
        "count" => Some(AggKind::CountRows),
        "count_if" => Some(AggKind::CountIf),
        "sum" => Some(AggKind::SumOrZero),
        "avg" => Some(AggKind::AvgOrNull),
        "min" => Some(AggKind::MinOrNull),
        "max" => Some(AggKind::MaxOrNull),
        "stdev" => Some(AggKind::StDev),
        "stdevp" => Some(AggKind::StDevP),
        "percentilecont" => Some(AggKind::PercentileCont),
        "percentiledisc" => Some(AggKind::PercentileDisc),
        "collect" => Some(AggKind::CollectRows),
        _ => None,
    }
}

fn lower_binary_op(op: BinaryOp) -> IrBinaryOp {
    match op {
        BinaryOp::Or => IrBinaryOp::Or,
        BinaryOp::And => IrBinaryOp::And,
        BinaryOp::Eq => IrBinaryOp::Eq,
        BinaryOp::Neq => IrBinaryOp::Neq,
        BinaryOp::Lt => IrBinaryOp::Lt,
        BinaryOp::Lte => IrBinaryOp::Lte,
        BinaryOp::Gt => IrBinaryOp::Gt,
        BinaryOp::Gte => IrBinaryOp::Gte,
        BinaryOp::Add => IrBinaryOp::Add,
        BinaryOp::Sub => IrBinaryOp::Sub,
        BinaryOp::Mul => IrBinaryOp::Mul,
        BinaryOp::Div => IrBinaryOp::Div,
    }
}

fn lower_cypher_binary_expr(op: BinaryOp, lhs: IrExpr, rhs: IrExpr) -> IrExpr {
    let function_name = match op {
        BinaryOp::Eq => Some("cypher_eq"),
        BinaryOp::Neq => Some("cypher_neq"),
        BinaryOp::Lt => Some("cypher_lt"),
        BinaryOp::Lte => Some("cypher_lte"),
        BinaryOp::Gt => Some("cypher_gt"),
        BinaryOp::Gte => Some("cypher_gte"),
        _ => None,
    };
    if let Some(name) = function_name {
        return IrExpr::Call {
            name: name.to_string(),
            args: vec![lhs, rhs],
        };
    }
    IrExpr::Binary {
        op: lower_binary_op(op),
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn lower_string_op(op: crate::language::cypher::ast::StringPredicateOp) -> StringOp {
    match op {
        crate::language::cypher::ast::StringPredicateOp::StartsWith => StringOp::StartsWith,
        crate::language::cypher::ast::StringPredicateOp::EndsWith => StringOp::EndsWith,
        crate::language::cypher::ast::StringPredicateOp::Contains => StringOp::Contains,
        crate::language::cypher::ast::StringPredicateOp::Regex => {
            unreachable!("regex string predicates lower through regex_match")
        }
    }
}

fn lower_string_predicate_expr(
    op: crate::language::cypher::ast::StringPredicateOp,
    target: IrExpr,
    pattern: IrExpr,
) -> IrExpr {
    match op {
        crate::language::cypher::ast::StringPredicateOp::Regex => IrExpr::Call {
            name: "regex_match".to_string(),
            args: vec![target, pattern],
        },
        _ => IrExpr::StringPredicate {
            op: lower_string_op(op),
            target: Box::new(target),
            pattern: Box::new(pattern),
        },
    }
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

    if contains_aggregate(expr) {
        return Ok(None);
    }
    if requires_scoped_materialization(expr) {
        return Ok(None);
    }

    let mut refs = BTreeSet::new();
    collect_free_variables(expr, &mut BTreeSet::new(), &mut refs);
    if refs
        .iter()
        .all(|name| fields.iter().any(|field| field == name))
    {
        return Ok(Some(lower_expr(lowerer, expr)?));
    }
    Ok(None)
}

fn free_variable_names(expr: &Expr) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_free_variables(expr, &mut BTreeSet::new(), &mut refs);
    refs
}

fn free_variable_names_for_sort(
    expr: &Expr,
    source_fields: &[String],
    projected_fields: &[String],
) -> BTreeSet<String> {
    let mut refs = free_variable_names(expr);
    let candidates = source_fields
        .iter()
        .chain(projected_fields.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    remove_local_exists_bindings(expr, &candidates, &mut refs);
    add_candidate_pattern_bindings(expr, &candidates, &mut refs);
    refs
}

fn free_variable_names_for_local_scope(expr: &Expr, locals: &BTreeSet<String>) -> BTreeSet<String> {
    let mut refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, locals, &mut refs);
    add_candidate_pattern_bindings(expr, locals, &mut refs);
    refs
}

pub(crate) fn validate_expression_scope(
    lowerer: &Lowerer,
    expr: &Expr,
    clause: &str,
) -> CypherPlanResult<()> {
    let candidates = lowerer
        .visible_fields()
        .into_iter()
        .collect::<BTreeSet<_>>();
    validate_expression_refs(expr, &candidates, clause)
}

pub(crate) fn expression_candidate_refs(
    expr: &Expr,
    candidates: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut refs);
    add_candidate_pattern_bindings(expr, candidates, &mut refs);
    refs.into_iter()
        .filter(|name| candidates.contains(name))
        .collect()
}

pub(crate) fn validate_expression_refs(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    clause: &str,
) -> CypherPlanResult<()> {
    let mut refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut refs);
    add_candidate_pattern_bindings(expr, candidates, &mut refs);
    let missing = refs
        .into_iter()
        .filter(|name| !candidates.contains(name))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "{clause} references variables that are not in scope: {}",
            missing.join(", ")
        )))
    }
}

fn add_candidate_pattern_bindings(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                add_candidate_bindings_from_query(query, candidates, refs);
            }
            for part in &exists.patterns {
                add_candidate_bindings_from_pattern(part, candidates, refs);
            }
            if let Some(predicate) = &exists.predicate {
                add_candidate_pattern_bindings(predicate, candidates, refs);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                add_candidate_bindings_from_pattern(part, candidates, refs);
            }
        }
        Expr::PatternComprehension {
            pattern,
            predicate,
            map,
            ..
        } => {
            add_candidate_bindings_from_pattern(pattern, candidates, refs);
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings(predicate, candidates, refs);
            }
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            add_candidate_pattern_bindings(expr, candidates, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            add_candidate_pattern_bindings(lhs, candidates, refs);
            add_candidate_pattern_bindings(rhs, candidates, refs);
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            add_candidate_pattern_bindings(target, candidates, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                add_candidate_pattern_bindings(arg, candidates, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                add_candidate_pattern_bindings(value, candidates, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                add_candidate_pattern_bindings(case, candidates, refs);
            }
            for (when, then) in arms {
                add_candidate_pattern_bindings(when, candidates, refs);
                add_candidate_pattern_bindings(then, candidates, refs);
            }
            if let Some(otherwise) = otherwise {
                add_candidate_pattern_bindings(otherwise, candidates, refs);
            }
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings(predicate, candidates, refs);
            }
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::ListReduce {
            collection, map, ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::ListTransform {
            collection, map, ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(map, candidates, refs);
        }
        Expr::ListFilter {
            collection,
            predicate,
            ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(predicate, candidates, refs);
        }
        Expr::Quantifier {
            collection,
            predicate,
            ..
        } => {
            add_candidate_pattern_bindings(collection, candidates, refs);
            add_candidate_pattern_bindings(predicate, candidates, refs);
        }
        Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => {}
    }
}

fn add_candidate_bindings_from_pattern(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for name in pattern_binding_names(pattern) {
        if candidates.contains(&name) {
            refs.insert(name);
        }
    }
    add_candidate_refs_from_pattern_properties(pattern, candidates, refs);
}

fn add_candidate_refs_from_expr(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    let mut expr_refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut expr_refs);
    add_candidate_pattern_bindings(expr, candidates, &mut expr_refs);
    refs.extend(
        expr_refs
            .into_iter()
            .filter(|name| candidates.contains(name)),
    );
}

fn add_candidate_refs_from_pattern_properties(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        add_candidate_refs_from_expr(properties, candidates, refs);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            add_candidate_refs_from_expr(properties, candidates, refs);
        }
        if let Some(properties) = &chain.node.properties {
            add_candidate_refs_from_expr(properties, candidates, refs);
        }
    }
}

fn add_candidate_bindings_from_query(
    query: &Query,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    add_candidate_bindings_from_query_scoped(query, candidates, &BTreeSet::new(), refs);
}

fn add_candidate_bindings_from_query_scoped(
    query: &Query,
    candidates: &BTreeSet<String>,
    outer_locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    let mut locals = outer_locals.clone();
    add_candidate_bindings_from_query_body(query, candidates, &mut locals, refs);
    for branch in &query.unions {
        let mut branch_locals = outer_locals.clone();
        add_candidate_bindings_from_query_body(&branch.query, candidates, &mut branch_locals, refs);
    }
}

fn add_candidate_bindings_from_query_body(
    query: &Query,
    candidates: &BTreeSet<String>,
    locals: &mut BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    let names = pattern_binding_names(part);
                    add_candidate_bindings_from_pattern_scoped(part, candidates, locals, refs);
                    for name in names {
                        if !candidates.contains(&name) || locals.contains(&name) {
                            locals.insert(name);
                        }
                    }
                }
                if let Some(predicate) = &clause.predicate {
                    add_candidate_refs_from_expr_scoped(predicate, candidates, locals, refs);
                }
            }
            Clause::Unwind(clause) => {
                add_candidate_refs_from_expr_scoped(&clause.expr, candidates, locals, refs);
                locals.insert(clause.alias.clone());
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    add_candidate_refs_from_expr_scoped(arg, candidates, locals, refs);
                }
                for item in &clause.yields {
                    locals.insert(item.alias.clone());
                }
                if clause.yield_all || clause.standalone {
                    locals.extend(default_query_procedure_yields(&clause.name));
                }
                if let Some(predicate) = &clause.predicate {
                    add_candidate_refs_from_expr_scoped(predicate, candidates, locals, refs);
                }
            }
            Clause::With(clause) => {
                let outputs = add_candidate_bindings_from_projection_scoped(
                    &clause.projection,
                    candidates,
                    locals,
                    refs,
                );
                *locals = outputs;
                if let Some(predicate) = &clause.predicate {
                    add_candidate_refs_from_expr_scoped(predicate, candidates, locals, refs);
                }
            }
            Clause::Return(clause) => {
                add_candidate_bindings_from_projection_scoped(
                    &clause.projection,
                    candidates,
                    locals,
                    refs,
                );
            }
        }
    }
}

fn add_candidate_bindings_from_projection_scoped(
    body: &ProjectionBody,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) -> BTreeSet<String> {
    for item in &body.items {
        add_candidate_refs_from_expr_scoped(&item.expr, candidates, locals, refs);
    }
    let outputs = projection_output_names(body, locals);
    let mut order_locals = locals.clone();
    order_locals.extend(outputs.iter().cloned());
    for item in &body.order_by {
        add_candidate_refs_from_expr_scoped(&item.expr, candidates, &order_locals, refs);
    }
    if let Some(skip) = &body.skip {
        add_candidate_refs_from_expr_scoped(skip, candidates, &order_locals, refs);
    }
    if let Some(limit) = &body.limit {
        add_candidate_refs_from_expr_scoped(limit, candidates, &order_locals, refs);
    }
    outputs
}

fn add_candidate_bindings_from_pattern_scoped(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for name in pattern_binding_names(pattern) {
        if candidates.contains(&name) && !locals.contains(&name) {
            refs.insert(name);
        }
    }
    add_candidate_refs_from_pattern_properties_scoped(pattern, candidates, locals, refs);
}

fn add_candidate_refs_from_expr_scoped(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    let mut expr_refs = free_variable_names(expr);
    remove_local_exists_bindings(expr, candidates, &mut expr_refs);
    refs.extend(
        expr_refs
            .into_iter()
            .filter(|name| candidates.contains(name) && !locals.contains(name)),
    );
    add_candidate_pattern_bindings_scoped(expr, candidates, locals, refs);
}

fn add_candidate_pattern_bindings_scoped(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                add_candidate_bindings_from_query_scoped(query, candidates, locals, refs);
            }
            for part in &exists.patterns {
                add_candidate_bindings_from_pattern_scoped(part, candidates, locals, refs);
            }
            if let Some(predicate) = &exists.predicate {
                add_candidate_pattern_bindings_scoped(predicate, candidates, locals, refs);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                add_candidate_bindings_from_pattern_scoped(part, candidates, locals, refs);
            }
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let mut pattern_locals = locals.clone();
            if let Some(variable) = variable {
                pattern_locals.insert(variable.clone());
            }
            let names = pattern_binding_names(pattern);
            add_candidate_bindings_from_pattern_scoped(pattern, candidates, &pattern_locals, refs);
            pattern_locals.extend(names);
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings_scoped(predicate, candidates, &pattern_locals, refs);
            }
            add_candidate_pattern_bindings_scoped(map, candidates, &pattern_locals, refs);
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            if let Some(predicate) = predicate {
                add_candidate_pattern_bindings_scoped(predicate, candidates, &item_locals, refs);
            }
            add_candidate_pattern_bindings_scoped(map, candidates, &item_locals, refs);
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(accumulator.clone());
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(map, candidates, &item_locals, refs);
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(map, candidates, &item_locals, refs);
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(predicate, candidates, &item_locals, refs);
        }
        Expr::Quantifier {
            variable,
            collection,
            predicate,
            ..
        } => {
            add_candidate_pattern_bindings_scoped(collection, candidates, locals, refs);
            let mut item_locals = locals.clone();
            item_locals.insert(variable.clone());
            add_candidate_pattern_bindings_scoped(predicate, candidates, &item_locals, refs);
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            add_candidate_pattern_bindings_scoped(expr, candidates, locals, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            add_candidate_pattern_bindings_scoped(lhs, candidates, locals, refs);
            add_candidate_pattern_bindings_scoped(rhs, candidates, locals, refs);
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            add_candidate_pattern_bindings_scoped(target, candidates, locals, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                add_candidate_pattern_bindings_scoped(arg, candidates, locals, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                add_candidate_pattern_bindings_scoped(value, candidates, locals, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                add_candidate_pattern_bindings_scoped(case, candidates, locals, refs);
            }
            for (when, then) in arms {
                add_candidate_pattern_bindings_scoped(when, candidates, locals, refs);
                add_candidate_pattern_bindings_scoped(then, candidates, locals, refs);
            }
            if let Some(otherwise) = otherwise {
                add_candidate_pattern_bindings_scoped(otherwise, candidates, locals, refs);
            }
        }
        Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => {}
    }
}

fn add_candidate_refs_from_pattern_properties_scoped(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    locals: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        add_candidate_refs_from_expr_scoped(properties, candidates, locals, refs);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            add_candidate_refs_from_expr_scoped(properties, candidates, locals, refs);
        }
        if let Some(properties) = &chain.node.properties {
            add_candidate_refs_from_expr_scoped(properties, candidates, locals, refs);
        }
    }
}

fn remove_local_exists_bindings(
    expr: &Expr,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                remove_local_query_bindings(query, candidates, refs);
            }
            for part in &exists.patterns {
                for name in pattern_binding_names(part) {
                    if !candidates.contains(&name) {
                        refs.remove(&name);
                    }
                }
            }
            if let Some(predicate) = &exists.predicate {
                remove_local_exists_bindings(predicate, candidates, refs);
            }
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            remove_local_exists_bindings(expr, candidates, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            remove_local_exists_bindings(lhs, candidates, refs);
            remove_local_exists_bindings(rhs, candidates, refs);
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            remove_local_exists_bindings(target, candidates, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                remove_local_exists_bindings(arg, candidates, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                remove_local_exists_bindings(value, candidates, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                remove_local_exists_bindings(case, candidates, refs);
            }
            for (when, then) in arms {
                remove_local_exists_bindings(when, candidates, refs);
                remove_local_exists_bindings(then, candidates, refs);
            }
            if let Some(otherwise) = otherwise {
                remove_local_exists_bindings(otherwise, candidates, refs);
            }
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            if let Some(predicate) = predicate {
                remove_local_exists_bindings(predicate, candidates, refs);
            }
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::ListReduce {
            collection, map, ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::ListTransform {
            collection, map, ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::ListFilter {
            collection,
            predicate,
            ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(predicate, candidates, refs);
        }
        Expr::PatternComprehension { predicate, map, .. } => {
            if let Some(predicate) = predicate {
                remove_local_exists_bindings(predicate, candidates, refs);
            }
            remove_local_exists_bindings(map, candidates, refs);
        }
        Expr::Quantifier {
            collection,
            predicate,
            ..
        } => {
            remove_local_exists_bindings(collection, candidates, refs);
            remove_local_exists_bindings(predicate, candidates, refs);
        }
        Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => {}
    }
}

fn remove_local_query_bindings(
    query: &Query,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    remove_local_query_body_bindings(query, candidates, refs);
    for branch in &query.unions {
        remove_local_query_body_bindings(&branch.query, candidates, refs);
    }
}

fn remove_local_query_body_bindings(
    query: &Query,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    remove_local_pattern_bindings(part, candidates, refs);
                    collect_local_pattern_expression_bindings(part, candidates, refs);
                }
                if let Some(predicate) = &clause.predicate {
                    remove_local_exists_bindings(predicate, candidates, refs);
                }
            }
            Clause::Unwind(clause) => {
                remove_local_exists_bindings(&clause.expr, candidates, refs);
                remove_query_local_name(&clause.alias, candidates, refs);
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    remove_local_exists_bindings(arg, candidates, refs);
                }
                for item in &clause.yields {
                    remove_query_local_name(&item.alias, candidates, refs);
                }
                if clause.yield_all || clause.standalone {
                    for name in default_query_procedure_yields(&clause.name) {
                        remove_query_local_name(&name, candidates, refs);
                    }
                }
                if let Some(predicate) = &clause.predicate {
                    remove_local_exists_bindings(predicate, candidates, refs);
                }
            }
            Clause::With(clause) => {
                remove_local_projection_bindings(&clause.projection, candidates, refs);
                if let Some(predicate) = &clause.predicate {
                    remove_local_exists_bindings(predicate, candidates, refs);
                }
            }
            Clause::Return(clause) => {
                remove_local_projection_bindings(&clause.projection, candidates, refs);
            }
        }
    }
}

fn remove_local_projection_bindings(
    body: &ProjectionBody,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for item in &body.items {
        remove_local_exists_bindings(&item.expr, candidates, refs);
        if let Some(alias) = item.alias.as_deref().or_else(|| item.expr.variable_name()) {
            remove_query_local_name(alias, candidates, refs);
        }
    }
    for item in &body.order_by {
        remove_local_exists_bindings(&item.expr, candidates, refs);
    }
    if let Some(skip) = &body.skip {
        remove_local_exists_bindings(skip, candidates, refs);
    }
    if let Some(limit) = &body.limit {
        remove_local_exists_bindings(limit, candidates, refs);
    }
}

fn collect_local_pattern_expression_bindings(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        remove_local_exists_bindings(properties, candidates, refs);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            remove_local_exists_bindings(properties, candidates, refs);
        }
        if let Some(properties) = &chain.node.properties {
            remove_local_exists_bindings(properties, candidates, refs);
        }
    }
}

fn remove_local_pattern_bindings(
    pattern: &PatternPart,
    candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    for name in pattern_binding_names(pattern) {
        remove_query_local_name(&name, candidates, refs);
    }
}

fn remove_query_local_name(
    name: &str,
    _candidates: &BTreeSet<String>,
    refs: &mut BTreeSet<String>,
) {
    refs.remove(name);
}

fn shadowed_projection_fields(body: &ProjectionBody, source_fields: &[String]) -> BTreeSet<String> {
    body.items
        .iter()
        .filter_map(|item| {
            let alias = item
                .alias
                .as_deref()
                .or_else(|| item.expr.variable_name())?;
            if source_fields.iter().any(|field| field == alias)
                && item.expr != Expr::Variable(alias.to_string())
            {
                Some(alias.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn projection_aliases(body: &ProjectionBody) -> BTreeMap<String, Expr> {
    body.items
        .iter()
        .filter_map(|item| {
            let alias = item
                .alias
                .clone()
                .or_else(|| item.expr.variable_name().map(ToString::to_string))?;
            Some((alias, item.expr.clone()))
        })
        .collect()
}

fn substitute_projection_aliases(expr: &Expr, aliases: &BTreeMap<String, Expr>) -> Expr {
    substitute_projection_aliases_with_bound(expr, aliases, &mut BTreeSet::new())
}

fn substitute_projection_aliases_with_bound(
    expr: &Expr,
    aliases: &BTreeMap<String, Expr>,
    bound: &mut BTreeSet<String>,
) -> Expr {
    match expr {
        Expr::Variable(name) if !bound.contains(name) => {
            aliases.get(name).cloned().unwrap_or_else(|| expr.clone())
        }
        Expr::Property { target, key } => Expr::Property {
            target: Box::new(substitute_projection_aliases_with_bound(
                target, aliases, bound,
            )),
            key: key.clone(),
        },
        Expr::LabelPredicate { target, labels } => Expr::LabelPredicate {
            target: Box::new(substitute_projection_aliases_with_bound(
                target, aliases, bound,
            )),
            labels: labels.clone(),
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op: *op,
            expr: Box::new(substitute_projection_aliases_with_bound(
                expr, aliases, bound,
            )),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: *op,
            lhs: Box::new(substitute_projection_aliases_with_bound(
                lhs, aliases, bound,
            )),
            rhs: Box::new(substitute_projection_aliases_with_bound(
                rhs, aliases, bound,
            )),
        },
        Expr::IsNull(expr) => Expr::IsNull(Box::new(substitute_projection_aliases_with_bound(
            expr, aliases, bound,
        ))),
        Expr::IsNotNull(expr) => Expr::IsNotNull(Box::new(
            substitute_projection_aliases_with_bound(expr, aliases, bound),
        )),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => Expr::StringPredicate {
            op: *op,
            target: Box::new(substitute_projection_aliases_with_bound(
                target, aliases, bound,
            )),
            pattern: Box::new(substitute_projection_aliases_with_bound(
                pattern, aliases, bound,
            )),
        },
        Expr::Function {
            name,
            distinct,
            args,
        } => Expr::Function {
            name: name.clone(),
            distinct: *distinct,
            args: args
                .iter()
                .map(|arg| substitute_projection_aliases_with_bound(arg, aliases, bound))
                .collect(),
        },
        Expr::Case {
            case,
            arms,
            otherwise,
        } => Expr::Case {
            case: case
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new),
            arms: arms
                .iter()
                .map(|(when, then)| {
                    (
                        substitute_projection_aliases_with_bound(when, aliases, bound),
                        substitute_projection_aliases_with_bound(then, aliases, bound),
                    )
                })
                .collect(),
            otherwise: otherwise
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new),
        },
        Expr::List(items) => Expr::List(
            items
                .iter()
                .map(|item| substitute_projection_aliases_with_bound(item, aliases, bound))
                .collect(),
        ),
        Expr::Map(items) => Expr::Map(
            items
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        substitute_projection_aliases_with_bound(value, aliases, bound),
                    )
                })
                .collect(),
        ),
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = predicate
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new);
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::ListComprehension {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate,
                map: Box::new(map),
            }
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let acc_was_bound = bound.contains(accumulator);
            let variable_was_bound = bound.contains(variable);
            bound.insert(accumulator.clone());
            bound.insert(variable.clone());
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            if !acc_was_bound {
                bound.remove(accumulator);
            }
            if !variable_was_bound {
                bound.remove(variable);
            }
            Expr::ListReduce {
                accumulator: accumulator.clone(),
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            }
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::ListTransform {
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            }
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = substitute_projection_aliases_with_bound(predicate, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::ListFilter {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            }
        }
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = substitute_projection_aliases_with_bound(predicate, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::Quantifier {
                kind: *kind,
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            }
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let mut pattern = (**pattern).clone();
            let local_names = pattern_binding_names(&pattern);
            let previously_bound = local_names
                .iter()
                .filter(|name| bound.contains(*name))
                .cloned()
                .collect::<BTreeSet<_>>();
            for name in &local_names {
                bound.insert(name.clone());
            }
            substitute_pattern_property_aliases(&mut pattern, aliases, bound);
            let variable_was_bound = variable.as_ref().is_some_and(|name| bound.contains(name));
            if let Some(variable) = variable {
                bound.insert(variable.clone());
            }
            let predicate = predicate
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new);
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            for name in &local_names {
                if !previously_bound.contains(name) {
                    bound.remove(name);
                }
            }
            if let Some(variable) = variable {
                if !variable_was_bound {
                    bound.remove(variable);
                }
            }
            Expr::PatternComprehension {
                variable: variable.clone(),
                pattern: Box::new(pattern),
                predicate,
                map: Box::new(map),
            }
        }
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => expr.clone(),
    }
}

fn substitute_pattern_property_aliases(
    pattern: &mut PatternPart,
    aliases: &BTreeMap<String, Expr>,
    bound: &mut BTreeSet<String>,
) {
    if let Some(properties) = pattern.element.start.properties.clone() {
        pattern.element.start.properties = Some(substitute_projection_aliases_with_bound(
            &properties,
            aliases,
            bound,
        ));
    }
    for chain in &mut pattern.element.chains {
        if let Some(properties) = chain.relationship.properties.clone() {
            chain.relationship.properties = Some(substitute_projection_aliases_with_bound(
                &properties,
                aliases,
                bound,
            ));
        }
        if let Some(properties) = chain.node.properties.clone() {
            chain.node.properties = Some(substitute_projection_aliases_with_bound(
                &properties,
                aliases,
                bound,
            ));
        }
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
        )))
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

fn lower_label_predicate_expr(target: IrExpr, labels: &[String]) -> IrExpr {
    let parts = labels
        .iter()
        .map(|label| match &target {
            IrExpr::Binding(binding) => IrExpr::HasLabel {
                binding: binding.clone(),
                label: label.clone(),
            },
            _ => IrExpr::Call {
                name: "in".to_string(),
                args: vec![
                    IrExpr::Lit(Lit::String(label.clone())),
                    IrExpr::Call {
                        name: "labels".to_string(),
                        args: vec![target.clone()],
                    },
                ],
            },
        })
        .collect();
    IrExpr::and(parts)
}

pub(crate) fn collect_free_variables(
    expr: &Expr,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Variable(name) => {
            if !bound.contains(name) {
                out.insert(name.clone());
            }
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            collect_free_variables(target, bound, out);
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            collect_free_variables(expr, bound, out);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            collect_free_variables(lhs, bound, out);
            collect_free_variables(rhs, bound, out);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                collect_free_variables(arg, bound, out);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                collect_free_variables(value, bound, out);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                collect_free_variables(case, bound, out);
            }
            for (when, then) in arms {
                collect_free_variables(when, bound, out);
                collect_free_variables(then, bound, out);
            }
            if let Some(otherwise) = otherwise {
                collect_free_variables(otherwise, bound, out);
            }
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            if let Some(predicate) = predicate {
                collect_free_variables(predicate, bound, out);
            }
            collect_free_variables(map, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            let acc_was_bound = bound.contains(accumulator);
            let variable_was_bound = bound.contains(variable);
            bound.insert(accumulator.clone());
            bound.insert(variable.clone());
            collect_free_variables(map, bound, out);
            if !acc_was_bound {
                bound.remove(accumulator);
            }
            if !variable_was_bound {
                bound.remove(variable);
            }
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            collect_free_variables(map, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            collect_free_variables(collection, bound, out);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            collect_free_variables(predicate, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::Quantifier {
            variable,
            collection,
            predicate,
            ..
        } => {
            collect_free_variables(collection, bound, out);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            collect_free_variables(predicate, bound, out);
            if !was_bound {
                bound.remove(variable);
            }
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let variable_was_bound = variable.as_ref().is_some_and(|name| bound.contains(name));
            if let Some(variable) = variable {
                bound.insert(variable.clone());
            }
            let pattern_bound = pattern_binding_names(pattern);
            let previously_bound = pattern_bound
                .iter()
                .filter(|name| bound.contains(*name))
                .cloned()
                .collect::<BTreeSet<_>>();
            for name in &pattern_bound {
                bound.insert(name.clone());
            }
            collect_pattern_property_variables(pattern, bound, out);
            if let Some(predicate) = predicate {
                collect_free_variables(predicate, bound, out);
            }
            collect_free_variables(map, bound, out);
            for name in &pattern_bound {
                if !previously_bound.contains(name) {
                    bound.remove(name);
                }
            }
            if let Some(variable) = variable {
                if !variable_was_bound {
                    bound.remove(variable);
                }
            }
        }
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                collect_query_variable_references(query, bound, out);
            }
            for part in &exists.patterns {
                collect_pattern_variable_references(part, out);
                collect_pattern_property_variables(part, bound, out);
            }
            if let Some(predicate) = &exists.predicate {
                collect_free_variables(predicate, bound, out);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                collect_pattern_variable_references(part, out);
                collect_pattern_property_variables(part, bound, out);
            }
        }
        Expr::Star | Expr::Parameter(_) | Expr::Literal(_) | Expr::CountStar => {}
    }
}

fn pattern_binding_names(pattern: &PatternPart) -> BTreeSet<String> {
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

fn collect_pattern_variable_references(pattern: &PatternPart, out: &mut BTreeSet<String>) {
    out.extend(pattern_binding_names(pattern));
}

fn collect_pattern_property_variables(
    pattern: &PatternPart,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        collect_free_variables(properties, bound, out);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            collect_free_variables(properties, bound, out);
        }
        if let Some(properties) = &chain.node.properties {
            collect_free_variables(properties, bound, out);
        }
    }
}

fn collect_query_variable_references(
    query: &Query,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    let mut query_bound = bound.clone();
    collect_query_body_variable_references(query, &mut query_bound, out);
    for branch in &query.unions {
        let mut branch_bound = bound.clone();
        collect_query_body_variable_references(&branch.query, &mut branch_bound, out);
    }
}

fn collect_query_body_variable_references(
    query: &Query,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    collect_pattern_variable_references(part, out);
                    collect_pattern_property_variables(part, bound, out);
                    bound.extend(pattern_binding_names(part));
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, bound, out);
                }
            }
            Clause::Unwind(clause) => {
                collect_free_variables(&clause.expr, bound, out);
                bound.insert(clause.alias.clone());
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    collect_free_variables(arg, bound, out);
                }
                for item in &clause.yields {
                    bound.insert(item.alias.clone());
                }
                if clause.yield_all || clause.standalone {
                    for item in default_query_procedure_yields(&clause.name) {
                        bound.insert(item);
                    }
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, bound, out);
                }
            }
            Clause::With(clause) => {
                collect_projection_variable_references(&clause.projection, bound, out);
                let outputs = projection_output_names(&clause.projection, bound);
                bound.clear();
                bound.extend(outputs);
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, bound, out);
                }
            }
            Clause::Return(clause) => {
                collect_projection_variable_references(&clause.projection, bound, out);
            }
        }
    }
}

fn collect_projection_variable_references(
    body: &ProjectionBody,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    for item in &body.items {
        collect_free_variables(&item.expr, bound, out);
    }
    let mut projection_bound = bound.clone();
    projection_bound.extend(projection_output_names(body, bound));
    for item in &body.order_by {
        collect_free_variables(&item.expr, &mut projection_bound, out);
    }
    if let Some(skip) = &body.skip {
        collect_free_variables(skip, &mut projection_bound, out);
    }
    if let Some(limit) = &body.limit {
        collect_free_variables(limit, &mut projection_bound, out);
    }
}

fn projection_output_names(body: &ProjectionBody, visible: &BTreeSet<String>) -> BTreeSet<String> {
    let mut outputs = if body.include_existing {
        visible.clone()
    } else {
        BTreeSet::new()
    };
    for item in &body.items {
        if let Some(alias) = item
            .alias
            .clone()
            .or_else(|| item.expr.variable_name().map(ToString::to_string))
        {
            outputs.insert(alias);
        }
    }
    outputs
}

fn default_query_procedure_yields(name: &str) -> Vec<String> {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" => vec!["label".to_string()],
        "db.relationshiptypes" => vec!["relationshipType".to_string()],
        "db.propertykeys" => vec!["propertyKey".to_string()],
        _ => vec!["value".to_string()],
    }
}

fn literal_u64(expr: &Expr) -> CypherPlanResult<Option<u64>> {
    match expr {
        Expr::Literal(Literal::Integer(value)) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| CypherPlanError::Invalid("slice bound is outside u64 range".to_string())),
        Expr::Literal(Literal::Float(_)) => Err(CypherPlanError::Invalid(
            "slice bounds must be non-negative integers".to_string(),
        )),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => {
            if literal_u64(expr)?.is_some() {
                Err(CypherPlanError::Invalid(
                    "slice bounds must be non-negative integers".to_string(),
                ))
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

pub(crate) fn lower_expr_with_input(
    lowerer: &mut Lowerer,
    input: Node,
    expr: &Expr,
) -> CypherPlanResult<(Node, IrExpr)> {
    let (input, expr) = materialize_expr(lowerer, input, expr)?;
    Ok((input, lower_expr(lowerer, &expr)?))
}

fn materialize_expr(
    lowerer: &mut Lowerer,
    input: Node,
    expr: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    match expr {
        Expr::Exists(exists) => materialize_exists(lowerer, input, exists),
        Expr::PatternPredicate(patterns) => materialize_pattern_predicate(lowerer, input, patterns),
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => materialize_list_comprehension(
            lowerer,
            input,
            variable,
            collection,
            predicate.as_deref(),
            map,
        ),
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => materialize_pattern_comprehension(
            lowerer,
            input,
            variable.as_deref(),
            pattern,
            predicate.as_deref(),
            map,
        ),
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => materialize_quantifier(lowerer, input, *kind, variable, collection, predicate),
        Expr::Function {
            name,
            distinct: false,
            args,
        } if name.eq_ignore_ascii_case("size")
            && matches!(args.as_slice(), [Expr::PatternPredicate(_)]) =>
        {
            let [Expr::PatternPredicate(patterns)] = args.as_slice() else {
                unreachable!("size(pattern) guard matched");
            };
            materialize_pattern_count(lowerer, input, patterns)
        }
        Expr::Function {
            name,
            distinct: false,
            args,
        } if name.eq_ignore_ascii_case("exists")
            && matches!(args.as_slice(), [Expr::PatternPredicate(_)]) =>
        {
            let [Expr::PatternPredicate(patterns)] = args.as_slice() else {
                unreachable!("exists(pattern) guard matched");
            };
            materialize_pattern_predicate(lowerer, input, patterns)
        }
        Expr::Unary { op, expr } => {
            let (input, expr) = materialize_expr(lowerer, input, expr)?;
            Ok((
                input,
                Expr::Unary {
                    op: *op,
                    expr: Box::new(expr),
                },
            ))
        }
        Expr::Binary { op, lhs, rhs } => {
            let (input, lhs) = materialize_expr(lowerer, input, lhs)?;
            let (input, rhs) = materialize_expr(lowerer, input, rhs)?;
            Ok((
                input,
                Expr::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            ))
        }
        Expr::Property { target, key } => {
            let (input, target) = materialize_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::Property {
                    target: Box::new(target),
                    key: key.clone(),
                },
            ))
        }
        Expr::LabelPredicate { target, labels } => {
            let (input, target) = materialize_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::LabelPredicate {
                    target: Box::new(target),
                    labels: labels.clone(),
                },
            ))
        }
        Expr::IsNull(expr) => {
            let (input, expr) = materialize_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNull(Box::new(expr))))
        }
        Expr::IsNotNull(expr) => {
            let (input, expr) = materialize_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNotNull(Box::new(expr))))
        }
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => {
            let (input, target) = materialize_expr(lowerer, input, target)?;
            let (input, pattern) = materialize_expr(lowerer, input, pattern)?;
            Ok((
                input,
                Expr::StringPredicate {
                    op: *op,
                    target: Box::new(target),
                    pattern: Box::new(pattern),
                },
            ))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(args.len());
            for arg in args {
                let (next, arg) = materialize_expr(lowerer, input, arg)?;
                input = next;
                lowered.push(arg);
            }
            Ok((
                input,
                Expr::Function {
                    name: name.clone(),
                    distinct: *distinct,
                    args: lowered,
                },
            ))
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListReduce {
                    accumulator: accumulator.clone(),
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListTransform {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let (input, collection) = materialize_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListFilter {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate: predicate.clone(),
                },
            ))
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            let (mut input, case) = match case {
                Some(case) => {
                    let (next, case) = materialize_expr(lowerer, input, case)?;
                    (next, Some(Box::new(case)))
                }
                None => (input, None),
            };
            let mut lowered_arms = Vec::with_capacity(arms.len());
            for (when, then) in arms {
                let (next, when) = materialize_expr(lowerer, input, when)?;
                let (next, then) = materialize_expr(lowerer, next, then)?;
                input = next;
                lowered_arms.push((when, then));
            }
            let otherwise = match otherwise {
                Some(expr) => {
                    let (next, expr) = materialize_expr(lowerer, input, expr)?;
                    input = next;
                    Some(Box::new(expr))
                }
                None => None,
            };
            Ok((
                input,
                Expr::Case {
                    case,
                    arms: lowered_arms,
                    otherwise,
                },
            ))
        }
        Expr::List(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for item in items {
                let (next, item) = materialize_expr(lowerer, input, item)?;
                input = next;
                lowered.push(item);
            }
            Ok((input, Expr::List(lowered)))
        }
        Expr::Map(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for (key, value) in items {
                let (next, value) = materialize_expr(lowerer, input, value)?;
                input = next;
                lowered.push((key.clone(), value));
            }
            Ok((input, Expr::Map(lowered)))
        }
        _ => Ok((input, expr.clone())),
    }
}

fn materialize_pre_aggregate_expr(
    lowerer: &mut Lowerer,
    input: Node,
    expr: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    if let Expr::Function {
        name,
        distinct,
        args,
    } = expr
    {
        if aggregate_kind(name).is_some() {
            let mut input = input;
            let mut lowered = Vec::with_capacity(args.len());
            for arg in args {
                let (next, arg) = materialize_pre_aggregate_expr(lowerer, input, arg)?;
                input = next;
                lowered.push(arg);
            }
            return Ok((
                input,
                Expr::Function {
                    name: name.clone(),
                    distinct: *distinct,
                    args: lowered,
                },
            ));
        }
    }
    if requires_scoped_materialization(expr) {
        if !contains_aggregate(expr) {
            return materialize_expr(lowerer, input, expr);
        }
        return Ok((input, expr.clone()));
    }
    match expr {
        Expr::Unary { op, expr } => {
            let (input, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
            Ok((
                input,
                Expr::Unary {
                    op: *op,
                    expr: Box::new(expr),
                },
            ))
        }
        Expr::Binary { op, lhs, rhs } => {
            let (input, lhs) = materialize_pre_aggregate_expr(lowerer, input, lhs)?;
            let (input, rhs) = materialize_pre_aggregate_expr(lowerer, input, rhs)?;
            Ok((
                input,
                Expr::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            ))
        }
        Expr::Property { target, key } => {
            let (input, target) = materialize_pre_aggregate_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::Property {
                    target: Box::new(target),
                    key: key.clone(),
                },
            ))
        }
        Expr::LabelPredicate { target, labels } => {
            let (input, target) = materialize_pre_aggregate_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::LabelPredicate {
                    target: Box::new(target),
                    labels: labels.clone(),
                },
            ))
        }
        Expr::IsNull(expr) => {
            let (input, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNull(Box::new(expr))))
        }
        Expr::IsNotNull(expr) => {
            let (input, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNotNull(Box::new(expr))))
        }
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => {
            let (input, target) = materialize_pre_aggregate_expr(lowerer, input, target)?;
            let (input, pattern) = materialize_pre_aggregate_expr(lowerer, input, pattern)?;
            Ok((
                input,
                Expr::StringPredicate {
                    op: *op,
                    target: Box::new(target),
                    pattern: Box::new(pattern),
                },
            ))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(args.len());
            for arg in args {
                let (next, arg) = materialize_pre_aggregate_expr(lowerer, input, arg)?;
                input = next;
                lowered.push(arg);
            }
            Ok((
                input,
                Expr::Function {
                    name: name.clone(),
                    distinct: *distinct,
                    args: lowered,
                },
            ))
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListReduce {
                    accumulator: accumulator.clone(),
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListTransform {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListFilter {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate: predicate.clone(),
                },
            ))
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            let (mut input, case) = match case {
                Some(case) => {
                    let (next, case) = materialize_pre_aggregate_expr(lowerer, input, case)?;
                    (next, Some(Box::new(case)))
                }
                None => (input, None),
            };
            let mut lowered_arms = Vec::with_capacity(arms.len());
            for (when, then) in arms {
                let (next, when) = materialize_pre_aggregate_expr(lowerer, input, when)?;
                let (next, then) = materialize_pre_aggregate_expr(lowerer, next, then)?;
                input = next;
                lowered_arms.push((when, then));
            }
            let otherwise = match otherwise {
                Some(expr) => {
                    let (next, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
                    input = next;
                    Some(Box::new(expr))
                }
                None => None,
            };
            Ok((
                input,
                Expr::Case {
                    case,
                    arms: lowered_arms,
                    otherwise,
                },
            ))
        }
        Expr::List(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for item in items {
                let (next, item) = materialize_pre_aggregate_expr(lowerer, input, item)?;
                input = next;
                lowered.push(item);
            }
            Ok((input, Expr::List(lowered)))
        }
        Expr::Map(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for (key, value) in items {
                let (next, value) = materialize_pre_aggregate_expr(lowerer, input, value)?;
                input = next;
                lowered.push((key.clone(), value));
            }
            Ok((input, Expr::Map(lowered)))
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            let (input, predicate) = match predicate {
                Some(predicate) => {
                    let (next, predicate) =
                        materialize_pre_aggregate_expr(lowerer, input, predicate)?;
                    (next, Some(Box::new(predicate)))
                }
                None => (input, None),
            };
            let (input, map) = materialize_pre_aggregate_expr(lowerer, input, map)?;
            Ok((
                input,
                Expr::ListComprehension {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate,
                    map: Box::new(map),
                },
            ))
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let pattern = materialize_pre_aggregate_pattern(lowerer, pattern)?;
            let (input, predicate) = match predicate {
                Some(predicate) => {
                    let (next, predicate) =
                        materialize_pre_aggregate_expr(lowerer, input, predicate)?;
                    (next, Some(Box::new(predicate)))
                }
                None => (input, None),
            };
            let (input, map) = materialize_pre_aggregate_expr(lowerer, input, map)?;
            Ok((
                input,
                Expr::PatternComprehension {
                    variable: variable.clone(),
                    pattern: Box::new(pattern),
                    predicate,
                    map: Box::new(map),
                },
            ))
        }
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            let (input, predicate) = materialize_pre_aggregate_expr(lowerer, input, predicate)?;
            Ok((
                input,
                Expr::Quantifier {
                    kind: *kind,
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate: Box::new(predicate),
                },
            ))
        }
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => Ok((input, expr.clone())),
    }
}

fn materialize_pre_aggregate_pattern(
    lowerer: &mut Lowerer,
    pattern: &PatternPart,
) -> CypherPlanResult<PatternPart> {
    let _ = lowerer;
    Ok(pattern.clone())
}

fn materialize_exists(
    lowerer: &mut Lowerer,
    input: Node,
    exists: &ExistsSubquery,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("exists");
    let count = lowerer.synthetic("exists_count");
    let right = lowerer.with_preserved_scope(|lowerer| {
        let right = lower_exists_right_plan(lowerer, exists)?;
        Ok(Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items: vec![ProjectionItem {
                alias: alias.clone(),
                expr: IrExpr::Binary {
                    op: IrBinaryOp::Gt,
                    lhs: Box::new(IrExpr::Binding(count.clone())),
                    rhs: Box::new(IrExpr::Lit(Lit::Int(0))),
                },
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: Node::GraphAggregate {
                group: Vec::new(),
                aggs: vec![AggCall {
                    kind: AggKind::CountRows,
                    alias: count.clone(),
                    arg: None,
                    distinct: false,
                }],
                fields: vec![count],
                input: right.boxed(),
            }
            .boxed(),
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

fn materialize_pattern_predicate(
    lowerer: &mut Lowerer,
    input: Node,
    patterns: &[PatternPart],
) -> CypherPlanResult<(Node, Expr)> {
    predicate::validate_pattern_predicate_scope(lowerer, patterns)?;
    let exists = ExistsSubquery {
        query: None,
        patterns: patterns.to_vec(),
        predicate: None,
    };
    materialize_exists(lowerer, input, &exists)
}

fn materialize_pattern_count(
    lowerer: &mut Lowerer,
    input: Node,
    patterns: &[PatternPart],
) -> CypherPlanResult<(Node, Expr)> {
    predicate::validate_pattern_predicate_scope(lowerer, patterns)?;
    let alias = lowerer.synthetic("pattern_count");
    let count = lowerer.synthetic("pattern_count_value");
    let exists = ExistsSubquery {
        query: None,
        patterns: patterns.to_vec(),
        predicate: None,
    };
    let right = lowerer.with_preserved_scope(|lowerer| {
        let right = lower_exists_right_plan(lowerer, &exists)?;
        Ok(Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items: vec![ProjectionItem {
                alias: alias.clone(),
                expr: IrExpr::Binding(count.clone()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: Node::GraphAggregate {
                group: Vec::new(),
                aggs: vec![AggCall {
                    kind: AggKind::CountRows,
                    alias: count.clone(),
                    arg: None,
                    distinct: false,
                }],
                fields: vec![count],
                input: right.boxed(),
            }
            .boxed(),
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

fn materialize_list_comprehension(
    lowerer: &mut Lowerer,
    input: Node,
    variable: &str,
    collection: &Expr,
    predicate_expr: Option<&Expr>,
    map: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("list");
    let predicate_needs_scope = predicate_expr
        .map(requires_scoped_materialization)
        .unwrap_or(false);
    if !requires_scoped_materialization(collection)
        && !predicate_needs_scope
        && !requires_scoped_materialization(map)
    {
        return Ok((
            Node::GraphListComprehension {
                input_expr: lower_expr(lowerer, collection)?,
                item: variable.to_string(),
                filter: predicate_expr
                    .map(|expr| lower_expr(lowerer, expr))
                    .transpose()?,
                map_expr: Some(lower_expr(lowerer, map)?),
                alias: alias.clone(),
                input: input.boxed(),
            },
            Expr::Variable(alias),
        ));
    }

    let collection_alias = lowerer.synthetic("list_collection");
    let collection_is_null = lowerer.synthetic("list_collection_null");
    let collected_alias = lowerer.synthetic("list_values");
    let right = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::ListComprehension, |lowerer| {
            let start = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let (right, collection) = lower_expr_with_input(lowerer, start, collection)?;
            let collection_project = Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                items: vec![
                    ProjectionItem {
                        alias: collection_alias.clone(),
                        expr: collection.clone(),
                    },
                    ProjectionItem {
                        alias: collection_is_null.clone(),
                        expr: IrExpr::IsNull(Box::new(collection)),
                    },
                ],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            };
            lowerer.add_visible(collection_alias.clone());
            lowerer.add_visible(collection_is_null.clone());
            let collection_scope = lowerer.visible_fields();
            let start = Node::GraphCorrelate {
                bindings: collection_scope.clone(),
            };
            let mut right = Node::GraphUnwind {
                input_expr: IrExpr::Binding(collection_alias.clone()),
                bind: variable.to_string(),
                outer: false,
                input: start.boxed(),
            };
            lowerer.add_visible(variable.to_string());
            if let Some(predicate_expr) = predicate_expr {
                right = predicate::lower_where_predicate(lowerer, right, predicate_expr)?;
            }
            let (right, value) = lower_expr_with_input(lowerer, right, map)?;
            let collect = Node::GraphCollect {
                value,
                distinct: false,
                order: Vec::new(),
                alias: collected_alias.clone(),
                input: right.boxed(),
            };
            let right = Node::GraphApply {
                kind: ApplyKind::Scalar,
                correlation: collection_scope,
                outputs: vec![collected_alias.clone()],
                optional_missing: OptionalMissing::Null,
                left: collection_project.boxed(),
                right: collect.boxed(),
            };
            Ok(Node::GraphProject {
                mode: ProjectMode::ReplaceScope,
                items: vec![ProjectionItem {
                    alias: alias.clone(),
                    expr: IrExpr::Case {
                        arms: vec![(
                            IrExpr::Binding(collection_is_null.clone()),
                            IrExpr::Lit(Lit::Null),
                        )],
                        otherwise: Some(Box::new(IrExpr::Binding(collected_alias.clone()))),
                    },
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            })
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

fn materialize_quantifier(
    lowerer: &mut Lowerer,
    input: Node,
    kind: QuantifierKind,
    variable: &str,
    collection: &Expr,
    predicate_expr: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("quantifier");
    if !requires_scoped_materialization(collection)
        && !requires_scoped_materialization(predicate_expr)
    {
        return Ok((
            Node::GraphQuantifier {
                kind: lower_quantifier_kind(kind),
                item_binding: variable.to_string(),
                input_expr: lower_expr(lowerer, collection)?,
                predicate: lower_expr(lowerer, predicate_expr)?,
                output: alias.clone(),
                input: input.boxed(),
            },
            Expr::Variable(alias),
        ));
    }

    let total_count = lowerer.synthetic("quantifier_total");
    let known_count = lowerer.synthetic("quantifier_known");
    let true_count = lowerer.synthetic("quantifier_true");
    let collection_alias = lowerer.synthetic("quantifier_collection");
    let collection_is_null = lowerer.synthetic("quantifier_collection_null");
    let right = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::Quantifier, |lowerer| {
            let start = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let (right, collection) = lower_expr_with_input(lowerer, start, collection)?;
            let right = Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                items: vec![
                    ProjectionItem {
                        alias: collection_alias.clone(),
                        expr: collection.clone(),
                    },
                    ProjectionItem {
                        alias: collection_is_null.clone(),
                        expr: IrExpr::IsNull(Box::new(collection)),
                    },
                ],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            };
            lowerer.add_visible(collection_alias.clone());
            lowerer.add_visible(collection_is_null.clone());
            let collection_scope = lowerer.visible_fields();

            let aggregate_start = Node::GraphCorrelate {
                bindings: collection_scope.clone(),
            };
            let rows = Node::GraphUnwind {
                input_expr: IrExpr::Binding(collection_alias.clone()),
                bind: variable.to_string(),
                outer: false,
                input: aggregate_start.boxed(),
            };
            lowerer.add_visible(variable.to_string());
            let (rows, predicate) = lower_expr_with_input(lowerer, rows, predicate_expr)?;
            let true_value = IrExpr::Case {
                arms: vec![(predicate.clone(), IrExpr::Lit(Lit::Int(1)))],
                otherwise: None,
            };
            let aggregate = Node::GraphAggregate {
                group: Vec::new(),
                aggs: vec![
                    AggCall {
                        kind: AggKind::CountRows,
                        alias: total_count.clone(),
                        arg: None,
                        distinct: false,
                    },
                    AggCall {
                        kind: AggKind::CountRows,
                        alias: known_count.clone(),
                        arg: Some(predicate),
                        distinct: false,
                    },
                    AggCall {
                        kind: AggKind::CountRows,
                        alias: true_count.clone(),
                        arg: Some(true_value),
                        distinct: false,
                    },
                ],
                fields: vec![total_count.clone(), known_count.clone(), true_count.clone()],
                input: rows.boxed(),
            };
            let right = Node::GraphApply {
                kind: ApplyKind::Scalar,
                correlation: collection_scope,
                outputs: vec![total_count.clone(), known_count.clone(), true_count.clone()],
                optional_missing: OptionalMissing::Null,
                left: right.boxed(),
                right: aggregate.boxed(),
            };
            Ok(Node::GraphProject {
                mode: ProjectMode::ReplaceScope,
                items: vec![ProjectionItem {
                    alias: alias.clone(),
                    expr: IrExpr::Case {
                        arms: vec![(
                            IrExpr::Binding(collection_is_null.clone()),
                            IrExpr::Lit(Lit::Null),
                        )],
                        otherwise: Some(Box::new(quantifier_result_expr(
                            kind,
                            &total_count,
                            &known_count,
                            &true_count,
                        ))),
                    },
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            })
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

fn materialize_pattern_comprehension(
    lowerer: &mut Lowerer,
    input: Node,
    _variable: Option<&str>,
    pattern_part: &PatternPart,
    predicate_expr: Option<&Expr>,
    map: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("pattern_list");
    let right = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::PatternComprehension, |lowerer| {
            let start = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let history = (!pattern_part.element.chains.is_empty())
                .then(|| lowerer.synthetic("pattern_history"));
            let mut right = pattern::lower_pattern_part(
                lowerer,
                start,
                pattern_part,
                false,
                history.as_deref(),
                false,
            )?;
            if let Some(predicate_expr) = predicate_expr {
                right = predicate::lower_where_predicate(lowerer, right, predicate_expr)?;
            }
            let (right, value) = lower_expr_with_input(lowerer, right, map)?;
            Ok(Node::GraphCollect {
                value,
                distinct: false,
                order: Vec::new(),
                alias: alias.clone(),
                input: right.boxed(),
            })
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

fn quantifier_result_expr(
    kind: QuantifierKind,
    total_count: &str,
    known_count: &str,
    true_count: &str,
) -> IrExpr {
    let total = IrExpr::Binding(total_count.to_string());
    let known = IrExpr::Binding(known_count.to_string());
    let true_values = IrExpr::Binding(true_count.to_string());
    let false_values = IrExpr::Binary {
        op: IrBinaryOp::Sub,
        lhs: Box::new(known.clone()),
        rhs: Box::new(true_values.clone()),
    };
    let null_values = IrExpr::Binary {
        op: IrBinaryOp::Sub,
        lhs: Box::new(total),
        rhs: Box::new(known.clone()),
    };
    let gt_zero = |expr: IrExpr| IrExpr::Binary {
        op: IrBinaryOp::Gt,
        lhs: Box::new(expr),
        rhs: Box::new(IrExpr::Lit(Lit::Int(0))),
    };
    let eq_zero = |expr: IrExpr| IrExpr::Binary {
        op: IrBinaryOp::Eq,
        lhs: Box::new(expr),
        rhs: Box::new(IrExpr::Lit(Lit::Int(0))),
    };
    let true_eq = |value: i64| IrExpr::Binary {
        op: IrBinaryOp::Eq,
        lhs: Box::new(true_values.clone()),
        rhs: Box::new(IrExpr::Lit(Lit::Int(value))),
    };
    let lit_bool = |value| IrExpr::Lit(Lit::Bool(value));
    let lit_null = IrExpr::Lit(Lit::Null);

    match kind {
        QuantifierKind::All => IrExpr::Case {
            arms: vec![
                (gt_zero(false_values), lit_bool(false)),
                (gt_zero(null_values), lit_null.clone()),
            ],
            otherwise: Some(Box::new(lit_bool(true))),
        },
        QuantifierKind::Any => IrExpr::Case {
            arms: vec![
                (gt_zero(true_values.clone()), lit_bool(true)),
                (gt_zero(null_values), lit_null.clone()),
            ],
            otherwise: Some(Box::new(lit_bool(false))),
        },
        QuantifierKind::None => IrExpr::Case {
            arms: vec![
                (gt_zero(true_values.clone()), lit_bool(false)),
                (gt_zero(null_values), lit_null.clone()),
            ],
            otherwise: Some(Box::new(lit_bool(true))),
        },
        QuantifierKind::Single => IrExpr::Case {
            arms: vec![
                (
                    IrExpr::Binary {
                        op: IrBinaryOp::Gt,
                        lhs: Box::new(true_values.clone()),
                        rhs: Box::new(IrExpr::Lit(Lit::Int(1))),
                    },
                    lit_bool(false),
                ),
                (
                    IrExpr::Binary {
                        op: IrBinaryOp::And,
                        lhs: Box::new(true_eq(1)),
                        rhs: Box::new(eq_zero(null_values.clone())),
                    },
                    lit_bool(true),
                ),
                (
                    IrExpr::Binary {
                        op: IrBinaryOp::And,
                        lhs: Box::new(true_eq(0)),
                        rhs: Box::new(eq_zero(null_values)),
                    },
                    lit_bool(false),
                ),
            ],
            otherwise: Some(Box::new(lit_null)),
        },
    }
}

fn lower_exists_right_plan(
    lowerer: &mut Lowerer,
    exists: &ExistsSubquery,
) -> CypherPlanResult<Node> {
    lowerer.with_child_traversal(CypherTraversalKind::ExistsSubquery, |lowerer| {
        if let Some(query) = &exists.query {
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
            if let Some(predicate_expr) = &exists.predicate {
                right = lowerer.with_child_traversal(
                    CypherTraversalKind::WherePredicate,
                    |lowerer| {
                        let right =
                            predicate::lower_where_predicate(lowerer, right, predicate_expr)?;
                        lowerer.record_current_imports(lowerer.visible_fields());
                        lowerer.record_current_correlation(lowerer.visible_fields());
                        Ok(right)
                    },
                )?;
            }
            Ok(right)
        }
    })
}

pub fn lower_expr(lowerer: &Lowerer, expr: &Expr) -> CypherPlanResult<IrExpr> {
    Ok(match expr {
        Expr::Star => IrExpr::Call {
            name: "cypher_star".to_string(),
            args: lowerer
                .visible_fields()
                .into_iter()
                .map(IrExpr::Binding)
                .collect(),
        },
        Expr::Variable(name) => IrExpr::Binding(name.clone()),
        Expr::Property { target, key } if key == "*" => IrExpr::Call {
            name: "cypher_property_star".to_string(),
            args: vec![lower_expr(lowerer, target)?],
        },
        Expr::Property { target, key } => match target.as_ref() {
            Expr::Variable(binding) => IrExpr::Property {
                binding: binding.clone(),
                name: key.clone(),
                policy: PropertyMissing::NullOnMissing,
            },
            other => IrExpr::Call {
                name: "property".to_string(),
                args: vec![
                    lower_expr(lowerer, other)?,
                    IrExpr::Lit(Lit::String(key.clone())),
                ],
            },
        },
        Expr::LabelPredicate { target, labels } => match target.as_ref() {
            Expr::Variable(binding) => IrExpr::and(
                labels
                    .iter()
                    .map(|label| IrExpr::HasLabel {
                        binding: binding.clone(),
                        label: label.clone(),
                    })
                    .collect(),
            ),
            other => lower_label_predicate_expr(lower_expr(lowerer, other)?, labels),
        },
        Expr::Parameter(name) => IrExpr::Call {
            name: "parameter".to_string(),
            args: vec![IrExpr::Lit(Lit::String(name.clone()))],
        },
        Expr::Literal(lit) => IrExpr::Lit(match lit {
            Literal::Null => Lit::Null,
            Literal::Bool(value) => Lit::Bool(*value),
            Literal::Integer(value) => {
                if let Ok(value) = value.parse::<i64>() {
                    Lit::Int(value)
                } else {
                    return Ok(IrExpr::Call {
                        name: "integer_literal".to_string(),
                        args: vec![IrExpr::Lit(Lit::String(value.clone()))],
                    });
                }
            }
            Literal::Float(value) => Lit::Float(*value),
            Literal::String(value) => Lit::String(value.clone()),
        }),
        Expr::List(items) => IrExpr::List(
            items
                .iter()
                .map(|item| lower_expr(lowerer, item))
                .collect::<CypherPlanResult<_>>()?,
        ),
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => IrExpr::ListReduce {
            collection: Box::new(lower_expr(lowerer, collection)?),
            accumulator: accumulator.clone(),
            item: variable.clone(),
            map: Box::new(lower_expr(lowerer, map)?),
        },
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => IrExpr::ListTransform {
            list: Box::new(lower_expr(lowerer, collection)?),
            item: variable.clone(),
            map: Box::new(lower_expr(lowerer, map)?),
        },
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => IrExpr::ListFilter {
            list: Box::new(lower_expr(lowerer, collection)?),
            item: variable.clone(),
            predicate: Box::new(lower_expr(lowerer, predicate)?),
        },
        Expr::Map(items) => IrExpr::Call {
            name: "map".to_string(),
            args: items
                .iter()
                .flat_map(|(key, value)| {
                    vec![
                        Ok(IrExpr::Lit(Lit::String(key.clone()))),
                        lower_expr(lowerer, value),
                    ]
                })
                .collect::<CypherPlanResult<_>>()?,
        },
        Expr::Unary { op, expr } => match op {
            UnaryOp::Not => IrExpr::Not(Box::new(lower_expr(lowerer, expr)?)),
            UnaryOp::Neg => IrExpr::Binary {
                op: IrBinaryOp::Sub,
                lhs: Box::new(IrExpr::Lit(Lit::Int(0))),
                rhs: Box::new(lower_expr(lowerer, expr)?),
            },
        },
        Expr::Binary { op, lhs, rhs } => {
            let lhs = lower_expr(lowerer, lhs)?;
            let rhs = lower_expr(lowerer, rhs)?;
            lower_cypher_binary_expr(*op, lhs, rhs)
        }
        Expr::IsNull(expr) => IrExpr::IsNull(Box::new(lower_expr(lowerer, expr)?)),
        Expr::IsNotNull(expr) => IrExpr::IsNotNull(Box::new(lower_expr(lowerer, expr)?)),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => lower_string_predicate_expr(
            *op,
            lower_expr(lowerer, target)?,
            lower_expr(lowerer, pattern)?,
        ),
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            if name.eq_ignore_ascii_case("typeof") && args.len() == 1 {
                if let Some(type_name) = static_typeof_expr(&args[0]) {
                    return Ok(IrExpr::Lit(Lit::String(type_name)));
                }
            }
            if aggregate_kind(name).is_some() {
                return Err(CypherPlanError::Unsupported(
                    "aggregate functions must be lowered through aggregate projection".to_string(),
                ));
            }
            if *distinct {
                return Err(CypherPlanError::Invalid(format!(
                    "DISTINCT is only valid for aggregate function `{name}`"
                )));
            }
            IrExpr::Call {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| lower_expr(lowerer, arg))
                    .collect::<CypherPlanResult<_>>()?,
            }
        }
        Expr::CountStar => {
            return Err(CypherPlanError::Unsupported(
                "count(*) must be lowered through aggregate projection".to_string(),
            ));
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            let case_expr = case
                .as_ref()
                .map(|expr| lower_expr(lowerer, expr))
                .transpose()?;
            IrExpr::Case {
                arms: arms
                    .iter()
                    .map(|(when, then)| {
                        let condition = if let Some(case_expr) = &case_expr {
                            IrExpr::Binary {
                                op: IrBinaryOp::Eq,
                                lhs: Box::new(case_expr.clone()),
                                rhs: Box::new(lower_expr(lowerer, when)?),
                            }
                        } else {
                            lower_expr(lowerer, when)?
                        };
                        Ok((condition, lower_expr(lowerer, then)?))
                    })
                    .collect::<CypherPlanResult<_>>()?,
                otherwise: otherwise
                    .as_ref()
                    .map(|expr| lower_expr(lowerer, expr).map(Box::new))
                    .transpose()?,
            }
        }
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::ListComprehension { .. }
        | Expr::PatternComprehension { .. }
        | Expr::Quantifier { .. } => {
            return Err(CypherPlanError::Unsupported(
                "scoped Cypher expression reached scalar lowering without materialization"
                    .to_string(),
            ));
        }
    })
}

fn static_typeof_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Literal(Literal::Null) => Some("NULL".to_string()),
        Expr::Literal(Literal::Bool(_)) => Some("BOOL".to_string()),
        Expr::Literal(Literal::Float(_)) => Some("DOUBLE".to_string()),
        Expr::Literal(Literal::String(_)) => Some("STRING".to_string()),
        Expr::Literal(Literal::Integer(value)) => Some(integer_literal_type(value).to_string()),
        Expr::List(items) => {
            let item_type = items
                .iter()
                .filter_map(static_typeof_expr)
                .reduce(unify_numeric_type)
                .unwrap_or_else(|| "ANY".to_string());
            Some(format!("{item_type}[]"))
        }
        Expr::Map(items) => {
            let fields = items
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{key} {}",
                        static_typeof_expr(value).unwrap_or_else(|| "ANY".to_string())
                    )
                })
                .collect::<Vec<_>>();
            Some(format!("STRUCT({})", fields.join(", ")))
        }
        Expr::Function { name, args, .. }
            if name.eq_ignore_ascii_case("cast") && args.len() == 2 =>
        {
            match &args[1] {
                Expr::Literal(Literal::String(type_name)) => Some(type_name.to_ascii_uppercase()),
                _ => None,
            }
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("date") => {
            Some("DATE".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("timestamp") => {
            Some("TIMESTAMP".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("interval") => {
            Some("INTERVAL".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("blob") => {
            Some("BLOB".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("gen_random_uuid") => {
            Some("UUID".to_string())
        }
        Expr::Function { name, args, .. }
            if name.eq_ignore_ascii_case("map") && args.len() == 2 =>
        {
            Some(format!(
                "MAP({}, {})",
                static_collection_item_type(&args[0]).unwrap_or_else(|| "ANY".to_string()),
                static_collection_item_type(&args[1]).unwrap_or_else(|| "ANY".to_string())
            ))
        }
        Expr::Function { name, args, .. } if name.eq_ignore_ascii_case("union_value") => {
            if let [Expr::Literal(Literal::String(tag)), value] = args.as_slice() {
                Some(format!(
                    "UNION({tag} {})",
                    static_typeof_expr(value).unwrap_or_else(|| "ANY".to_string())
                ))
            } else if let [Expr::Map(fields)] = args.as_slice() {
                fields.first().map(|(tag, value)| {
                    format!(
                        "UNION({tag} {})",
                        static_typeof_expr(value).unwrap_or_else(|| "ANY".to_string())
                    )
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn static_collection_item_type(expr: &Expr) -> Option<String> {
    static_typeof_expr(expr).map(|type_name| {
        type_name
            .strip_suffix("[]")
            .unwrap_or(type_name.as_str())
            .to_string()
    })
}

fn integer_literal_type(value: &str) -> &'static str {
    if value.parse::<i64>().is_ok() {
        "INT64"
    } else if value.parse::<i128>().is_ok() {
        "INT128"
    } else {
        "UINT128"
    }
}

fn unify_numeric_type(left: String, right: String) -> String {
    if left == right {
        return left;
    }
    let rank = |value: &str| match value {
        "UINT128" => 4,
        "INT128" => 3,
        "DOUBLE" | "FLOAT" => 2,
        "INT64" | "INT32" | "INT16" | "INT8" | "UINT64" | "UINT32" | "UINT16" | "UINT8" => 1,
        _ => 0,
    };
    if rank(&right) > rank(&left) {
        right
    } else {
        left
    }
}

fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::CountStar => true,
        Expr::Function { name, args, .. } => {
            aggregate_kind(name).is_some() || args.iter().any(contains_aggregate)
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            case.as_deref().is_some_and(contains_aggregate)
                || arms
                    .iter()
                    .any(|(when, then)| contains_aggregate(when) || contains_aggregate(then))
                || otherwise.as_deref().is_some_and(contains_aggregate)
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            contains_aggregate(target)
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            contains_aggregate(expr)
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => contains_aggregate(lhs) || contains_aggregate(rhs),
        Expr::List(items) => items.iter().any(contains_aggregate),
        Expr::Map(items) => items.iter().any(|(_, value)| contains_aggregate(value)),
        Expr::Exists(_) | Expr::PatternPredicate(_) => false,
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            contains_aggregate(collection)
                || predicate.as_deref().is_some_and(contains_aggregate)
                || contains_aggregate(map)
        }
        Expr::ListReduce {
            collection, map, ..
        } => contains_aggregate(collection) || contains_aggregate(map),
        Expr::ListTransform {
            collection, map, ..
        } => contains_aggregate(collection) || contains_aggregate(map),
        Expr::ListFilter {
            collection,
            predicate,
            ..
        } => contains_aggregate(collection) || contains_aggregate(predicate),
        Expr::PatternComprehension {
            pattern,
            predicate,
            map,
            ..
        } => {
            pattern_contains_aggregate(pattern)
                || predicate.as_deref().is_some_and(contains_aggregate)
                || contains_aggregate(map)
        }
        Expr::Quantifier {
            collection,
            predicate,
            ..
        } => contains_aggregate(collection) || contains_aggregate(predicate),
        _ => false,
    }
}

fn pattern_contains_aggregate(pattern: &PatternPart) -> bool {
    pattern
        .element
        .start
        .properties
        .as_ref()
        .is_some_and(contains_aggregate)
        || pattern.element.chains.iter().any(|chain| {
            chain
                .relationship
                .properties
                .as_ref()
                .is_some_and(contains_aggregate)
                || chain
                    .node
                    .properties
                    .as_ref()
                    .is_some_and(contains_aggregate)
        })
}

fn lower_quantifier_kind(kind: QuantifierKind) -> IrQuantifierKind {
    match kind {
        QuantifierKind::All => IrQuantifierKind::All,
        QuantifierKind::Any => IrQuantifierKind::Any,
        QuantifierKind::None => IrQuantifierKind::None,
        QuantifierKind::Single => IrQuantifierKind::Single,
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
        assert!(format!("{err}").contains("slice bounds must be non-negative integers"));
    }
}

fn requires_scoped_materialization(expr: &Expr) -> bool {
    match expr {
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::ListComprehension { .. }
        | Expr::PatternComprehension { .. }
        | Expr::Quantifier { .. } => true,
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            requires_scoped_materialization(expr)
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => requires_scoped_materialization(lhs) || requires_scoped_materialization(rhs),
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            requires_scoped_materialization(target)
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            args.iter().any(requires_scoped_materialization)
        }
        Expr::ListReduce {
            collection, map, ..
        } => requires_scoped_materialization(collection) || requires_scoped_materialization(map),
        Expr::ListTransform {
            collection, map, ..
        } => requires_scoped_materialization(collection) || requires_scoped_materialization(map),
        Expr::ListFilter {
            collection,
            predicate,
            ..
        } => {
            requires_scoped_materialization(collection)
                || requires_scoped_materialization(predicate)
        }
        Expr::Map(items) => items
            .iter()
            .any(|(_, value)| requires_scoped_materialization(value)),
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            case.as_deref().is_some_and(requires_scoped_materialization)
                || arms.iter().any(|(when, then)| {
                    requires_scoped_materialization(when) || requires_scoped_materialization(then)
                })
                || otherwise
                    .as_deref()
                    .is_some_and(requires_scoped_materialization)
        }
        _ => false,
    }
}
