//! Protect the positional layout shared by a recursive query and its work table.
use std::sync::Arc;

use datafusion::common::tree_node::Transformed;
use datafusion::error::Result;
use datafusion::execution::session_state::SessionStateBuilder;
use datafusion::logical_expr::LogicalPlan;
use datafusion::optimizer::{OptimizerConfig, OptimizerRule};
use datafusion::prelude::{SessionConfig, SessionContext};

mod recursive_projection;
use recursive_projection::RecursiveProjection;

/// Upstream's DISTINCT rule treats some fan-out FDs as row uniqueness.
/// Use the stronger proof model for plans carrying supplied constraints.
#[derive(Debug)]
struct ConstraintDistinctGuard {
    inner: Arc<dyn OptimizerRule + Send + Sync>,
}
impl OptimizerRule for ConstraintDistinctGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn supports_rewrite(&self) -> bool {
        true
    }
    fn apply_order(&self) -> Option<datafusion::optimizer::ApplyOrder> {
        self.inner.apply_order()
    }
    fn rewrite(
        &self,
        plan: LogicalPlan,
        config: &dyn OptimizerConfig,
    ) -> Result<Transformed<LogicalPlan>> {
        if let LogicalPlan::Distinct(datafusion::logical_expr::Distinct::All(input)) = &plan {
            let p = super::constraints::analyze(input);
            if !p.evidence.is_empty() && !input.schema().fields().is_empty() {
                if p.unique_on(
                    &(0..input.schema().fields().len()).collect::<Vec<_>>(),
                    true,
                ) {
                    return Ok(Transformed::yes(input.as_ref().clone()));
                }
                let expr = input
                    .schema()
                    .columns()
                    .into_iter()
                    .map(datafusion::logical_expr::Expr::Column)
                    .collect::<Vec<_>>();
                let aggregate =
                    datafusion::logical_expr::LogicalPlanBuilder::from(input.as_ref().clone())
                        .aggregate(expr, Vec::<datafusion::logical_expr::Expr>::new())?
                        .build()?;
                return Ok(Transformed::yes(aggregate));
            }
        }
        self.inner.rewrite(plan, config)
    }
}

pub(super) fn session(config: SessionConfig) -> SessionContext {
    let session = SessionContext::new_with_config(config);
    let rules: Vec<Arc<dyn OptimizerRule + Send + Sync>> = session
        .state()
        .optimizers()
        .iter()
        .map(|rule| {
            if rule.name() == "optimize_projections" {
                Arc::new(RecursiveProjection {
                    inner: rule.clone(),
                }) as Arc<dyn OptimizerRule + Send + Sync>
            } else if rule.name() == "replace_distinct_aggregate" {
                Arc::new(ConstraintDistinctGuard {
                    inner: rule.clone(),
                }) as Arc<dyn OptimizerRule + Send + Sync>
            } else {
                rule.clone()
            }
        })
        .collect();
    let state = SessionStateBuilder::new_from_existing(session.state())
        .with_optimizer_rules(rules)
        .build();
    SessionContext::new_with_state(state)
}

pub(super) fn simplify_bound_expressions(plan: LogicalPlan, config: &dyn OptimizerConfig) -> Result<LogicalPlan> {
    use datafusion::common::tree_node::TreeNode;
    use datafusion::logical_expr::Expr;
    use datafusion::optimizer::{simplify_expressions::SimplifyExpressions, utils::NamePreserver};
    fn literal(expr: &Expr) -> Option<Expr> {
        match expr {
            Expr::Literal(..) => Some(expr.clone()),
            Expr::Alias(alias) => literal(&alias.expr),
            _ => None,
        }
    }
    fn constants(plan: &LogicalPlan) -> Vec<Option<Expr>> {
        match plan {
            LogicalPlan::Projection(projection) => projection.expr.iter().map(literal).collect(),
            LogicalPlan::SubqueryAlias(alias) => constants(&alias.input),
            _ => vec![None; plan.schema().fields().len()],
        }
    }
    Ok(plan.transform_up_with_subqueries(|node| {
        if !matches!(node, LogicalPlan::Projection(_) | LogicalPlan::Filter(_)) {
            return Ok(Transformed::no(node));
        }
        let input = node.inputs()[0];
        let values = input.schema().columns().into_iter().zip(constants(input))
            .filter_map(|(column, value)| value.map(|value| (column, value)))
            .collect::<std::collections::HashMap<_, _>>();
        let names = NamePreserver::new(&node);
        let node = node.map_expressions(|expr| {
            let name = names.save(&expr);
            expr.transform_up(|expr| {
                if let Expr::Column(column) = &expr {
                    if let Some(value) = values.get(column) {
                        return Ok(Transformed::yes(value.clone()));
                    }
                }
                Ok(Transformed::no(expr))
            })?.map_data(|expr| Ok(name.restore(expr)))
        })?;
        node.transform_data(|node| SimplifyExpressions::new().rewrite(node, config))
    })?.data)
}
