//! Scope-aware substitution at SQL emission. Outer references must follow the
//! same pure rename composition as ordinary columns, including subquery metadata.
use super::*;
use datafusion::logical_expr::{LogicalPlanBuilder, Projection, Subquery};
type Bindings = BTreeMap<Column, Expr>;
fn value(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Alias(a) => value(&a.expr),
        Expr::Column(_) | Expr::Literal(..) => Some(expr.clone()),
        _ => None,
    }
}
fn lookup<'a>(bindings: &'a Bindings, column: &Column) -> Option<&'a Expr> {
    bindings.get(column).or_else(|| {
        if column.relation.is_some() {
            return None;
        }
        let mut matching = bindings.iter().filter(|(c, _)| c.name == column.name);
        let (_, value) = matching.next()?;
        matching.next().is_none().then_some(value)
    })
}
fn outer(expr: Expr, bindings: &Bindings) -> Expr {
    if let Expr::OuterReferenceColumn(field, column) = &expr
        && let Some(replacement) = lookup(bindings, column)
    {
        return match replacement {
            Expr::Column(c) => Expr::OuterReferenceColumn(
                Arc::new(field.as_ref().clone().with_name(&c.name)),
                c.clone(),
            ),
            Expr::Literal(..) => replacement.clone(),
            _ => expr,
        };
    }
    expr
}
fn subquery(mut query: Subquery, bindings: &Bindings) -> datafusion::common::Result<Subquery> {
    query.outer_ref_columns = query
        .outer_ref_columns
        .into_iter()
        .filter_map(|e| {
            let e = outer(e, bindings);
            (!matches!(e, Expr::Literal(..))).then_some(e)
        })
        .collect();
    query.subquery = Arc::new(
        query
            .subquery
            .as_ref()
            .clone()
            .transform_up(|node| {
                let mut nested = bindings.clone();
                // A nested subquery's immediate parent binds its own input columns;
                // those names shadow correlations originating outside this scope.
                for input in node.inputs() {
                    for c in input.schema().columns() {
                        nested.retain(|key, _| {
                            !(key == &c || (key.relation.is_none() && key.name == c.name))
                        });
                    }
                }
                node.map_expressions(|expr| {
                    expr.transform_up(|expr| {
                        let expr = outer(expr, bindings);
                        let expr = rebind_query(expr, &nested)?;
                        Ok(Transformed::yes(expr))
                    })
                })
            })?
            .data,
    );
    Ok(query)
}
fn rebind_query(expr: Expr, bindings: &Bindings) -> datafusion::common::Result<Expr> {
    Ok(match expr {
        Expr::ScalarSubquery(s) => Expr::ScalarSubquery(subquery(s, bindings)?),
        Expr::Exists(mut s) => {
            s.subquery = subquery(s.subquery, bindings)?;
            Expr::Exists(s)
        }
        Expr::InSubquery(mut s) => {
            s.subquery = subquery(s.subquery, bindings)?;
            Expr::InSubquery(s)
        }
        Expr::SetComparison(mut s) => {
            s.subquery = subquery(s.subquery, bindings)?;
            Expr::SetComparison(s)
        }
        e => e,
    })
}
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
            while let LogicalPlan::Projection(input) = projection.input.as_ref() {
                let Some(values) = input.expr.iter().map(value).collect::<Option<Vec<_>>>() else {
                    break;
                };
                let bindings = input
                    .schema
                    .columns()
                    .into_iter()
                    .zip(values)
                    .collect::<Bindings>();
                let exprs = projection
                    .expr
                    .into_iter()
                    .map(|expr| {
                        expr.transform_up(|expr| {
                            let expr = if let Expr::Column(c) = &expr {
                                lookup(&bindings, c).cloned().unwrap_or(expr)
                            } else {
                                expr
                            };
                            Ok(Transformed::yes(rebind_query(expr, &bindings)?))
                        })
                        .map(|e| e.data)
                    })
                    .collect::<datafusion::common::Result<Vec<_>>>()?;
                projection = Projection::try_new_with_schema(
                    exprs,
                    input.input.clone(),
                    projection.schema.clone(),
                )?;
                changed = true;
            }
            // Nontrivial closed inputs need a real SELECT boundary. An ordinary
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

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(name: &str) -> Expr {
        Expr::OuterReferenceColumn(
            Arc::new(arrow::datatypes::Field::new(name, DataType::Int64, true)),
            Column::new_unqualified(name),
        )
    }
    #[test]
    fn nested_local_bindings_shadow_an_outer_rename() {
        let nested = Subquery {
            subquery: Arc::new(
                LogicalPlanBuilder::empty(true)
                    .project(vec![reference("x").alias("nested")])
                    .unwrap()
                    .build()
                    .unwrap(),
            ),
            outer_ref_columns: vec![reference("x")],
            spans: Default::default(),
        };
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![lit(7_i64).alias("x")])
            .unwrap()
            .project(vec![
                reference("x").alias("outside"),
                Expr::ScalarSubquery(nested).alias("inside"),
            ])
            .unwrap()
            .build()
            .unwrap();
        let query = Subquery {
            subquery: Arc::new(plan),
            outer_ref_columns: vec![reference("x")],
            spans: Default::default(),
        };
        let bindings = [(
            Column::new_unqualified("x"),
            Expr::Column(Column::new_unqualified("y")),
        )]
        .into_iter()
        .collect();
        let rewritten = subquery(query, &bindings).unwrap();
        let text = rewritten.subquery.display_indent().to_string();
        assert!(text.contains("outer_ref(y) AS outside"), "{text}");
        assert!(text.contains("outer_ref(x) AS nested"), "{text}");
        assert_eq!(rewritten.outer_ref_columns, vec![reference("y")]);
    }
}
