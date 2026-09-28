//! Prune recursive outputs and their positional worktable as one operation.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use datafusion::{
    common::{
        DFSchemaRef,
        tree_node::{Transformed, TreeNode, TreeNodeRecursion},
    },
    datasource::{cte_worktable::CteWorkTable, provider_as_source, source_as_provider},
    error::Result,
    logical_expr::{
        Expr, Extension, LogicalPlan, LogicalPlanBuilder, RecursiveQuery, TableScan,
        UserDefinedLogicalNodeCore,
    },
    optimizer::{OptimizerConfig, OptimizerRule},
};

#[derive(Debug)]
pub(super) struct RecursiveProjection {
    pub(super) inner: Arc<dyn OptimizerRule + Send + Sync>,
}

fn project(plan: LogicalPlan, indices: &[usize]) -> Result<LogicalPlan> {
    let columns = plan.schema().columns();
    LogicalPlanBuilder::from(plan)
        .project(
            indices
                .iter()
                .map(|i| Expr::Column(columns[*i].clone()))
                .collect::<Vec<_>>(),
        )?
        .build()
}

fn work_indices(plan: &LogicalPlan, name: &str) -> Result<BTreeSet<usize>> {
    let mut indices = BTreeSet::new();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            if let Ok(provider) = source_as_provider(&scan.source) {
                if let Some(work) = provider.as_any().downcast_ref::<CteWorkTable>() {
                    if work.name() == name {
                        indices.extend(
                            scan.projection
                                .clone()
                                .unwrap_or_else(|| (0..work.schema().fields().len()).collect()),
                        );
                    }
                }
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(indices)
}

// Used only during this rule. Identity projections on both inputs ensure the
// upstream rule returns exactly the requested columns, in their original order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Hash)]
struct PrunableRecursive {
    query: RecursiveQuery,
    dependencies: Vec<Vec<usize>>,
    required: Vec<usize>,
    original_indices: Vec<usize>,
}

impl UserDefinedLogicalNodeCore for PrunableRecursive {
    fn name(&self) -> &str {
        "PrunableRecursive"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        vec![&self.query.static_term, &self.query.recursive_term]
    }
    fn schema(&self) -> &DFSchemaRef {
        self.query.static_term.schema()
    }
    fn expressions(&self) -> Vec<Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PrunableRecursive: {}", self.query.name)
    }
    fn necessary_children_exprs(&self, output: &[usize]) -> Option<Vec<Vec<usize>>> {
        // UNION DISTINCT compares complete rows between iterations. Dropping a
        // field can change both multiplicity and termination.
        let mut needed: BTreeSet<_> = if self.query.is_distinct {
            (0..self.schema().fields().len()).collect()
        } else {
            output.iter().copied().collect()
        };
        needed.extend(&self.required);
        loop {
            let before = needed.len();
            for i in needed.clone() {
                needed.extend(&self.dependencies[i]);
            }
            // Even a zero-column result needs the recursive predicate/join
            // dependencies to preserve the number of rows and termination.
            needed.extend(self.dependencies.last().into_iter().flatten().copied());
            if needed.len() == before {
                break;
            }
        }
        let indices: Vec<_> = needed.into_iter().collect();
        Some(vec![indices.clone(), indices])
    }
    fn with_exprs_and_inputs(&self, _: Vec<Expr>, inputs: Vec<LogicalPlan>) -> Result<Self> {
        let [static_term, recursive_term]: [LogicalPlan; 2] = inputs.try_into().map_err(|_| {
            datafusion::common::internal_datafusion_err!("recursive projection requires two inputs")
        })?;
        let retained = static_term
            .schema()
            .columns()
            .iter()
            .map(|c| self.schema().index_of_column(c))
            .collect::<Result<Vec<_>>>()?;
        let mut positions = vec![None; self.schema().fields().len()];
        for (new, old) in retained.iter().enumerate() {
            positions[*old] = Some(new);
        }
        let schema = Arc::new(static_term.schema().as_arrow().clone());
        let recursive_term = recursive_term
            .transform_up_with_subqueries(|node| {
                let LogicalPlan::TableScan(scan) = &node else {
                    return Ok(Transformed::no(node));
                };
                let Ok(provider) = source_as_provider(&scan.source) else {
                    return Ok(Transformed::no(node));
                };
                let Some(work) = provider.as_any().downcast_ref::<CteWorkTable>() else {
                    return Ok(Transformed::no(node));
                };
                if work.name() != self.query.name {
                    return Ok(Transformed::no(node));
                }
                let projection = scan
                    .projection
                    .clone()
                    .unwrap_or_else(|| (0..positions.len()).collect());
                let projection = projection
                    .into_iter()
                    .map(|old| {
                        positions.get(old).copied().flatten().ok_or_else(|| {
                            datafusion::common::internal_datafusion_err!(
                                "recursive worktable column {old} was pruned while still in use"
                            )
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let scan = TableScan::try_new(
                    scan.table_name.clone(),
                    provider_as_source(Arc::new(CteWorkTable::new(work.name(), schema.clone()))),
                    Some(projection),
                    scan.filters.clone(),
                    scan.fetch,
                )?;
                Ok(Transformed::yes(LogicalPlan::TableScan(scan)))
            })?
            .data;
        let remap = |deps: &[usize]| {
            deps.iter()
                .filter_map(|old| positions[*old])
                .collect::<Vec<_>>()
        };
        let mut dependencies = retained
            .iter()
            .map(|i| remap(&self.dependencies[*i]))
            .collect::<Vec<_>>();
        dependencies.push(remap(self.dependencies.last().unwrap()));
        Ok(Self {
            query: RecursiveQuery {
                name: self.query.name.clone(),
                static_term: Arc::new(static_term),
                recursive_term: Arc::new(recursive_term),
                is_distinct: self.query.is_distinct,
            },
            required: remap(&self.required),
            original_indices: retained.iter().map(|i| self.original_indices[*i]).collect(),
            dependencies,
        })
    }
}

impl OptimizerRule for RecursiveProjection {
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
        // Copies of a named CTE must retain one common positional layout.
        // Merge consumer requirements and repeat until nested CTE requirements
        // also stabilize. Each pass only adds columns to the retained sets.
        let mut counts = BTreeMap::<String, usize>::new();
        plan.apply_with_subqueries(|node| {
            if let LogicalPlan::RecursiveQuery(query) = node {
                *counts.entry(query.name.clone()).or_default() += 1;
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        let mut requirements = BTreeMap::<String, BTreeSet<usize>>::new();
        let (wrapped_changed, optimized) = loop {
            let wrapped = plan.clone().transform_up_with_subqueries(|node| {
                let LogicalPlan::RecursiveQuery(query) = node else {
                    return Ok(Transformed::no(node));
                };
                let width = query.static_term.schema().fields().len();
                let mut dependencies = Vec::with_capacity(width + 1);
                // Ask the same projection rule which worktable fields produce each
                // output. Closing these dependencies accounts for future iterations.
                for indices in (0..if query.is_distinct { 0 } else { width })
                    .map(|i| vec![i])
                    .chain(std::iter::once(vec![]))
                {
                    let term = self
                        .inner
                        .rewrite(
                            project(query.recursive_term.as_ref().clone(), &indices)?,
                            config,
                        )?
                        .data;
                    dependencies.push(work_indices(&term, &query.name)?.into_iter().collect());
                }
                if query.is_distinct {
                    dependencies = vec![vec![]; width + 1];
                }
                let required = requirements
                    .get(&query.name)
                    .map(|r| r.iter().copied().collect())
                    .unwrap_or_default();
                let all = (0..width).collect::<Vec<_>>();
                let query = RecursiveQuery {
                    static_term: Arc::new(project(query.static_term.as_ref().clone(), &all)?),
                    recursive_term: Arc::new(project(query.recursive_term.as_ref().clone(), &all)?),
                    ..query
                };
                Ok(Transformed::yes(LogicalPlan::Extension(Extension {
                    node: Arc::new(PrunableRecursive {
                        query,
                        dependencies,
                        required,
                        original_indices: all,
                    }),
                })))
            })?;
            let optimized = self.inner.rewrite(wrapped.data, config)?;

            let mut changed = false;
            optimized.data.apply_with_subqueries(|node| {
                if let LogicalPlan::Extension(extension) = node {
                    if let Some(recursive) =
                        extension.node.as_any().downcast_ref::<PrunableRecursive>()
                    {
                        if counts.get(&recursive.query.name).copied().unwrap_or(0) > 1 {
                            let required = requirements
                                .entry(recursive.query.name.clone())
                                .or_default();
                            let before = required.len();
                            required.extend(&recursive.original_indices);
                            changed |= before != required.len();
                        }
                    }
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
            if !changed {
                break (wrapped.transformed, optimized);
            }
        };
        let restored = optimized.data.transform_up_with_subqueries(|node| {
            if let LogicalPlan::Extension(extension) = &node {
                if let Some(recursive) = extension.node.as_any().downcast_ref::<PrunableRecursive>()
                {
                    return Ok(Transformed::yes(LogicalPlan::RecursiveQuery(
                        recursive.query.clone(),
                    )));
                }
            }
            Ok(Transformed::no(node))
        })?;
        Ok(if wrapped_changed || optimized.transformed {
            Transformed::yes(restored.data)
        } else {
            Transformed::no(restored.data)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::{
        logical_expr::{col, lit},
        optimizer::{OptimizerContext, optimize_projections::OptimizeProjections},
        prelude::SessionConfig,
    };

    fn recurrence(distinct: bool) -> Result<LogicalPlan> {
        let seed = LogicalPlanBuilder::empty(true)
            .project(vec![
                lit(999_i64).alias("unused"),
                lit(0_i64).alias("step"),
                lit(1_i64).alias("a"),
                lit(2_i64).alias("b"),
                lit(3_i64).alias("c"),
                lit("unused payload").alias("payload"),
            ])?
            .build()?;
        let work = Arc::new(CteWorkTable::new(
            "r",
            Arc::new(seed.schema().as_arrow().clone()),
        ));
        let recursive = LogicalPlanBuilder::scan("r", provider_as_source(work), None)?
            .filter(col("step").lt(lit(3_i64)))?
            .project(vec![
                col("unused"),
                (col("step") + lit(1_i64)).alias("step"),
                col("b").alias("a"),
                col("c").alias("b"),
                (col("c") + lit(1_i64)).alias("c"),
                col("payload"),
            ])?
            .build()?;
        Ok(LogicalPlan::RecursiveQuery(RecursiveQuery {
            name: "r".into(),
            static_term: Arc::new(seed),
            recursive_term: Arc::new(recursive),
            is_distinct: distinct,
        }))
    }

    fn optimize(plan: LogicalPlan) -> Result<LogicalPlan> {
        RecursiveProjection {
            inner: Arc::new(OptimizeProjections::new()),
        }
        .rewrite(plan, &OptimizerContext::new())
        .map(|t| t.data)
    }

    fn check_worktables(plan: &LogicalPlan, width: usize) -> Result<()> {
        let mut found = false;
        plan.apply_with_subqueries(|node| {
            if let LogicalPlan::TableScan(scan) = node {
                if let Ok(provider) = source_as_provider(&scan.source) {
                    if let Some(work) = provider.as_any().downcast_ref::<CteWorkTable>() {
                        found = true;
                        assert_eq!(work.schema().fields().len(), width, "{plan:?}");
                        assert!(scan.projection.as_ref().unwrap().iter().all(|i| *i < width));
                    }
                }
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        assert!(found);
        Ok(())
    }

    #[tokio::test]
    async fn recursive_projection_remaps_indices_and_closes_iteration_dependencies() -> Result<()> {
        let plan = LogicalPlanBuilder::from(recurrence(false)?)
            .project(vec![col("a")])?
            .build()?;
        let optimized = optimize(plan)?;
        check_worktables(&optimized, 4)?;
        assert_eq!(optimize(optimized.clone())?, optimized);
        let ctx = super::super::session(SessionConfig::new());
        let batches = ctx.execute_logical_plan(optimized).await?.collect().await?;
        let values = batches
            .iter()
            .flat_map(|b| {
                b.column(0)
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .unwrap()
                    .values()
                    .iter()
                    .copied()
            })
            .collect::<Vec<_>>();
        assert_eq!(values, vec![1, 2, 3, 4]);
        Ok(())
    }

    #[tokio::test]
    async fn zero_column_recursive_output_preserves_iteration_count() -> Result<()> {
        let plan = project(recurrence(false)?, &[])?;
        let optimized = optimize(plan)?;
        check_worktables(&optimized, 1)?;
        let ctx = super::super::session(SessionConfig::new());
        let batches = ctx.execute_logical_plan(optimized).await?.collect().await?;
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn recursive_distinct_retains_full_row_identity() -> Result<()> {
        let plan = LogicalPlanBuilder::from(recurrence(true)?)
            .project(vec![col("unused")])?
            .build()?;
        let optimized = optimize(plan)?;
        check_worktables(&optimized, 6)?;
        let ctx = super::super::session(SessionConfig::new());
        let batches = ctx.execute_logical_plan(optimized).await?.collect().await?;
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn shared_recursive_consumers_keep_one_worktable_layout() -> Result<()> {
        let query = recurrence(false)?;
        let left = LogicalPlanBuilder::from(query.clone())
            .project(vec![col("a")])?
            .build()?;
        let right = LogicalPlanBuilder::from(query)
            .project(vec![col("step").alias("a")])?
            .build()?;
        let optimized = optimize(LogicalPlanBuilder::from(left).union(right)?.build()?)?;
        check_worktables(&optimized, 4)?;
        let mut definitions = vec![];
        optimized.apply_with_subqueries(|node| {
            if let LogicalPlan::RecursiveQuery(query) = node {
                definitions.push(query.clone());
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0], definitions[1]);
        let ctx = super::super::session(SessionConfig::new());
        let batches = ctx.execute_logical_plan(optimized).await?.collect().await?;
        let mut values = batches
            .iter()
            .flat_map(|b| {
                b.column(0)
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .unwrap()
                    .values()
                    .iter()
                    .copied()
            })
            .collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, vec![0, 1, 1, 2, 2, 3, 3, 4]);
        Ok(())
    }
}
