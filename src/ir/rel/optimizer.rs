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
