//! Share repeated pure partition unions without changing bag multiplicity.
use super::*;
use datafusion::common::tree_node::{Transformed, TreeNodeRecursion};
use datafusion::logical_expr::{LogicalPlan, LogicalPlanBuilder};

fn scalar(expr: &Expr) -> bool {
    match expr {
        Expr::Column(_) | Expr::Literal(..) => true,
        Expr::Alias(a) => scalar(&a.expr),
        Expr::Cast(a) => scalar(&a.expr),
        _ => false,
    }
}
fn predicate(expr: &Expr) -> bool {
    use datafusion::logical_expr::Operator;
    match expr {
        Expr::BinaryExpr(b) if matches!(b.op, Operator::And | Operator::Or) =>
            predicate(&b.left) && predicate(&b.right),
        Expr::BinaryExpr(b) if matches!(b.op, Operator::Eq | Operator::NotEq) =>
            scalar(&b.left) && scalar(&b.right),
        Expr::IsNull(e) | Expr::IsNotNull(e) => scalar(e),
        Expr::Literal(..) => true,
        _ => false,
    }
}
fn scan_tree(plan: &LogicalPlan) -> bool {
    match plan {
        LogicalPlan::TableScan(s) => s.filters.is_empty() && s.fetch.is_none(),
        LogicalPlan::Projection(p) => p.expr.iter().all(scalar) && scan_tree(&p.input),
        LogicalPlan::Union(u) => u.inputs.iter().all(|p| scan_tree(p)),
        LogicalPlan::Filter(f) => predicate(&f.predicate) && scan_tree(&f.input),
        _ => false,
    }
}

// Filters on physical columns belong to the source, not to a graph binding.
// Their exact expressions remain part of the canonical identity.
fn physical_scan(plan: &LogicalPlan) -> bool {
    match plan {
        LogicalPlan::TableScan(_) => true,
        LogicalPlan::Filter(f) => predicate(&f.predicate) && physical_scan(&f.input),
        _ => false,
    }
}

// Binding aliases are consumers of a partition scan, not part of the scan's
// identity. Normalize only leaf output aliases; physical column references,
// casts, types, provider identities and UNION ALL branches remain unchanged.
fn canonical(plan: &LogicalPlan) -> datafusion::common::Result<Option<LogicalPlan>> {
    match plan {
        LogicalPlan::Union(u) => {
            let mut inputs = Vec::new();
            for input in &u.inputs {
                let Some(input) = canonical(input)? else {
                    return Ok(None);
                };
                inputs.push(input);
            }
            let mut inputs = inputs.into_iter();
            let Some(first) = inputs.next() else {
                return Ok(None);
            };
            let mut builder = LogicalPlanBuilder::from(first);
            for input in inputs {
                builder = builder.union(input)?;
            }
            Ok(Some(builder.build()?))
        }
        LogicalPlan::Projection(p)
            if physical_scan(&p.input) && scan_tree(plan) =>
        {
            let exprs = p
                .expr
                .iter()
                .enumerate()
                .map(|(i, expr)| {
                    let value = match expr {
                        Expr::Alias(a) => *a.expr.clone(),
                        e => e.clone(),
                    };
                    value.alias(format!("c{i}"))
                })
                .collect::<Vec<_>>();
            Ok(Some(
                LogicalPlanBuilder::from(p.input.as_ref().clone())
                    .project(exprs)?
                    .build()?,
            ))
        }
        _ => Ok(None),
    }
}
fn candidate(node: &LogicalPlan) -> datafusion::common::Result<Option<LogicalPlan>> {
    if matches!(node, LogicalPlan::Union(_))
        && scan_tree(node)
        && node.schema().iter().all(|(q, _)| q.is_none())
    {
        Ok(Some(canonical(node)?.unwrap_or_else(|| node.clone())))
    } else {
        Ok(None)
    }
}

pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    // Restrict sharing to deterministic source trees, with exact structural
    // equality (including provider identity and projected types). No name-only
    // matching, volatile expressions or occurrence-numbering sources. Pure
    // equality/null filters permit sharing undirected edge orientations while
    // keeping their self-loop exclusion and UNION ALL multiplicity intact.
    let mut candidates: Vec<(LogicalPlan, usize)> = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    plan.apply_with_subqueries(|node| {
        match node {
            LogicalPlan::TableScan(s) => {
                names.insert(s.table_name.table().to_owned());
            }
            LogicalPlan::SubqueryAlias(a) => {
                names.insert(a.alias.table().to_owned());
            }
            LogicalPlan::RecursiveQuery(r) => {
                names.insert(r.name.clone());
            }
            _ => {}
        }
        if let Some(normalized) = candidate(node)? {
            if let Some((_, count)) = candidates.iter_mut().find(|(p, _)| p == &normalized) {
                *count += 1;
            } else {
                candidates.push((normalized, 1));
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    let mut selected = Vec::new();
    for (candidate, count) in candidates {
        if count < 2 {
            continue;
        }
        let mut i = selected.len();
        let name = loop {
            let name = format!("__w_sql_cte_shared_scan_{i}");
            if names.insert(name.clone()) {
                break name;
            }
            i += 1;
        };
        selected.push((candidate, name));
    }
    Ok(plan
        .transform_down_with_subqueries(|node| {
            if let Some(normalized) = candidate(&node)?
                && let Some((_, name)) = selected.iter().find(|(p, _)| p == &normalized)
            {
                let names = node
                    .schema()
                    .fields()
                    .iter()
                    .map(|f| f.name().clone())
                    .collect::<Vec<_>>();
                let shared = LogicalPlanBuilder::from(normalized)
                    .alias(name.clone())?
                    .build()?;
                let columns = shared
                    .schema()
                    .columns()
                    .into_iter()
                    .zip(names)
                    .map(|(column, name)| Expr::Column(column).alias(name))
                    .collect::<Vec<_>>();
                let shared = LogicalPlanBuilder::from(shared).project(columns)?.build()?;
                return Ok(Transformed::new(shared, true, TreeNodeRecursion::Jump));
            }
            Ok(Transformed::no(node))
        })?
        .data)
}
