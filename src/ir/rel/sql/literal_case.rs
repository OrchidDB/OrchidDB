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
mod tests {
    use super::*;
    use datafusion::logical_expr::expr::Case;
    fn case(conditions: Vec<(Expr, Expr)>, otherwise: Option<Expr>) -> Expr {
        Expr::Case(Case::new(None, conditions.into_iter().map(|(a,b)| (Box::new(a),Box::new(b))).collect(), otherwise.map(Box::new)))
    }
    #[test]
    fn false_and_null_conditions_do_not_select_a_branch() {
        let value = case(vec![(lit(false),lit(1_i64)), (lit(ScalarValue::Boolean(None)),lit(2_i64))], Some(lit(3_i64)));
        assert_eq!(simplify(value, &DFSchema::empty()).unwrap().data, lit(3_i64));
        let value = case(vec![(lit(false),lit(1_i64))],None);
        assert_eq!(simplify(value, &DFSchema::empty()).unwrap().data,lit(ScalarValue::Int64(None)));
    }
    #[test]
    fn true_condition_discards_only_later_branches() {
        let condition = lit(1_i64).eq(lit(2_i64));
        let value = case(vec![(condition.clone(),lit(1_i64)),(lit(true),lit(2_i64)),(lit(true),lit(3_i64))],Some(lit(4_i64)));
        assert_eq!(simplify(value,&DFSchema::empty()).unwrap().data,case(vec![(condition,lit(1_i64))],Some(lit(2_i64))));
    }
    #[test]
    fn simple_case_boolean_values_are_not_conditions() {
        let value = Expr::Case(Case::new(Some(Box::new(lit(false))),vec![(Box::new(lit(false)),Box::new(lit(1_i64)))],Some(Box::new(lit(2_i64)))));
        assert_eq!(simplify(value.clone(),&DFSchema::empty()).unwrap().data,value);
    }
}
