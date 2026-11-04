//! Conservative cleanup of SQL projection scaffolding.
//!
//! Computations may cross a pure rename boundary exactly once. Otherwise only
//! columns and literal values cross projection boundaries. Even an
//! immutable function can fail, and duplicating or dropping it would change
//! evaluation behavior. Cardinality operators and correlated subqueries keep
//! their original scopes.
use super::*;
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::logical_expr::{LogicalPlanBuilder, Projection};

fn simple_value(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Alias(alias) => simple_value(&alias.expr),
        Expr::Column(_) | Expr::Literal(..) => Some(expr.clone()),
        _ => None,
    }
}

/// A pure output rename can absorb computations without duplicating, dropping,
/// or conditionally evaluating them. Keep set-returning, window, and aggregate
/// expressions in place; scalar subqueries retain their complete correlation.
fn rename_values(outer: &Projection, inner: &Projection) -> datafusion::common::Result<Option<Vec<Expr>>> {
    let mut uses = vec![0_usize; inner.expr.len()];
    for expr in &outer.expr {
        match simple_value(expr) {
            Some(Expr::Column(column)) => uses[inner.schema.index_of_column(&column)?] += 1,
            Some(Expr::Literal(..)) => {},
            _ => return Ok(None),
        }
    }
    let mut values = Vec::with_capacity(inner.expr.len());
    for (index, expr) in inner.expr.iter().enumerate() {
        if let Some(value) = simple_value(expr) { values.push(value); continue; }
        if uses[index] != 1 { return Ok(None); }
        let mut scoped = false;
        expr.apply(|expr| {
            if matches!(expr, Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_)
                | Expr::WindowFunction(_) | Expr::AggregateFunction(_) | Expr::Unnest(_)) {
                scoped = true;
                return Ok(TreeNodeRecursion::Stop);
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        if scoped { return Ok(None); }
        let value = match expr { Expr::Alias(alias) => alias.expr.as_ref().clone(), other => other.clone() };
        values.push(value);
    }
    Ok(Some(values))
}

// Compose scalar projects when each computed child is consumed once and is
// proven total. This permits (x + 1) followed by a comparison/CASE without
// duplicating work, but does not inline shared or effectful computations.
fn total_values(outer: &Projection, inner: &Projection) -> datafusion::common::Result<Option<Vec<Expr>>> {
    let mut uses=vec![0usize;inner.expr.len()];
    let mut scoped=false;
    for e in &outer.expr {
        e.apply(|e| {
            match e {
                Expr::Column(c)=>uses[inner.schema.index_of_column(c)?]+=1,
                Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_)
                    | Expr::WindowFunction(_) | Expr::AggregateFunction(_) | Expr::Unnest(_)=>scoped=true,
                _=>{},
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
    }
    if scoped {return Ok(None);}
    let mut values=Vec::new();
    for (i,e) in inner.expr.iter().enumerate() {
        if let Some(e)=simple_value(e) {values.push(e);continue;}
        let e=if let Expr::Alias(a)=e {a.expr.as_ref()} else {e};
        if uses[i]!=1 || e.is_volatile() || !crate::ir::rel::list_comparison::arithmetic_is_total(&inner.input,e) {return Ok(None);}
        let mut unsafe_scope=false;
        e.apply(|e| {
            unsafe_scope |= matches!(e,Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_)
                | Expr::SetComparison(_) | Expr::WindowFunction(_) | Expr::AggregateFunction(_) | Expr::Unnest(_));
            Ok(TreeNodeRecursion::Continue)
        })?;
        if unsafe_scope {return Ok(None);}
        values.push(e.clone());
    }
    Ok(Some(values))
}

fn has_subquery(expr: &Expr) -> datafusion::common::Result<bool> {
    let mut found = false;
    expr.apply(|expr| {
        if matches!(expr, Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_)) {
            found = true;
            return Ok(TreeNodeRecursion::Stop);
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(found)
}

/// Only literal projections over the actual one-row relation establish a
/// discardable Cartesian input. Do not infer this from cardinality estimates,
/// LIMIT, aggregate output, user table names, or arbitrary scalar expressions.
fn singleton_literals(plan: &LogicalPlan) -> Option<Vec<Expr>> {
    match plan {
        LogicalPlan::EmptyRelation(e) if e.produce_one_row && e.schema.fields().is_empty() => Some(vec![]),
        LogicalPlan::SubqueryAlias(alias) => singleton_literals(&alias.input),
        LogicalPlan::Projection(projection) => {
            let input = singleton_literals(&projection.input)?;
            projection.expr.iter().map(|expr| match simple_value(expr)? {
                value @ Expr::Literal(..) => Some(value),
                Expr::Column(column) => input.get(projection.input.schema().index_of_column(&column).ok()?).cloned(),
                _ => None,
            }).collect()
        }
        _ => None,
    }
}

fn remove_singleton_join(join: &datafusion::logical_expr::Join) -> datafusion::common::Result<Option<LogicalPlan>> {
    use datafusion::logical_expr::{JoinConstraint, JoinType};
    if join.join_type != JoinType::Inner || join.join_constraint != JoinConstraint::On
        || !join.on.is_empty() || join.filter.is_some() || join.null_aware {
        return Ok(None);
    }
    let (input, expressions) = if let Some(mut literals) = singleton_literals(&join.left) {
        literals.extend(join.right.schema().columns().into_iter().map(Expr::Column));
        (join.right.clone(), literals)
    } else if let Some(literals) = singleton_literals(&join.right) {
        let mut expressions: Vec<_> = join.left.schema().columns().into_iter().map(Expr::Column).collect();
        expressions.extend(literals);
        (join.left.clone(), expressions)
    } else {
        return Ok(None);
    };
    // Keep the complete join schema, including source qualifiers, column order,
    // typed nulls, and constants that subsequent projections may still read.
    let expressions = expressions.into_iter().zip(join.schema.iter())
        .map(|(expr, (qualifier, field))| expr.alias_qualified(qualifier.cloned(), field.name()))
        .collect();
    Ok(Some(LogicalPlan::Projection(Projection::try_new_with_schema(
        expressions, input, join.schema.clone(),
    )?)))
}

pub(super) fn simplify(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        if let LogicalPlan::Join(join) = &node
            && let Some(replacement) = remove_singleton_join(join)? {
            return Ok(Transformed::yes(replacement));
        }
        // Snapshot identity AND actual contents establish that this compiler
        // seed is exactly one row. A user table with a similar name is not a
        // constant relation. Never remove filters or a zero-row fetch.
        if let LogicalPlan::TableScan(scan) = &node
            && scan.table_name.table().starts_with("__graph_rel_one_row_")
            && scan.filters.is_empty()
            && scan.fetch != Some(0)
        {
            let provider = datafusion::datasource::source_as_provider(&scan.source)?;
            if let Some(snapshot) = provider.as_any()
                .downcast_ref::<crate::ir::rel::temporal_semantics::Snapshot>()
                && snapshot.batches().iter().map(RecordBatch::num_rows).sum::<usize>() == 1
            {
                let batch = snapshot.batches().iter().find(|b| b.num_rows() == 1).unwrap();
                let indices = scan.projection.clone()
                    .unwrap_or_else(|| (0..batch.num_columns()).collect());
                let exprs = indices.iter().zip(scan.projected_schema.iter())
                    .map(|(index, (qualifier, field))| {
                        Ok(lit(ScalarValue::try_from_array(batch.column(*index), 0)?)
                            .alias_qualified(qualifier.cloned(), field.name()))
                    }).collect::<datafusion::common::Result<Vec<_>>>()?;
                let input = Arc::new(LogicalPlanBuilder::empty(true).build()?);
                if exprs.is_empty() {
                    return Ok(Transformed::yes(input.as_ref().clone()));
                }
                return Ok(Transformed::yes(LogicalPlan::Projection(
                    Projection::try_new_with_schema(exprs, input, scan.projected_schema.clone())?)));
            }
        }
        let LogicalPlan::Projection(mut outer) = node else {
            return Ok(Transformed::no(node));
        };
        // Empty-output plans carry an execution/result contract; do not turn
        // their internal seed into an invalid empty SELECT list.
        if outer.expr.is_empty() {
            return Ok(Transformed::no(LogicalPlan::Projection(outer)));
        }
        // A literal-only one-row seed contributes neither values nor effects
        // when every result is independent of its columns. This includes
        // closed scalar subqueries; correlated ones must keep their input.
        if singleton_literals(&outer.input).is_some() {
            let mut independent = true;
            for expr in &outer.expr {
                independent &= expr.column_refs().is_empty();
                expr.apply(|expr| {
                    match expr {
                        Expr::OuterReferenceColumn(..) | Expr::SetComparison(_) => independent = false,
                        Expr::ScalarSubquery(query) => independent &= query.outer_ref_columns.is_empty(),
                        Expr::Exists(query) => independent &= query.subquery.outer_ref_columns.is_empty(),
                        Expr::InSubquery(query) => independent &= query.subquery.outer_ref_columns.is_empty(),
                        _ => {},
                    }
                    Ok(TreeNodeRecursion::Continue)
                })?;
            }
            if independent {
                outer.input = Arc::new(LogicalPlanBuilder::empty(true).build()?);
                return Ok(Transformed::yes(LogicalPlan::Projection(outer)));
            }
        }
        if outer.expr.iter().map(has_subquery).collect::<datafusion::common::Result<Vec<_>>>()?
            .into_iter().any(|found| found)
        {
            return Ok(Transformed::no(LogicalPlan::Projection(outer)));
        }
        let mut changed = false;
        while let LogicalPlan::Projection(inner) = outer.input.as_ref() {
            // A projection directly above a window is an SQL emission boundary.
            // In particular, SPARQL adds a guard literal here so DataFusion's
            // unparser retains the window outputs. Composing away that guard
            // can leave ORDER BY or aggregates referring to a missing ordinal.
            if matches!(inner.input.as_ref(), LogicalPlan::Window(_)) {
                break;
            }
            let values = match inner.expr.iter().map(simple_value).collect::<Option<Vec<_>>>() {
                Some(values) => Some(values),
                None => match rename_values(&outer, inner)? { Some(values)=>Some(values), None=>total_values(&outer,inner)? },
            };
            let Some(values) = values else {
                break;
            };
            let exprs = outer.expr.iter().zip(outer.schema.iter()).map(|(expr, (qualifier, field))| {
                let expr = expr.clone().transform_up(|expr| {
                    if let Expr::Column(column) = &expr {
                        let index = inner.schema.index_of_column(column)?;
                        return Ok(Transformed::yes(values[index].clone()));
                    }
                    Ok(Transformed::no(expr))
                })?.data;
                // Rewriting an expression must not change the name referenced
                // by parent operators, nor retain an obsolete inner alias.
                let expr = match expr { Expr::Alias(a) => *a.expr, other => other };
                Ok(expr.alias_qualified(qualifier.cloned(), field.name()))
            }).collect::<datafusion::common::Result<Vec<_>>>()?;
            outer = Projection::try_new_with_schema(exprs, inner.input.clone(), outer.schema.clone())?;
            changed = true;
            // A scalar subquery may cross this pure rename once with its
            // input and outer-reference metadata intact. Never then compose
            // another lower projection without rebinding its correlations.
            if outer.expr.iter().map(has_subquery).collect::<datafusion::common::Result<Vec<_>>>()?
                .into_iter().any(|found| found) { break; }
        }
        Ok(Transformed::new(LogicalPlan::Projection(outer), changed, TreeNodeRecursion::Continue))
    })?.data)
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__sql__cleanup__tests.rs"]
mod tests;
