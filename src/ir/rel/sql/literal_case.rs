//! Remove unreachable branches after SQL specialization without evaluating any
//! expression. Keep the original CASE result type and projection field names.
use super::*;
use datafusion::common::DFSchema;
use datafusion::logical_expr::{ExprSchemable, expr_rewriter::NamePreserver};

fn simplify(expr: Expr, schema: &DFSchema) -> datafusion::error::Result<Transformed<Expr>> {
    let Expr::Case(case) = &expr else { return Ok(Transformed::no(expr)); };
    // Simple CASE compares its operand to each WHEN value; a Boolean WHEN is
    // not itself a condition in that form.
    if case.expr.is_some() { return Ok(Transformed::no(expr)); }
    if let [(condition, value)] = case.when_then_expr.as_slice()
        && let Expr::IsNotNull(tested) = condition.as_ref()
        && tested == value
        && matches!(value.as_ref(), Expr::Column(_) | Expr::AggregateFunction(_))
        && !value.is_volatile()
        && let Some(otherwise) = &case.else_expr
    {
        return Ok(Transformed::yes(datafusion::functions::core::expr_fn::coalesce(vec![
            value.as_ref().clone(), otherwise.as_ref().clone(),
        ]).cast_to(&expr.get_type(schema)?, schema)?));
    }
    let mut arms = Vec::new();
    let mut otherwise = case.else_expr.clone();
    let mut changed = false;
    for (condition, value) in &case.when_then_expr {
        match condition.as_ref() {
            Expr::Literal(ScalarValue::Boolean(Some(false) | None), _)
            | Expr::Literal(ScalarValue::Null, _) => { changed = true; }
            Expr::Literal(ScalarValue::Boolean(Some(true)), _) => {
                otherwise = Some(value.clone());
                changed = true;
                break;
            }
            _ => arms.push((condition.clone(), value.clone())),
        }
    }
    if !changed { return Ok(Transformed::no(expr)); }
    let ty = expr.get_type(schema)?;
    let replacement = if arms.is_empty() {
        match otherwise {
            Some(value) => *value,
            None => lit(ScalarValue::try_from(&ty)?),
        }
    } else {
        Expr::Case(datafusion::logical_expr::expr::Case::new(None, arms, otherwise))
    };
    Ok(Transformed::yes(replacement.cast_to(&ty, schema)?))
}

pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let names = NamePreserver::new(&node);
        let mut schema = DFSchema::empty();
        for input in node.inputs() { schema.merge(input.schema()); }
        schema.merge(node.schema());
        node.map_expressions(|expr| {
            let saved = names.save(&expr);
            expr.transform_up(|expr| simplify(expr, &schema))?
                .map_data(|expr| Ok(saved.restore(expr)))
        })
    })?.data)
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__sql__literal_case__tests.rs"]
mod tests;
