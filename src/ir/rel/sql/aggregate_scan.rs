//! Resolve aggregate operands through a trivial physical-scan naming layer.
//! This removes no computation: every projected value is a column or literal.
use super::*;
use datafusion::logical_expr::Aggregate;

fn value(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Alias(alias) => value(&alias.expr),
        Expr::Column(_) | Expr::Literal(..) => Some(expr.clone()),
        _ => None,
    }
}

pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Aggregate(aggregate) = &node else { return Ok(Transformed::no(node)); };
        let LogicalPlan::Projection(projection) = aggregate.input.as_ref() else { return Ok(Transformed::no(node)); };
        let LogicalPlan::TableScan(scan) = projection.input.as_ref() else { return Ok(Transformed::no(node)); };
        if scan.fetch.is_some() || !scan.filters.is_empty() { return Ok(Transformed::no(node)); }
        let Some(values) = projection.expr.iter().map(value).collect::<Option<Vec<_>>>() else { return Ok(Transformed::no(node)); };
        let mut subquery = false;
        for expr in aggregate.group_expr.iter().chain(&aggregate.aggr_expr) {
            expr.apply(|expr| {
                if matches!(expr, Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_) | Expr::OuterReferenceColumn(..) | Expr::GroupingSet(_)) {
                    subquery = true;
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        if subquery { return Ok(Transformed::no(node)); }
        let substitute = |expr: &Expr| -> datafusion::common::Result<Expr> {
            Ok(expr.clone().transform_up(|expr| {
                if let Expr::Column(column) = &expr {
                    let index = projection.schema.index_of_column(column)?;
                    return Ok(Transformed::yes(values[index].clone()));
                }
                Ok(Transformed::no(expr))
            })?.data)
        };
        let groups = aggregate.group_expr.iter().map(substitute)
            .collect::<datafusion::common::Result<Vec<_>>>()?;
        // Do not expose a projected literal as GROUP BY 1 (an SQL ordinal),
        // or let an emitter turn a constant group into a global aggregate.
        // Inspect after substitution: a named grouping column may be a literal.
        for group in &groups {
            let mut bound_column = false;
            group.apply(|expr| {
                if matches!(expr, Expr::Column(_)) { bound_column = true; }
                Ok(TreeNodeRecursion::Continue)
            })?;
            if !bound_column { return Ok(Transformed::no(node)); }
        }
        // Retain every output name/qualifier, including group-key fields read by
        // parent operators. Only operands change; aggregate filters, ordering,
        // distinctness, null treatment and grouping-key functions stay intact.
        let preserve_name = |expr: Expr, index: usize| {
            let (qualifier, field) = aggregate.schema.qualified_field(index);
            let expr = match expr { Expr::Alias(alias) => *alias.expr, expr => expr };
            expr.alias_qualified(qualifier.cloned(), field.name())
        };
        let group_count = groups.len();
        let groups = groups.into_iter().enumerate().map(|(i, expr)| preserve_name(expr, i)).collect();
        let expressions = aggregate.aggr_expr.iter().enumerate()
            .map(|(i, expr)| substitute(expr).map(|expr| preserve_name(expr, group_count + i)))
            .collect::<datafusion::common::Result<Vec<_>>>()?;
        let rewritten = Aggregate::try_new_with_schema(projection.input.clone(), groups, expressions, aggregate.schema.clone())?;
        Ok(Transformed::yes(LogicalPlan::Aggregate(rewritten)))
    })?.data)
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__sql__aggregate_scan__tests.rs"]
mod tests;
