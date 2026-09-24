//! Aggregate projection rewriting and grouping key construction.

use super::expression::{
    is_typed_negate_operand, lower_cypher_binary_expr, lower_expr, lower_label_predicate_expr,
    lower_string_predicate_expr, render_kuzu_expr,
};
use super::materialize::{
    lower_expr_with_input, materialize_pre_aggregate_expr, requires_scoped_materialization,
};
use super::scope::{
    add_candidate_pattern_bindings, free_variable_names, free_variable_names_for_local_scope,
    pattern_binding_names, remove_local_exists_bindings, validate_expression_scope,
};
use super::{
    AggCall, AggKind, BTreeSet, CypherPlanError, CypherPlanResult, CypherTraversalKind, Expr,
    IrBinaryOp, IrExpr, Lit, Lowerer, Node, PatternPart, ProjectErrorPolicy, ProjectMode,
    ProjectionBody, ProjectionItem, SortKey, UnaryOp, invalid_order_scope,
    order_expr_after_cardinality_projection, sort_key,
};
pub(super) fn lower_aggregate(
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

pub(super) struct AggregateLowering {
    pub(super) node: Node,
    pub(super) sort_keys: Option<Vec<SortKey>>,
    pub(super) hidden_sort_fields: Vec<String>,
}

#[derive(Default)]
pub(super) struct AggregateRewrite {
    group: Vec<ProjectionItem>,
    aggs: Vec<AggCall>,
}

pub(super) fn rewrite_aggregate_projection_expr(
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
                return Err(nested_aggregate_error(name, args));
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
                _ => AggKind::EngineFunction,
            };
            let arg = match kind {
                AggKind::EngineFunction => Some(IrExpr::Call {
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| lower_expr(lowerer, arg))
                        .collect::<CypherPlanResult<_>>()?,
                }),
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

pub(super) fn rewrite_non_aggregate_projection_expr(
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

pub(super) fn ensure_scoped_outer_refs_grouped(
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

pub(super) fn rewrite_aggregate_pattern(
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
pub(super) fn rewrite_aggregate_projection(
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
                return Err(nested_aggregate_error(name, args));
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
                _ => AggKind::EngineFunction,
            };
            let arg = match kind {
                AggKind::EngineFunction => Some(IrExpr::Call {
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| lower_expr(lowerer, arg))
                        .collect::<CypherPlanResult<_>>()?,
                }),
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
            let typed_negate = matches!(op, UnaryOp::Neg) && is_typed_negate_operand(expr);
            let expr = rewrite_aggregate_projection(lowerer, expr, rewrite, None)?;
            Ok(match op {
                UnaryOp::Not => IrExpr::Not(Box::new(expr)),
                UnaryOp::Neg if typed_negate => IrExpr::Call {
                    name: "negate".to_string(),
                    args: vec![expr],
                },
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

pub(super) fn ensure_group_key(
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

pub(super) fn aggregate_kind(name: &str) -> Option<AggKind> {
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
        _ if crate::ir::functions::is_native_aggregate(name) => Some(AggKind::EngineFunction),
        _ => None,
    }
}

pub(super) fn nested_aggregate_error(name: &str, args: &[Expr]) -> CypherPlanError {
    CypherPlanError::Unsupported(format!(
        "Binder exception: Expression {}({}) contains nested aggregation.",
        name.to_ascii_uppercase(),
        args.iter()
            .map(render_kuzu_expr)
            .collect::<Vec<_>>()
            .join(",")
    ))
}

pub(super) fn contains_aggregate(expr: &Expr) -> bool {
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

pub(super) fn pattern_contains_aggregate(pattern: &PatternPart) -> bool {
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
