//! Cost-based choices over existing legal relational alternatives.
use super::estimate::{filter_probability, input_rows};
use datafusion::{
    common::{
        Result,
        tree_node::{Transformed, TreeNode},
    },
    logical_expr::{Expr, LogicalPlan, Operator},
};
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct OptimizerDecision {
    pub optimization: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub estimated_work_before: f64,
    pub estimated_work_after: f64,
    pub reason: String,
}
fn conjuncts(e: &Expr, out: &mut Vec<Expr>) {
    if let Expr::BinaryExpr(b) = e {
        if b.op == Operator::And {
            conjuncts(&b.left, out);
            conjuncts(&b.right, out);
            return;
        }
    }
    out.push(e.clone());
}
fn simple(e: &Expr) -> bool {
    match e {
        Expr::Column(_) | Expr::Literal(_, _) => true,
        _ => false,
    }
}
fn transparent(e: &Expr) -> Expr {
    if let Expr::Case(c) = e {
        if c.expr.is_none() {
            for (when, then) in &c.when_then_expr {
                match when.as_ref() {
                    Expr::Literal(datafusion::common::ScalarValue::Boolean(Some(true)), _) => {
                        return transparent(then);
                    }
                    Expr::Literal(
                        datafusion::common::ScalarValue::Boolean(Some(false) | None),
                        _,
                    ) => (),
                    _ => return e.clone(),
                }
            }
            if let Some(other) = &c.else_expr {
                return transparent(other);
            }
        }
    }
    e.clone()
}
fn total(e: &Expr) -> bool {
    match e {
        Expr::BinaryExpr(b) => {
            matches!(
                b.op,
                Operator::Eq
                    | Operator::NotEq
                    | Operator::Lt
                    | Operator::LtEq
                    | Operator::Gt
                    | Operator::GtEq
            ) && simple(&b.left)
                && simple(&b.right)
        }
        Expr::IsNull(e) | Expr::IsNotNull(e) => simple(e),
        Expr::IsTrue(e) => total(e),
        Expr::Literal(datafusion::common::ScalarValue::Boolean(_), _) => true,
        Expr::InList(l) => simple(&l.expr) && l.list.iter().all(simple),
        _ => false,
    }
}
fn unit_cost(e: &Expr) -> f64 {
    if let Expr::InList(l) = e {
        l.list.len().max(1) as f64
    } else {
        1.0
    }
}
/// Reorder total, side-effect-free conjuncts by expected comparisons per surviving row.
/// Leave casts, UDFs and error-producing expressions in their original order.
pub fn optimize(plan: LogicalPlan) -> Result<(LogicalPlan, Vec<OptimizerDecision>)> {
    if super::explain(&plan).is_empty() {
        return Ok((plan, vec![]));
    }
    // Expose frontend CASE guards and filter-over-cross-join patterns before costing.
    let plan = super::super::layout::push_filters(plan)?;
    let (plan, mut decisions) = super::access::restrict_collections(plan)?;
    let (plan, joins) = super::access::optimize(plan)?;
    decisions.extend(joins);
    let plan = plan
        .transform_up_with_subqueries(|node| {
            let LogicalPlan::Filter(mut f) = node else {
                return Ok(Transformed::no(node));
            };
            let mut expressions = Vec::new();
            conjuncts(&transparent(&f.predicate), &mut expressions);
            let mut input = f.input.clone();
            // Language lowering often represents conjunctions as consecutive
            // filters. Combine only total predicates, preserving every barrier.
            while let LogicalPlan::Filter(inner) = input.as_ref() {
                let mut prefix = Vec::new();
                conjuncts(&transparent(&inner.predicate), &mut prefix);
                if prefix.iter().any(|e| !total(e)) || expressions.iter().any(|e| !total(e)) {
                    break;
                }
                prefix.extend(expressions);
                expressions = prefix;
                input = inner.input.clone();
            }
            if expressions.len() < 2 || expressions.iter().any(|e| !total(e)) {
                return Ok(Transformed::no(LogicalPlan::Filter(f)));
            }
            let Some(rows) = input_rows(&input) else {
                return Ok(Transformed::no(LogicalPlan::Filter(f)));
            };
            let Some(mut costs) = expressions
                .iter()
                .enumerate()
                .map(|(i, e)| filter_probability(&input, e).map(|p| (i, p, unit_cost(e))))
                .collect::<Option<Vec<_>>>()
            else {
                return Ok(Transformed::no(LogicalPlan::Filter(f)));
            };
            let work = |costs: &[(usize, f64, f64)]| {
                let mut survivors = rows;
                let mut sum = 0.0;
                for (_, p, cost) in costs {
                    sum += survivors * cost;
                    survivors *= p;
                }
                sum
            };
            let before = work(&costs);
            costs.sort_by(|a, b| {
                ((1.0 - b.1) / b.2)
                    .total_cmp(&((1.0 - a.1) / a.2))
                    .then(a.0.cmp(&b.0))
            });
            let after = work(&costs);
            if after >= before || before - after < 0.01 {
                return Ok(Transformed::no(LogicalPlan::Filter(f)));
            }
            let sorted = costs
                .iter()
                .map(|(i, _, _)| expressions[*i].clone())
                .collect::<Vec<_>>();
            decisions.push(OptimizerDecision {
                optimization: "order_pure_filters".into(),
                before: expressions.iter().map(ToString::to_string).collect(),
                after: sorted.iter().map(ToString::to_string).collect(),
                estimated_work_before: before,
                estimated_work_after: after,
                reason:
                    "collected predicate selectivity and comparison count; total expressions only"
                        .into(),
            });
            f.input = input;
            f.predicate = sorted.into_iter().reduce(|a, b| a.and(b)).unwrap();
            Ok(Transformed::yes(LogicalPlan::Filter(f)))
        })?
        .data;
    Ok((plan, decisions))
}
