//! Discard join keys proven equal by literal projections. In particular,
//! fixed-label mappings need no runtime string comparison on every edge.
//! Null-extended sides of outer joins do not establish constant outputs.
use super::*;
use datafusion::logical_expr::JoinType;

fn equal_constants(a: &ScalarValue, b: &ScalarValue) -> bool {
    // Composite nulls and floating-point NaNs have dialect-specific equality.
    !a.is_null() && a == b && matches!(a,
        ScalarValue::Utf8(_) | ScalarValue::LargeUtf8(_) | ScalarValue::Utf8View(_)
        | ScalarValue::Boolean(_) | ScalarValue::Int8(_) | ScalarValue::Int16(_)
        | ScalarValue::Int32(_) | ScalarValue::Int64(_))
}

fn value(expr: &Expr, input: &LogicalPlan, constants: &[Option<ScalarValue>]) -> Option<ScalarValue> {
    match expr {
        Expr::Alias(a) => value(&a.expr, input, constants),
        Expr::Literal(v, _) => Some(v.clone()),
        Expr::Column(c) => constants.get(input.schema().index_of_column(c).ok()?)?.clone(),
        _ => None,
    }
}

fn constants(plan: &LogicalPlan) -> Vec<Option<ScalarValue>> {
    let unknown = || vec![None; plan.schema().fields().len()];
    match plan {
        LogicalPlan::Projection(p) => {
            let input = constants(&p.input);
            p.expr.iter().map(|e| value(e, &p.input, &input)).collect()
        }
        LogicalPlan::SubqueryAlias(p) => constants(&p.input),
        LogicalPlan::Filter(p) => constants(&p.input),
        LogicalPlan::Sort(p) => constants(&p.input),
        LogicalPlan::Limit(p) => constants(&p.input),
        LogicalPlan::Join(j) => {
            let left = || constants(&j.left);
            let right = || constants(&j.right);
            match j.join_type {
                JoinType::Inner => left().into_iter().chain(right()).collect(),
                JoinType::Left => left().into_iter().chain(vec![None; j.right.schema().fields().len()]).collect(),
                JoinType::Right => vec![None; j.left.schema().fields().len()].into_iter().chain(right()).collect(),
                JoinType::LeftSemi | JoinType::LeftAnti => left(),
                JoinType::RightSemi | JoinType::RightAnti => right(),
                _ => unknown(),
            }
        }
        _ => unknown(),
    }
}

pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Join(mut join) = node else { return Ok(Transformed::no(node)); };
        let left = constants(&join.left);
        let right = constants(&join.right);
        let old_len = join.on.len();
        join.on.retain(|(a, b)| {
            match (value(a, &join.left, &left), value(b, &join.right, &right)) {
                (Some(a), Some(b)) => !equal_constants(&a, &b),
                _ => true,
            }
        });
        let mut changed = old_len != join.on.len();
        if let Some(filter) = join.filter.take() {
            let lookup = |expr: &Expr| match expr {
                Expr::Literal(v, _) => Some(v.clone()),
                Expr::Column(c) => match (join.left.schema().index_of_column(c), join.right.schema().index_of_column(c)) {
                    (Ok(i), Err(_)) => left[i].clone(),
                    (Err(_), Ok(i)) => right[i].clone(),
                    (Ok(i), Ok(j)) if left[i] == right[j] => left[i].clone(),
                    _ => None,
                },
                _ => None,
            };
            let simplified = filter.transform_up(|expr| {
                if let Expr::BinaryExpr(b) = &expr {
                    if b.op == datafusion::logical_expr::Operator::Eq
                        && let (Some(a), Some(b)) = (lookup(&b.left), lookup(&b.right))
                        && equal_constants(&a, &b) {
                        return Ok(Transformed::yes(lit(true)));
                    }
                    if b.op == datafusion::logical_expr::Operator::And {
                        if b.left.as_ref() == &lit(true) { return Ok(Transformed::yes(*b.right.clone())); }
                        if b.right.as_ref() == &lit(true) { return Ok(Transformed::yes(*b.left.clone())); }
                    }
                }
                Ok(Transformed::no(expr))
            })?;
            changed |= simplified.transformed;
            join.filter = if simplified.data == lit(true) { None } else { Some(simplified.data) };
        }
        Ok(Transformed::new(LogicalPlan::Join(join), changed, TreeNodeRecursion::Continue))
    })?.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::logical_expr::LogicalPlanBuilder;
    use datafusion::prelude::col;

    fn input(name: &str, value: ScalarValue) -> LogicalPlan {
        LogicalPlanBuilder::empty(true).project(vec![lit(value).alias("label")]).unwrap()
            .alias(name).unwrap().build().unwrap()
    }
    fn joined(a: ScalarValue, b: ScalarValue, kind: JoinType) -> LogicalPlan {
        LogicalPlanBuilder::from(input("a", a)).join(input("b", b), kind,
            (vec!["a.label"], vec!["b.label"]), None).unwrap().build().unwrap()
    }
    #[test]
    fn only_equal_non_null_constants_remove_keys() {
        for kind in [JoinType::Inner, JoinType::Left, JoinType::Full, JoinType::LeftSemi, JoinType::LeftAnti] {
            for (a,b,expected) in [
                (ScalarValue::from("Person"), ScalarValue::from("Person"), 0),
                (ScalarValue::from("Person"), ScalarValue::from("Other"), 1),
                (ScalarValue::Utf8(None), ScalarValue::Utf8(None), 1),
            ] {
                let plan = joined(a,b,kind);
                let rewritten = rewrite(plan.clone()).unwrap();
                assert_eq!(plan.schema(), rewritten.schema());
                let LogicalPlan::Join(j) = rewritten else { panic!() };
                assert_eq!(j.on.len(), expected);
            }
        }
    }
    #[test]
    fn outer_join_null_extension_does_not_establish_constants() {
        let text = ScalarValue::from("Person");
        assert_eq!(constants(&joined(text.clone(), text.clone(), JoinType::Left)), vec![Some(text.clone()), None]);
        assert_eq!(constants(&joined(text.clone(), text.clone(), JoinType::Right)), vec![None, Some(text.clone())]);
        assert_eq!(constants(&joined(text.clone(), text, JoinType::Full)), vec![None, None]);
    }

    #[test]
    fn residual_label_equality_preserves_other_predicates() {
        let text = ScalarValue::from("Person");
        let LogicalPlan::Join(mut join) = joined(text.clone(), text, JoinType::Left) else { panic!() };
        join.on.clear();
        let residual = col("a.label").is_not_null();
        join.filter = Some(col("a.label").eq(col("b.label")).and(residual.clone()));
        let LogicalPlan::Join(rewritten) = rewrite(LogicalPlan::Join(join)).unwrap() else { panic!() };
        assert_eq!(rewritten.filter, Some(residual));
    }
}
