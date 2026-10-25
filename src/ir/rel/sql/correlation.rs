//! Preserve projection namespaces around correlated SQL expressions so physical
//! source columns cannot capture logical outer references.
use super::*;
use datafusion::logical_expr::LogicalPlanBuilder;
fn correlated(exprs: &[Expr]) -> datafusion::common::Result<bool> {
    let mut found = false;
    for expr in exprs {
        expr.apply(|expr| {
            found |= match expr {
                Expr::ScalarSubquery(s) => !s.outer_ref_columns.is_empty(),
                Expr::Exists(s) => !s.subquery.outer_ref_columns.is_empty(),
                Expr::InSubquery(s) => !s.subquery.outer_ref_columns.is_empty(),
                Expr::SetComparison(s) => !s.subquery.outer_ref_columns.is_empty(),
                _ => false,
            };
            Ok(TreeNodeRecursion::Continue)
        })?;
    }
    Ok(found)
}
pub(super) fn preserve_scopes(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    let mut next = 0;
    let mut names = std::collections::BTreeSet::new();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::SubqueryAlias(alias) = node {
            names.insert(alias.alias.table().to_owned());
        }
        if let LogicalPlan::TableScan(scan) = node {
            names.insert(scan.table_name.table().to_owned());
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(plan
        .transform_up_with_subqueries(|node| {
            let LogicalPlan::Projection(mut projection) = node else {
                return Ok(Transformed::no(node));
            };
            if !correlated(&projection.expr)? {
                return Ok(Transformed::no(LogicalPlan::Projection(projection)));
            }
            let mut changed = false;
            // Preserve the input namespace even for pure renames/literals.
            // Emitting correlated expressions in the same SELECT as those
            // aliases lets physical source columns capture an outer reference.
            // Closed inputs need a real SELECT boundary. An ordinary
            // alias is insufficient: the upstream unparser can inline its project.
            if matches!(projection.input.as_ref(), LogicalPlan::Projection(_)) {
                let mut free = false;
                projection.input.apply_with_subqueries(|node| {
                    for expr in node.expressions() {
                        expr.apply(|e| {
                            free |= matches!(e, Expr::OuterReferenceColumn(..));
                            Ok(TreeNodeRecursion::Continue)
                        })?;
                    }
                    Ok(TreeNodeRecursion::Continue)
                })?;
                if !free {
                    let name = loop {
                        let name = format!("__w_sql_cte_correlation_{next}");
                        next += 1;
                        if names.insert(name.clone()) {
                            break name;
                        }
                    };
                    projection.input = Arc::new(
                        LogicalPlanBuilder::from(projection.input.as_ref().clone())
                            .alias(name)?
                            .build()?,
                    );
                    changed = true;
                }
            }
            Ok(Transformed::new(
                LogicalPlan::Projection(projection),
                changed,
                TreeNodeRecursion::Continue,
            ))
        })?
        .data)
}
