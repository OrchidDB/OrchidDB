//! Structural empty-input rewrites, independent of constant evaluation.
use super::*;
use datafusion::common::tree_node::Transformed;
use datafusion::logical_expr::{EmptyRelation, Projection};

fn empty(plan: &LogicalPlan) -> bool {
    matches!(plan, LogicalPlan::EmptyRelation(e) if !e.produce_one_row)
}
fn discardable(plan: &LogicalPlan) -> bool {
    fn simple(expr: &Expr) -> bool {
        match expr {
            Expr::Column(_) | Expr::Literal(..) => true,
            Expr::Alias(a) => simple(&a.expr),
            _ => false,
        }
    }
    match plan {
        LogicalPlan::EmptyRelation(_) | LogicalPlan::Values(_) => true,
        LogicalPlan::TableScan(s) => s.filters.is_empty(),
        LogicalPlan::Projection(p) => p.expr.iter().all(simple) && discardable(&p.input),
        LogicalPlan::SubqueryAlias(p) => discardable(&p.input),
        _ => false,
    }
}
pub(super) fn simplify(plan: LogicalPlan) -> RelResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let erase = match &node {
            LogicalPlan::Projection(p) => empty(&p.input),
            LogicalPlan::SubqueryAlias(p) => empty(&p.input),
            LogicalPlan::Filter(p) => empty(&p.input),
            LogicalPlan::Join(j) if j.join_type == JoinType::Inner =>
                (empty(&j.left) && discardable(&j.right)) || (empty(&j.right) && discardable(&j.left)),
            _ => false,
        };
        if erase {
            return Ok(Transformed::yes(LogicalPlan::EmptyRelation(EmptyRelation {
                produce_one_row: false, schema: node.schema().clone(),
            })));
        }
        if let LogicalPlan::Join(j) = &node {
            let preserved = match j.join_type {
                JoinType::Left if empty(&j.right) => Some((&j.left, true)),
                JoinType::Right if empty(&j.left) => Some((&j.right, false)),
                _ => None,
            };
            if let Some((input, left)) = preserved {
                let mut expressions = Vec::new();
                for (index, (qualifier, field)) in j.schema.iter().enumerate() {
                    let retained = if left { index < j.left.schema().fields().len() }
                        else { index >= j.left.schema().fields().len() };
                    let expr = if retained {
                        Expr::Column(Column::new(qualifier.cloned(), field.name()))
                    } else { lit(ScalarValue::try_from(field.data_type())?) };
                    expressions.push(expr.alias_qualified(qualifier.cloned(), field.name()));
                }
                return Ok(Transformed::yes(LogicalPlan::Projection(Projection::try_new_with_schema(
                    expressions, input.clone(), j.schema.clone(),
                )?)));
            }
        }
        Ok(Transformed::no(node))
    })?.data)
}
