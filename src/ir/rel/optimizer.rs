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

/// UNNEST keeps a column name but changes List<T> into T. Upstream's leaf
/// extraction pushdown checks column presence, not that type-changing contract.
/// Keep field extraction above this boundary; children below it still optimize.
#[derive(Debug)]
struct UnnestLeafProjectionGuard {inner:Arc<dyn OptimizerRule+Send+Sync>}
impl OptimizerRule for UnnestLeafProjectionGuard {
    fn name(&self)->&str{self.inner.name()}
    fn supports_rewrite(&self)->bool{true}
    fn apply_order(&self)->Option<datafusion::optimizer::ApplyOrder>{self.inner.apply_order()}
    fn rewrite(&self,plan:LogicalPlan,config:&dyn OptimizerConfig)->Result<Transformed<LogicalPlan>> {
        use datafusion::common::tree_node::{TreeNode,TreeNodeRecursion};
        if let LogicalPlan::Projection(projection)=&plan {
            let mut unnest=false;
            projection.input.apply(|node|{unnest|=matches!(node,LogicalPlan::Unnest(_));Ok(TreeNodeRecursion::Continue)})?;
            if unnest{return Ok(Transformed::no(plan));}
        }
        self.inner.rewrite(plan,config)
    }
}

/// Sessions can outlive the diagnostic scope; consult it at rewrite time.
#[derive(Debug)]
struct ScopedConstantFolding {
    inner: Arc<dyn OptimizerRule + Send + Sync>,
}
impl OptimizerRule for ScopedConstantFolding {
    fn name(&self) -> &str { self.inner.name() }
    fn supports_rewrite(&self) -> bool { true }
    fn apply_order(&self) -> Option<datafusion::optimizer::ApplyOrder> { self.inner.apply_order() }
    fn rewrite(&self, plan: LogicalPlan, config: &dyn OptimizerConfig) -> Result<Transformed<LogicalPlan>> {
        if crate::ir::diagnostics::constant_folding_enabled() && super::conversion_rules::can_fold_plan(&plan) { self.inner.rewrite(plan, config) }
        else { Ok(Transformed::no(plan)) }
    }
}

#[derive(Debug)]
struct CompilerOnlyPlanner;
#[async_trait::async_trait]
impl datafusion::execution::context::QueryPlanner for CompilerOnlyPlanner {
    async fn create_physical_plan(&self,_:&LogicalPlan,_:&datafusion::execution::context::SessionState)->Result<Arc<dyn datafusion::physical_plan::ExecutionPlan>> {
        Err(datafusion::common::DataFusionError::Plan("DataFusion is compiler-only; execute relational plans through DuckDB or Trino".into()))
    }
}

pub(super) fn session(config: SessionConfig) -> SessionContext {
    let session = SessionContext::new_with_config(config);
    let rules: Vec<Arc<dyn OptimizerRule + Send + Sync>> = session
        .state()
        .optimizers()
        .iter()
        .map(|rule| {
            if rule.name() == "simplify_expressions" {
                Arc::new(ScopedConstantFolding { inner: rule.clone() }) as Arc<dyn OptimizerRule + Send + Sync>
            } else if rule.name() == "push_down_leaf_projections" {
                Arc::new(UnnestLeafProjectionGuard{inner:rule.clone()}) as Arc<dyn OptimizerRule+Send+Sync>
            } else if rule.name() == "optimize_projections" {
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
        .with_query_planner(Arc::new(CompilerOnlyPlanner))
        .build();
    SessionContext::new_with_state(state)
}

pub(super) fn simplify_bound_expressions(plan: LogicalPlan, config: &dyn OptimizerConfig) -> Result<LogicalPlan> {
    if !crate::ir::diagnostics::constant_folding_enabled() || !super::conversion_rules::can_fold_plan(&plan) { return Ok(plan); }
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

#[cfg(test)]
mod diagnostic_folding_tests {
    use super::*;
    #[tokio::test]
    async fn existing_session_observes_scoped_folding_mode() {
        use datafusion::{logical_expr::LogicalPlanBuilder, prelude::lit};
        let context = session(SessionConfig::new());
        let plan = LogicalPlanBuilder::empty(true).project(vec![(lit(1_i64)+lit(2_i64)).alias("result")]).unwrap().build().unwrap();
        crate::ir::diagnostics::without_constant_folding(async {
            let optimized = context.state().optimize(&plan).unwrap();
            assert!(optimized.display_indent().to_string().contains("+"));
        }).await;
        assert!(!context.state().optimize(&plan).unwrap().display_indent().to_string().contains("+"));
    }
}

/// Run demand pruning at the SQL boundary too: source/representation lowering
/// exposes columns after the ordinary relational optimization stage. Recursive
/// worktable layouts must be updated together with their consumers.
pub(super) fn prune_sql_outputs(plan: LogicalPlan) -> Result<LogicalPlan> {
    use datafusion::optimizer::{OptimizerContext, optimize_projections::OptimizeProjections};
    use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
    use datafusion::logical_expr::Expr;
    // Late per-consumer pruning cannot change occurrence scopes: window
    // ordinals and volatile values may be shared by several SQL CTE consumers.
    // It also loses ORDER BY dependencies in wrapped aggregates. The earlier
    // relational optimizer still prunes these plans with their full contracts.
    let mut occurrence_sensitive = false;
    plan.apply_with_subqueries(|node| {
        node.apply_expressions(|expr| expr.apply(|expr| {
            occurrence_sensitive |= expr.is_volatile() || matches!(expr, Expr::WindowFunction(_));
            if let Expr::AggregateFunction(a) = expr {
                occurrence_sensitive |= !a.params.order_by.is_empty();
            }
            Ok(TreeNodeRecursion::Continue)
        }))?;
        Ok(TreeNodeRecursion::Continue)
    })?;
    if occurrence_sensitive {
        // Shared occurrence scopes are atomic. Independent siblings can still
        // prune their internal scaffolding without changing any child output
        // contract or creating consumer-specific copies of a shared window.
        if matches!(&plan, LogicalPlan::SubqueryAlias(_) | LogicalPlan::Window(_)
            | LogicalPlan::Aggregate(_) | LogicalPlan::RecursiveQuery(_)) { return Ok(plan); }
        return Ok(plan.map_children(|child| {
            let original=child.clone();
            let pruned=prune_sql_outputs(child)?;
            Ok(datafusion::common::tree_node::Transformed::new(pruned.clone(),pruned!=original,
                TreeNodeRecursion::Continue))
        })?.data);
    }

    Ok(RecursiveProjection { inner: Arc::new(OptimizeProjections::new()) }
        .rewrite(plan, &OptimizerContext::new())?.data)
}

#[cfg(test)]
mod executable_environment_tests {
    #[tokio::test]
    async fn compiler_session_refuses_datafusion_physical_execution() {
        let session=super::session(datafusion::prelude::SessionConfig::new());
        let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let error=session.state().create_physical_plan(&plan).await.unwrap_err();
        assert!(error.to_string().contains("DataFusion is compiler-only"),"{error}");
    }
}
