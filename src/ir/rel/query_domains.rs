//! Finite physical discriminator domains. Unknown is never an empty domain.
use super::*;

pub(super) fn strings(plan: &LogicalPlan, name: &str) -> Option<BTreeSet<String>> {
    let index = plan
        .schema()
        .index_of_column(&Column::from_name(name))
        .ok()?;
    column(plan, index)
}
fn expression(expr: &Expr, input: &LogicalPlan) -> Option<BTreeSet<String>> {
    match expr {
        Expr::Alias(a) => expression(&a.expr, input),
        Expr::Literal(ScalarValue::Utf8(Some(s)), _) => Some(BTreeSet::from([s.clone()])),
        Expr::Literal(value, _) if value.is_null() => Some(BTreeSet::new()),
        Expr::Column(c) => column(input, input.schema().index_of_column(c).ok()?),
        _ => None,
    }
}
fn column(plan: &LogicalPlan, index: usize) -> Option<BTreeSet<String>> {
    match plan {
        LogicalPlan::Projection(p) => expression(p.expr.get(index)?, &p.input),
        LogicalPlan::Filter(p) => column(&p.input, index),
        LogicalPlan::SubqueryAlias(p) => column(&p.input, index),
        LogicalPlan::Sort(p) => column(&p.input, index),
        LogicalPlan::Limit(p) => column(&p.input, index),
        LogicalPlan::Union(p) => {
            let mut values = BTreeSet::new();
            for input in &p.inputs {
                values.extend(column(input, index)?);
            }
            Some(values)
        }
        LogicalPlan::Join(p)
            if matches!(
                p.join_type,
                JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full
            ) =>
        {
            let left = p.left.schema().fields().len();
            if index < left {
                column(&p.left, index)
            } else {
                column(&p.right, index - left)
            }
        }
        LogicalPlan::EmptyRelation(p) if !p.produce_one_row => Some(BTreeSet::new()),
        _ => None,
    }
}
