//! Protect the positional layout shared by a recursive query and its work table.
use std::sync::Arc;

use datafusion::common::tree_node::Transformed;
use datafusion::error::Result;
use datafusion::logical_expr::LogicalPlan;
use datafusion::optimizer::{OptimizerConfig, OptimizerRule};

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

/// Logical compiler resources only: no session runtime, catalog, physical
/// planner, object store, task executor, or memory pool is constructed here.
pub(super) struct CompilerSession {
    analyzer: datafusion::optimizer::analyzer::Analyzer,
    optimizer: datafusion::optimizer::Optimizer,
    options: Arc<datafusion::common::config::ConfigOptions>,
    registry: datafusion::logical_expr::registry::MemoryFunctionRegistry,
}
impl CompilerSession {
    pub(super) fn without_rule(mut self, name: &str) -> Self {
        self.optimizer.rules.retain(|rule| rule.name() != name);
        self
    }
    pub(super) fn state(&self) -> CompilerQuery<'_> {
        CompilerQuery { session: self, context: datafusion::optimizer::OptimizerContext::new_with_config_options(self.options.clone()) }
    }
}
pub(super) struct CompilerQuery<'a> {
    session: &'a CompilerSession,
    context: datafusion::optimizer::OptimizerContext,
}
impl CompilerQuery<'_> {
    pub(super) fn analyzer(&self) -> &datafusion::optimizer::analyzer::Analyzer { &self.session.analyzer }
    pub(super) fn config_options(&self) -> &datafusion::common::config::ConfigOptions { &self.session.options }
    pub(super) fn optimize(&self, plan: &LogicalPlan) -> Result<LogicalPlan> {
        let analyzed = self.analyzer().execute_and_check(plan.clone(), self.config_options(), |_, _| {})?;
        self.session.optimizer.optimize(analyzed, self, |_, _| {})
    }
}
impl OptimizerConfig for CompilerQuery<'_> {
    fn query_execution_start_time(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.context.query_execution_start_time() }
    fn alias_generator(&self) -> &Arc<datafusion::common::alias::AliasGenerator> { self.context.alias_generator() }
    fn options(&self) -> Arc<datafusion::common::config::ConfigOptions> { self.context.options() }
    fn function_registry(&self) -> Option<&dyn datafusion::logical_expr::registry::FunctionRegistry> { Some(&self.session.registry) }
}
pub(crate) fn projection_rule() -> Arc<dyn OptimizerRule + Send + Sync> {
    Arc::new(RecursiveProjection {
        inner: Arc::new(datafusion::optimizer::optimize_projections::OptimizeProjections::new()),
    })
}

pub(super) fn session() -> CompilerSession {
    let optimizer = datafusion::optimizer::Optimizer::new();
    let rules: Vec<Arc<dyn OptimizerRule + Send + Sync>> = optimizer.rules
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
    let mut registry = datafusion::logical_expr::registry::MemoryFunctionRegistry::new();
    datafusion::functions::register_all(&mut registry).expect("builtin scalar registry");
    datafusion::functions_nested::register_all(&mut registry).expect("builtin nested registry");
    datafusion::functions_aggregate::register_all(&mut registry).expect("builtin aggregate registry");
    datafusion::functions_window::register_all(&mut registry).expect("builtin window registry");
    let mut options = datafusion::common::config::ConfigOptions::default();
    options.execution.target_partitions = 1;
    CompilerSession {
        analyzer: datafusion::optimizer::analyzer::Analyzer::new(),
        optimizer: datafusion::optimizer::Optimizer::with_rules(rules),
        options: Arc::new(options),
        registry,
    }
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
        let context = session();
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
        for expression in node.expressions() {
            // Lambda markers are marked volatile to prohibit compiler-time
            // invocation: their implementation belongs to SQL. That marker
            // alone does not make a pure lambda an occurrence-sensitive value.
            // This is analysis only; the emitted expression is never replaced.
            let analyzed = expression.transform_up(|expr| {
                if let Expr::ScalarFunction(call) = &expr
                    && let Some(lambda) = super::collection_lambda::definition(&call.func)
                    && lambda.total_body
                    // A list-validation helper can throw even with a total
                    // body. Only transparent source/capture operands qualify.
                    && call.args.iter().all(|arg| matches!(arg, Expr::Column(_) | Expr::Literal(..))) {
                    return Ok(Transformed::yes(Expr::Literal(
                        datafusion::common::ScalarValue::try_from(&lambda.result)?, None)));
                }
                Ok(Transformed::no(expr))
            })?.data;
            occurrence_sensitive |= analyzed.is_volatile();
        }
        node.apply_expressions(|expr| expr.apply(|expr| {
            occurrence_sensitive |= matches!(expr, Expr::WindowFunction(_));
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
mod compiler_context_tests {
    #[test]
    fn query_clock_is_fixed_without_an_execution_environment() {
        use datafusion::optimizer::OptimizerConfig;
        let compiler = super::session();
        let query = compiler.state();
        let start = query.query_execution_start_time();
        assert!(start.is_some());
        assert_eq!(start, query.query_execution_start_time());
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        assert!(matches!(query.optimize(&plan).unwrap(), datafusion::logical_expr::LogicalPlan::EmptyRelation(_)));
    }
}
