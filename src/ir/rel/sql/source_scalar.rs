//! Evaluate total source-owned scalars on their owning input before join fanout.
//! A materialized scalar boundary prevents PostgreSQL from inlining the work
//! back into each joined row. Only explicitly eligible functions may move.
use super::*;
use datafusion::logical_expr::{JoinType, LogicalPlanBuilder, col};

fn available(expr: &Expr, plan: &LogicalPlan) -> bool {
    let columns = expr.column_refs();
    !columns.is_empty()
        && columns
            .iter()
            .all(|c| plan.schema().index_of_column(c).is_ok())
}
fn push(
    plan: LogicalPlan,
    expr: Expr,
    name: &str,
    crossed: bool,
) -> datafusion::common::Result<Option<LogicalPlan>> {
    match &plan {
        LogicalPlan::Projection(p) => {
            let mut safe = true;
            let mapped = expr
                .clone()
                .transform_up(|e| {
                    let Expr::Column(c) = &e else {
                        return Ok(Transformed::no(e));
                    };
                    let Ok(i) = p.schema.index_of_column(c) else {
                        safe = false;
                        return Ok(Transformed::no(e));
                    };
                    let value = match &p.expr[i] {
                        Expr::Alias(a) => a.expr.as_ref(),
                        e => e,
                    };
                    if !matches!(value, Expr::Column(_) | Expr::Literal(..)) {
                        safe = false;
                    }
                    Ok(Transformed::yes(value.clone()))
                })?
                .data;
            if safe {
                if let Some(input) = push(p.input.as_ref().clone(), mapped, name, crossed)? {
                    let mut columns = p.expr.clone();
                    columns.push(col(name));
                    return Ok(Some(
                        LogicalPlanBuilder::from(input).project(columns)?.build()?,
                    ));
                }
            }
        }
        LogicalPlan::SubqueryAlias(a) => {
            let mapped = expr
                .clone()
                .transform_up(|e| {
                    let Expr::Column(c) = &e else {
                        return Ok(Transformed::no(e));
                    };
                    let i = a.schema.index_of_column(c)?;
                    Ok(Transformed::yes(Expr::Column(
                        a.input.schema().columns()[i].clone(),
                    )))
                })?
                .data;
            if let Some(input) = push(a.input.as_ref().clone(), mapped, name, crossed)? {
                return Ok(Some(
                    LogicalPlanBuilder::from(input)
                        .alias(a.alias.clone())?
                        .build()?,
                ));
            }
        }
        LogicalPlan::Filter(f) if f.input.exists(|p| Ok(matches!(p, LogicalPlan::Join(_))))? => {
            if let Some(input) = push(f.input.as_ref().clone(), expr.clone(), name, crossed)? {
                return Ok(Some(
                    LogicalPlanBuilder::from(input)
                        .filter(f.predicate.clone())?
                        .build()?,
                ));
            }
        }
        LogicalPlan::Join(j) if matches!(j.join_type, JoinType::Inner | JoinType::Left) => {
            let inputs = [&j.left, &j.right];
            // A left-only total expression is unchanged by null extension.
            // Never move a right-side expression below an outer join here.
            for side in 0..if j.join_type == JoinType::Left { 1 } else { 2 } {
                if available(&expr, inputs[side]) && !available(&expr, inputs[1 - side]) {
                    if let Some(input) =
                        push(inputs[side].as_ref().clone(), expr.clone(), name, true)?
                    {
                        let mut children = vec![j.left.as_ref().clone(), j.right.as_ref().clone()];
                        children[side] = input;
                        return Ok(Some(plan.with_new_exprs(plan.expressions(), children)?));
                    }
                }
            }
        }
        _ => {}
    }
    if !crossed || !available(&expr, &plan) {
        return Ok(None);
    }
    let original = plan.schema().clone();
    let mut columns = original
        .columns()
        .into_iter()
        .map(Expr::Column)
        .collect::<Vec<_>>();
    columns.push(expr.alias(name));
    let cte = format!("__w_sql_cte_source_scalar_{name}");
    let shared = LogicalPlanBuilder::from(plan)
        .project(columns)?
        .alias(cte)?
        .build()?;
    let mut restored = original
        .iter()
        .enumerate()
        .map(|(i, (q, f))| {
            Expr::Column(shared.schema().columns()[i].clone()).alias_qualified(q.cloned(), f.name())
        })
        .collect::<Vec<_>>();
    restored.push(Expr::Column(shared.schema().columns().last().unwrap().clone()).alias(name));
    Ok(Some(
        LogicalPlanBuilder::from(shared)
            .project(restored)?
            .build()?,
    ))
}

pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    let mut names = std::collections::BTreeSet::new();
    plan.apply_with_subqueries(|p| {
        names.extend(p.schema().fields().iter().map(|f| f.name().clone()));
        Ok(TreeNodeRecursion::Continue)
    })?;
    let mut serial = 0;
    Ok(plan
        .transform_up_with_subqueries(|node| {
            let LogicalPlan::Projection(p) = &node else {
                return Ok(Transformed::no(node));
            };
            let mut candidates = Vec::new();
            for e in &p.expr {
                e.apply(|e| {
                    if let Expr::ScalarFunction(f) = e {
                        if crate::ir::functions::logical::definition(&f.func)
                            .is_some_and(|f| f.precompute_before_join)
                            && f.args
                                .iter()
                                .all(|a| matches!(a, Expr::Column(_) | Expr::Literal(..)))
                            && !candidates.contains(e)
                        {
                            candidates.push(e.clone());
                        }
                    }
                    Ok(TreeNodeRecursion::Continue)
                })?;
            }
            let mut input = p.input.as_ref().clone();
            let mut expressions = p.expr.clone();
            let mut changed = false;
            for expr in candidates {
                let name = loop {
                    let n = format!("__source_scalar_{serial}");
                    serial += 1;
                    if names.insert(n.clone()) {
                        break n;
                    }
                };
                if let Some(next) = push(input.clone(), expr.clone(), &name, false)? {
                    input = next;
                    expressions = expressions
                        .into_iter()
                        .map(|e| {
                            e.transform_up(|e| {
                                if e == expr {
                                    Ok(Transformed::yes(col(&name)))
                                } else {
                                    Ok(Transformed::no(e))
                                }
                            })
                            .map(|t| t.data)
                        })
                        .collect::<datafusion::common::Result<Vec<_>>>()?;
                    changed = true;
                }
            }
            if !changed {
                return Ok(Transformed::no(node));
            }
            Ok(Transformed::yes(
                LogicalPlanBuilder::from(input)
                    .project(expressions)?
                    .build()?,
            ))
        })?
        .data)
}
