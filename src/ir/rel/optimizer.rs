//! Protect the positional layout shared by a recursive query and its work table.
use std::sync::Arc;

use datafusion::common::tree_node::{Transformed, TreeNode, TreeNodeRecursion};
use datafusion::error::Result;
use datafusion::execution::session_state::SessionStateBuilder;
use datafusion::logical_expr::LogicalPlan;
use datafusion::optimizer::{OptimizerConfig, OptimizerRule};
use datafusion::prelude::{SessionConfig, SessionContext};

#[derive(Debug)]
struct RecursiveProjectionGuard {
    inner: Arc<dyn OptimizerRule + Send + Sync>,
}

impl OptimizerRule for RecursiveProjectionGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn supports_rewrite(&self) -> bool {
        true
    }

    fn rewrite(
        &self,
        plan: LogicalPlan,
        config: &dyn OptimizerConfig,
    ) -> Result<Transformed<LogicalPlan>> {
        let mut recursive = false;
        plan.apply_with_subqueries(|node| {
            if matches!(node, LogicalPlan::RecursiveQuery(_)) {
                recursive = true;
                Ok(TreeNodeRecursion::Stop)
            } else {
                Ok(TreeNodeRecursion::Continue)
            }
        })?;
        if recursive {
            // DataFusion 53 prunes the recursive terms without remapping the
            // CteWorkTable's original schema/projection indices. The next
            // iteration then reads the wrong columns (or an out-of-range index).
            // Keep this one rule off recursive plans until those layouts are
            // updated together. All other rules and non-recursive plans retain
            // their usual optimization.
            Ok(Transformed::no(plan))
        } else {
            self.inner.rewrite(plan, config)
        }
    }
}

pub(super) fn session(config: SessionConfig) -> SessionContext {
    let session = SessionContext::new_with_config(config);
    let rules = session
        .state()
        .optimizers()
        .iter()
        .map(|rule| {
            if rule.name() == "optimize_projections" {
                Arc::new(RecursiveProjectionGuard {
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
