//! Executable relational lowering for Graph IR.
//!
//! This module decomposes supported Graph IR regions into ordinary
//! DataFusion logical plans. It intentionally sits beside `ir::df`: that
//! module preserves graph operators as DataFusion extension nodes for rules
//! and round-tripping, while this module lowers graph-shaped operators into
//! base relational scans, joins, projections, filters, and aggregates that
//! DataFusion can execute directly.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Array, Int64Builder, RecordBatch, StringArray,
    StringBuilder, new_null_array,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow_select::concat::concat_batches;
use datafusion::common::{Column, ScalarValue};
use datafusion::datasource::{MemTable, provider_as_source};
use datafusion::error::DataFusionError;
use datafusion::functions::math::expr_fn as df_math;
use datafusion::functions_aggregate::count::count_all;
use datafusion::functions_aggregate::expr_fn::{
    array_agg as df_array_agg, avg as df_avg, count as df_count, max as df_max, min as df_min,
    sum as df_sum,
};
use datafusion::functions_window::expr_fn as df_window;
use datafusion::logical_expr::ExprFunctionExt;
use datafusion::logical_expr::expr::{Case, InList};
use datafusion::logical_expr::{
    BinaryExpr, Cast, Expr, ExprSchemable, JoinType, LogicalPlan, LogicalPlanBuilder, Operator,
    TryCast,
};
use datafusion::prelude::{SessionConfig, SessionContext, lit};

use crate::ir::catalog::{CatalogError, EdgeTable, NodeTable, PropertyGraph};
use crate::ir::expr::{AggKind, BinaryOp, IrExpr, Lit, StringOp};
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::plan::{
    ApplyKind, BindKind, ChooseArm, ChooseSelector, ChooseUnmatched, CoalesceSuccess, Direction,
    GraphPlan, JoinKind, LabelExpr, Node, NullsOrder, ProjectMode, ProjectionItem, Slice, SortDir,
    TargetMode, UnionAlign,
};
use crate::ir::policy::{Language, ResultForm};
use crate::ir::value::Value;

const ID_SUFFIX: &str = "__id";
const LABEL_SUFFIX: &str = "__label";
const PROP_MARKER: &str = "__prop__";
const SRC_ID_SUFFIX: &str = "__src_id";
const SRC_LABEL_SUFFIX: &str = "__src_label";
const DST_ID_SUFFIX: &str = "__dst_id";
const DST_LABEL_SUFFIX: &str = "__dst_label";
const MAX_EXECUTABLE_PLAN_NODES: usize = 80;
const MAX_EXECUTABLE_PLAN_DEPTH: usize = 48;

#[derive(Debug, Clone, Default)]
pub struct RelBackend {
    options: RelBackendOptions,
}

#[derive(Debug, Clone)]
pub struct RelBackendOptions {
    /// Internal path-maintenance expressions are ignored when the path is not
    /// projected to the user. This lets ordinary Gremlin traversals lower to
    /// relational plans while path-returning traversals still surface gaps as
    /// mismatches or unsupported expressions.
    pub tolerate_internal_path_state: bool,
}

impl Default for RelBackendOptions {
    fn default() -> Self {
        Self {
            tolerate_internal_path_state: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RelError {
    #[error("unsupported relational lowering: {0}")]
    Unsupported(String),
    #[error("catalog: {0}")]
    Catalog(#[from] CatalogError),
    #[error("arrow: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("datafusion: {0}")]
    DataFusion(#[from] DataFusionError),
}

pub type RelResult<T> = Result<T, RelError>;

#[derive(Debug, Clone, Default)]
pub struct IslandReport {
    pub lowerable_nodes: usize,
    pub unsupported: Vec<String>,
}

impl IslandReport {
    pub fn is_complete(&self) -> bool {
        self.unsupported.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct LogicalPlanStats {
    nodes: usize,
    depth: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct GraphPlanStats {
    nodes: usize,
    depth: usize,
    bidirectional_expands: usize,
    select_history_projects: usize,
}

#[derive(Debug, Clone)]
pub struct LoweredPlan {
    pub plan: LogicalPlan,
    pub fields: Vec<String>,
    pub result_form: ResultForm,
    pub islands: IslandReport,
}

#[derive(Debug, Clone)]
struct LoweredNode {
    plan: LogicalPlan,
    islands: IslandReport,
    fields: Option<Vec<String>>,
    result_form: Option<ResultForm>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingShape {
    Node,
    Edge,
}

#[derive(Debug)]
struct LoweringContext<'a> {
    graph: &'a PropertyGraph,
    options: RelBackendOptions,
    language: Language,
    scan_counter: usize,
    correlate_plan: Option<LogicalPlan>,
}

impl RelBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: RelBackendOptions) -> Self {
        Self { options }
    }

    pub fn lower(&self, plan: &GraphPlan, graph: &PropertyGraph) -> RelResult<LoweredPlan> {
        let graph_stats = graph_plan_stats(&plan.root);
        if plan.policy.language == Language::Gremlin
            && graph_stats.bidirectional_expands >= 2
            && graph_stats.select_history_projects > 0
            && graph_stats.depth > 28
        {
            return Err(RelError::Unsupported(format!(
                "Gremlin plan is not a safe SQL island yet: nodes={} depth={} both_expands={} select_history_projects={}",
                graph_stats.nodes,
                graph_stats.depth,
                graph_stats.bidirectional_expands,
                graph_stats.select_history_projects
            )));
        }
        let mut ctx = LoweringContext {
            graph,
            options: self.options.clone(),
            language: plan.policy.language,
            scan_counter: 0,
            correlate_plan: None,
        };
        let lowered = ctx.lower_node(&plan.root)?;
        let fields = lowered
            .fields
            .clone()
            .unwrap_or_else(|| output_fields(&lowered.plan));
        Ok(LoweredPlan {
            plan: lowered.plan,
            fields,
            result_form: lowered.result_form.unwrap_or(ResultForm::RowSet),
            islands: lowered.islands,
        })
    }

    pub async fn execute(
        &self,
        plan: &GraphPlan,
        graph: &PropertyGraph,
    ) -> RelResult<ReturnedBatches> {
        let lowered = self.lower(plan, graph)?;
        execute_lowered(lowered).await
    }
}

async fn execute_lowered(lowered: LoweredPlan) -> RelResult<ReturnedBatches> {
    let stats = logical_plan_stats(&lowered.plan);
    if stats.nodes > MAX_EXECUTABLE_PLAN_NODES || stats.depth > MAX_EXECUTABLE_PLAN_DEPTH {
        return Err(RelError::Unsupported(format!(
            "relational island too complex to execute safely yet: nodes={} depth={}",
            stats.nodes, stats.depth
        )));
    }
    let output_schema = Arc::new(lowered.plan.schema().as_arrow().clone());
    let config = SessionConfig::new()
        .set_usize("datafusion.optimizer.max_passes", 1)
        .set_bool("datafusion.optimizer.enable_dynamic_filter_pushdown", false);
    let ctx = SessionContext::new_with_config(config);
    let df = ctx.execute_logical_plan(lowered.plan).await?;
    let batches = df.collect().await?;
    let batch = if batches.is_empty() {
        RecordBatch::new_empty(output_schema)
    } else if batches.len() == 1 {
        batches.into_iter().next().expect("single batch")
    } else {
        concat_batches(&output_schema, batches.iter())?
    };
    Ok(ReturnedBatches {
        fields: lowered.fields,
        result_form: lowered.result_form,
        batch,
    })
}

fn logical_plan_stats(plan: &LogicalPlan) -> LogicalPlanStats {
    let mut stats = LogicalPlanStats::default();
    let mut stack = vec![(plan, 1usize)];
    while let Some((node, depth)) = stack.pop() {
        stats.nodes += 1;
        stats.depth = stats.depth.max(depth);
        for input in node.inputs() {
            stack.push((input, depth + 1));
        }
    }
    stats
}

fn graph_plan_stats(root: &Node) -> GraphPlanStats {
    let mut stats = GraphPlanStats::default();
    let mut stack = vec![(root, 1usize)];
    while let Some((node, depth)) = stack.pop() {
        stats.nodes += 1;
        stats.depth = stats.depth.max(depth);
        match node {
            Node::GraphExpand { dir, input, .. } => {
                if *dir == Direction::Both {
                    stats.bidirectional_expands += 1;
                }
                stack.push((input, depth + 1));
            }
            Node::GraphProject { items, input, .. } => {
                stats.select_history_projects += items
                    .iter()
                    .filter(|item| item.alias.starts_with("__gremlin_select_history_"))
                    .count();
                stack.push((input, depth + 1));
            }
            Node::GraphReturn { input, .. }
            | Node::GraphConstructTriples { input, .. }
            | Node::GraphDescribe { input, .. }
            | Node::GraphAsk { input, .. }
            | Node::GraphBind { input, .. }
            | Node::GraphPathPattern { input, .. }
            | Node::GraphPathFilter { input, .. }
            | Node::GraphCreate { input, .. }
            | Node::GraphSetProperty { input, .. }
            | Node::GraphDelete { input, .. }
            | Node::GraphFilter { input, .. }
            | Node::GraphCurrentProject { input, .. }
            | Node::GraphAggregate { input, .. }
            | Node::GraphGroupMap { input, .. }
            | Node::GraphGroupCountSideEffect { input, .. }
            | Node::GraphCap { input, .. }
            | Node::GraphShortestPath { input, .. }
            | Node::GraphDistinct { input, .. }
            | Node::GraphSort { input, .. }
            | Node::GraphSlice { input, .. }
            | Node::GraphSliceExpr { input, .. }
            | Node::GraphBarrier { input, .. }
            | Node::GraphUnwind { input, .. }
            | Node::GraphQuantifier { input, .. }
            | Node::GraphCollect { input, .. }
            | Node::GraphListComprehension { input, .. }
            | Node::GraphSelect { input, .. }
            | Node::GraphService { input, .. } => {
                stack.push((input, depth + 1));
            }
            Node::GraphJoin { left, right, .. }
            | Node::GraphApply { left, right, .. }
            | Node::GraphUnion { left, right, .. }
            | Node::GraphSparqlMinus { left, right, .. } => {
                stack.push((left, depth + 1));
                stack.push((right, depth + 1));
            }
            Node::GraphRepeat {
                seed,
                body,
                until_traversal,
                prefix_traversal,
                ..
            } => {
                stack.push((seed, depth + 1));
                stack.push((body, depth + 1));
                if let Some(input) = until_traversal {
                    stack.push((input, depth + 1));
                }
                if let Some(input) = prefix_traversal {
                    stack.push((input, depth + 1));
                }
            }
            Node::GraphCoalesce { input, arms, .. } => {
                stack.push((input, depth + 1));
                for arm in arms {
                    stack.push((arm, depth + 1));
                }
            }
            Node::GraphChoose {
                input,
                arms,
                default,
                ..
            } => {
                stack.push((input, depth + 1));
                for arm in arms {
                    stack.push((&arm.body, depth + 1));
                }
                if let Some(default) = default {
                    stack.push((default, depth + 1));
                }
            }
            Node::GraphProcedureCall { input, .. } => {
                if let Some(input) = input {
                    stack.push((input, depth + 1));
                }
            }
            Node::GraphExtension { inputs, .. } => {
                for input in inputs {
                    stack.push((input, depth + 1));
                }
            }
            Node::GraphNodeScan { .. }
            | Node::GraphRelScan { .. }
            | Node::GraphValues { .. }
            | Node::GraphOneRow
            | Node::GraphEmpty
            | Node::GraphCorrelate { .. }
            | Node::GraphRdfQuadScan { .. }
            | Node::GraphRdfPropertyPath { .. } => {}
        }
    }
    stats
}

impl<'a> LoweringContext<'a> {
    fn lower_node(&mut self, node: &Node) -> RelResult<LoweredNode> {
        use Node::*;
        let lowered = match node {
            GraphReturn {
                fields,
                result_form,
                input,
            } => {
                let input = self.lower_node(input)?;
                let exprs = self.return_projection(&input.plan, fields)?;
                let plan = LogicalPlanBuilder::from(input.plan)
                    .project(exprs)?
                    .build()?;
                LoweredNode {
                    plan,
                    islands: input.islands,
                    fields: Some(fields.clone()),
                    result_form: Some(*result_form),
                }
            }
            GraphNodeScan {
                binding, labels, ..
            } => self.lower_node_scan(binding, labels)?,
            GraphRelScan { binding, types, .. } => self.lower_rel_scan(binding, types)?,
            GraphValues {
                bindings,
                rows,
                bulk: _,
            } => self.lower_values(bindings, rows)?,
            GraphOneRow => LoweredNode::new(LogicalPlanBuilder::empty(true).build()?),
            GraphEmpty => LoweredNode::new(LogicalPlanBuilder::empty(false).build()?),
            GraphCorrelate { .. } => {
                let Some(plan) = &self.correlate_plan else {
                    return Err(RelError::Unsupported(
                        "GraphCorrelate outside GraphApply".into(),
                    ));
                };
                LoweredNode::new(plan.clone())
            }
            GraphBind {
                bind,
                kind,
                expr,
                input,
            } => self.lower_bind(bind, *kind, expr.as_ref(), input)?,
            GraphExpand {
                source,
                target,
                target_mode,
                target_labels,
                rel_binding,
                rel_types,
                dir,
                length,
                input,
                ..
            } => {
                if length.is_variable_length() {
                    return Err(RelError::Unsupported(
                        "variable-length expand needs recursive DataFusion lowering".into(),
                    ));
                }
                self.lower_expand(
                    input,
                    source,
                    target,
                    *target_mode,
                    target_labels,
                    rel_binding.as_ref(),
                    rel_types,
                    *dir,
                )?
            }
            GraphFilter { condition, input } => {
                let input = self.lower_node(input)?;
                let condition = self.lower_expr(&input.plan, condition)?;
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .filter(condition)?
                    .build()?;
                input.with_plan(plan)
            }
            GraphProject {
                mode, items, input, ..
            } => self.lower_project(*mode, items, input)?,
            GraphCurrentProject {
                expr,
                fields,
                input,
            } => self.lower_current_project(expr, fields, input)?,
            GraphAggregate {
                group, aggs, input, ..
            } => {
                let input = self.lower_node(input)?;
                let mut group_exprs = apply_correlation_key_columns(&input.plan)
                    .iter()
                    .map(|key| col_exact(key).alias(key))
                    .collect::<Vec<_>>();
                group_exprs.extend(
                    group
                        .iter()
                        .map(|item| {
                            self.lower_expr(&input.plan, &item.expr)
                                .map(|expr| expr.alias(item.alias.clone()))
                        })
                        .collect::<RelResult<Vec<_>>>()?,
                );
                let aggs = aggs
                    .iter()
                    .map(|agg| {
                        let expr = match agg.kind {
                            AggKind::CountRows | AggKind::CountBulk => match &agg.arg {
                                Some(arg) => df_count(self.lower_expr(&input.plan, arg)?),
                                None => count_all(),
                            },
                            AggKind::CountDistinct => {
                                let Some(arg) = &agg.arg else {
                                    return Err(RelError::Unsupported(
                                        "count distinct without an argument".into(),
                                    ));
                                };
                                datafusion::functions_aggregate::count::count_distinct(
                                    self.lower_expr(&input.plan, arg)?,
                                )
                            }
                            AggKind::Sum | AggKind::SumOrZero => {
                                df_sum(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                            }
                            AggKind::Avg | AggKind::AvgOrZero | AggKind::AvgOrNull => {
                                df_avg(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                            }
                            AggKind::Min | AggKind::MinOrNull => {
                                df_min(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                            }
                            AggKind::Max | AggKind::MaxOrNull => {
                                df_max(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                            }
                            AggKind::CollectRows | AggKind::CollectTraversers => {
                                let expr = df_array_agg(
                                    self.lower_required_agg_arg(&input.plan, &agg.arg)?,
                                );
                                if agg.distinct {
                                    expr.distinct().build()?
                                } else {
                                    expr
                                }
                            }
                            other => {
                                return Err(RelError::Unsupported(format!(
                                    "aggregate `{other:?}` is not relationally lowered yet"
                                )));
                            }
                        };
                        Ok(expr.alias(agg.alias.clone()))
                    })
                    .collect::<RelResult<Vec<_>>>()?;
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .aggregate(group_exprs, aggs)?
                    .build()?;
                input.with_plan(plan)
            }
            GraphDistinct { input, .. } => {
                let input = self.lower_node(input)?;
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .distinct()?
                    .build()?;
                input.with_plan(plan)
            }
            GraphSort { keys, input } => {
                let input = self.lower_node(input)?;
                let mut sorts = Vec::new();
                for key in keys {
                    sorts.extend(self.sort_exprs(&input.plan, key)?);
                }
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .sort(sorts)?
                    .build()?;
                input.with_plan(plan)
            }
            GraphSlice { slice, input } => {
                let input = self.lower_node(input)?;
                let Slice {
                    offset,
                    fetch,
                    tail,
                } = slice;
                if tail.is_some() {
                    return Err(RelError::Unsupported("tail slice".into()));
                }
                let correlation_keys = apply_correlation_key_columns(&input.plan);
                let plan = if correlation_keys.is_empty() {
                    LogicalPlanBuilder::from(input.plan.clone())
                        .limit(*offset as usize, fetch.map(|n| n as usize))?
                        .build()?
                } else {
                    partitioned_limit(input.plan.clone(), &correlation_keys, *offset, *fetch)?
                };
                input.with_plan(plan)
            }
            GraphJoin {
                kind,
                left,
                right,
                condition,
            } => self.lower_join(*kind, left, right, condition.as_ref())?,
            GraphApply {
                kind,
                correlation,
                outputs,
                left,
                right,
                ..
            } => self.lower_apply(*kind, correlation, outputs, left, right)?,
            GraphUnion {
                all,
                align,
                left,
                right,
            } => self.lower_union(*all, *align, left, right)?,
            GraphChoose {
                selector,
                arms,
                default,
                unmatched,
                input,
                ..
            } => self.lower_choose(selector, arms, default.as_deref(), *unmatched, input)?,
            GraphCoalesce {
                success,
                output,
                correlation,
                input,
                arms,
                ..
            } => self.lower_coalesce(*success, output, correlation, input, arms)?,
            GraphUnwind {
                input_expr,
                bind,
                outer,
                input,
            } => self.lower_unwind(input_expr, bind, *outer, input)?,
            other => {
                return Err(RelError::Unsupported(format!(
                    "{}",
                    unsupported_node_name(other)
                )));
            }
        };
        Ok(lowered)
    }

    fn lower_required_agg_arg(&self, plan: &LogicalPlan, arg: &Option<IrExpr>) -> RelResult<Expr> {
        let Some(arg) = arg else {
            return Err(RelError::Unsupported(
                "aggregate requires an argument".to_string(),
            ));
        };
        self.lower_expr(plan, arg)
    }

    fn lower_node_scan(&mut self, binding: &str, labels: &LabelExpr) -> RelResult<LoweredNode> {
        let labels = self.node_labels(labels)?;
        let prop_defs = self.node_property_defs(&labels)?;
        let schema = node_schema(binding, &prop_defs);
        let mut batches = Vec::new();
        for label in labels {
            let table = self.graph.node_table(&label)?;
            batches.push(normalize_node_table(
                binding,
                table,
                &prop_defs,
                schema.clone(),
            )?);
        }
        if batches.is_empty() {
            batches.push(RecordBatch::new_empty(schema));
        }
        self.scan_batches("nodes", batches)
    }

    fn lower_rel_scan(&mut self, binding: &str, types: &LabelExpr) -> RelResult<LoweredNode> {
        let rel_types = self.rel_types(types)?;
        let prop_defs = self.edge_property_defs(&rel_types)?;
        let schema = edge_schema(binding, &prop_defs);
        let mut batches = Vec::new();
        for rel_type in rel_types {
            let mut base_id = 0_i64;
            for table in self.graph.edge_tables(&rel_type)? {
                batches.push(normalize_edge_table(
                    binding,
                    table,
                    base_id,
                    &prop_defs,
                    schema.clone(),
                )?);
                base_id += table.batch.num_rows() as i64;
            }
        }
        if batches.is_empty() {
            batches.push(RecordBatch::new_empty(schema));
        }
        self.scan_batches("edges", batches)
    }

    fn lower_values(&mut self, bindings: &[String], rows: &[Vec<Value>]) -> RelResult<LoweredNode> {
        let batch = values_batch(bindings, rows)?;
        self.scan_batches("values", vec![batch])
    }

    fn lower_bind(
        &mut self,
        bind: &str,
        kind: BindKind,
        expr: Option<&IrExpr>,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let Some(expr) = expr else {
            if has_binding_shape(&input.plan, bind).is_some() || has_exact_col(&input.plan, bind) {
                return Ok(input);
            }
            if has_binding_shape(&input.plan, "current").is_some() {
                let projections = duplicate_binding_projection(&input.plan, "current", bind)?;
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .project(projections)?
                    .build()?;
                return Ok(input.with_plan(plan));
            }
            return match kind {
                BindKind::Node | BindKind::Edge => Err(RelError::Unsupported(format!(
                    "metadata bind `{bind}` has no source element"
                ))),
                BindKind::Scalar => Ok(input),
            };
        };

        let mut projections = existing_columns(&input.plan, &BTreeSet::from([bind.to_string()]));
        projections.extend(self.project_item_exprs(&input.plan, bind, expr)?);
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections)?
            .build()?;
        Ok(input.with_plan(plan))
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_expand(
        &mut self,
        input: &Node,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
        dir: Direction,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        if has_binding_shape(&input.plan, source).is_none() {
            return Err(RelError::Unsupported(format!(
                "expand source `{source}` is not an element binding"
            )));
        }

        match dir {
            Direction::Out | Direction::In => self.lower_expand_direction(
                input,
                source,
                target,
                target_mode,
                target_labels,
                rel_binding,
                rel_types,
                dir,
            ),
            Direction::Both => self.lower_expand_both(
                input,
                source,
                target,
                target_mode,
                target_labels,
                rel_binding,
                rel_types,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_expand_both(
        &mut self,
        input: LoweredNode,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
    ) -> RelResult<LoweredNode> {
        let rel = rel_binding
            .cloned()
            .unwrap_or_else(|| format!("__rel_{}", self.scan_counter));
        let edge_scan = self.lower_rel_scan(&rel, rel_types)?;

        let source_is_src = binding_pair_eq(source, &src_id_col(&rel), &src_label_col(&rel));
        let source_is_dst = binding_pair_eq(source, &dst_id_col(&rel), &dst_label_col(&rel));
        let source_join = vec![Expr::or(source_is_src.clone(), source_is_dst.clone())];
        let mut joined = LogicalPlanBuilder::from(input.plan.clone())
            .join_on(edge_scan.plan.clone(), JoinType::Inner, source_join)?
            .build()?;

        match target_mode {
            TargetMode::Existing => {
                let target_is_dst =
                    binding_pair_eq(target, &dst_id_col(&rel), &dst_label_col(&rel));
                let target_is_src =
                    binding_pair_eq(target, &src_id_col(&rel), &src_label_col(&rel));
                let opposite = Expr::or(
                    Expr::and(source_is_src, target_is_dst),
                    Expr::and(source_is_dst, target_is_src),
                );
                joined = LogicalPlanBuilder::from(joined).filter(opposite)?.build()?;
            }
            TargetMode::BindNew
            | TargetMode::ReplaceCurrent
            | TargetMode::ReplaceCurrentAndBindLabel
            | TargetMode::BindNewOrReplaceCurrent => {
                let target_scan_binding = if has_binding_shape(&joined, target).is_some() {
                    format!("__target_{}", self.scan_counter)
                } else {
                    target.to_string()
                };
                let target_scan = self.lower_node_scan(&target_scan_binding, target_labels)?;
                let target_is_dst = binding_pair_eq(
                    &target_scan_binding,
                    &dst_id_col(&rel),
                    &dst_label_col(&rel),
                );
                let target_is_src = binding_pair_eq(
                    &target_scan_binding,
                    &src_id_col(&rel),
                    &src_label_col(&rel),
                );
                let target_join = vec![Expr::or(
                    Expr::and(source_is_src, target_is_dst),
                    Expr::and(source_is_dst, target_is_src),
                )];
                joined = LogicalPlanBuilder::from(joined)
                    .join_on(target_scan.plan, JoinType::Inner, target_join)?
                    .build()?;
                if target_scan_binding != target {
                    let mut projections = existing_columns_excluding_bindings(
                        &joined,
                        &[target, target_scan_binding.as_str()],
                    );
                    projections.extend(duplicate_binding_projection_only(
                        &joined,
                        &target_scan_binding,
                        target,
                    )?);
                    joined = LogicalPlanBuilder::from(joined)
                        .project(projections)?
                        .build()?;
                }
            }
        }

        let mut islands = input.islands;
        islands.merge(edge_scan.islands);
        Ok(LoweredNode {
            plan: joined,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_expand_direction(
        &mut self,
        input: LoweredNode,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
        dir: Direction,
    ) -> RelResult<LoweredNode> {
        let rel = rel_binding
            .cloned()
            .unwrap_or_else(|| format!("__rel_{}", self.scan_counter));
        let edge_scan = self.lower_rel_scan(&rel, rel_types)?;

        let (edge_source_id, edge_source_label, edge_target_id, edge_target_label) = match dir {
            Direction::Out => (
                src_id_col(&rel),
                src_label_col(&rel),
                dst_id_col(&rel),
                dst_label_col(&rel),
            ),
            Direction::In => (
                dst_id_col(&rel),
                dst_label_col(&rel),
                src_id_col(&rel),
                src_label_col(&rel),
            ),
            Direction::Both => unreachable!("both expands are split before lowering"),
        };

        let source_join = vec![
            binary(
                col_exact(id_col(source)),
                BinaryOp::Eq,
                col_exact(edge_source_id),
            ),
            binary(
                col_exact(label_col(source)),
                BinaryOp::Eq,
                col_exact(edge_source_label),
            ),
        ];
        let mut joined = LogicalPlanBuilder::from(input.plan.clone())
            .join_on(edge_scan.plan.clone(), JoinType::Inner, source_join)?
            .build()?;

        match target_mode {
            TargetMode::Existing => {
                let filters = vec![
                    binary(
                        col_exact(id_col(target)),
                        BinaryOp::Eq,
                        col_exact(edge_target_id),
                    ),
                    binary(
                        col_exact(label_col(target)),
                        BinaryOp::Eq,
                        col_exact(edge_target_label),
                    ),
                ];
                let filter = filters.into_iter().reduce(Expr::and).expect("filters");
                joined = LogicalPlanBuilder::from(joined).filter(filter)?.build()?;
            }
            TargetMode::BindNew
            | TargetMode::ReplaceCurrent
            | TargetMode::ReplaceCurrentAndBindLabel
            | TargetMode::BindNewOrReplaceCurrent => {
                let target_scan_binding = if has_binding_shape(&joined, target).is_some() {
                    format!("__target_{}", self.scan_counter)
                } else {
                    target.to_string()
                };
                let target_scan = self.lower_node_scan(&target_scan_binding, target_labels)?;
                let target_join = vec![
                    binary(
                        col_exact(id_col(&target_scan_binding)),
                        BinaryOp::Eq,
                        col_exact(edge_target_id),
                    ),
                    binary(
                        col_exact(label_col(&target_scan_binding)),
                        BinaryOp::Eq,
                        col_exact(edge_target_label),
                    ),
                ];
                joined = LogicalPlanBuilder::from(joined)
                    .join_on(target_scan.plan, JoinType::Inner, target_join)?
                    .build()?;
                if target_scan_binding != target {
                    let mut projections = existing_columns_excluding_bindings(
                        &joined,
                        &[target, target_scan_binding.as_str()],
                    );
                    projections.extend(duplicate_binding_projection_only(
                        &joined,
                        &target_scan_binding,
                        target,
                    )?);
                    joined = LogicalPlanBuilder::from(joined)
                        .project(projections)?
                        .build()?;
                }
            }
        }

        let mut islands = input.islands;
        islands.merge(edge_scan.islands);
        Ok(LoweredNode {
            plan: joined,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    fn lower_project(
        &mut self,
        mode: ProjectMode,
        items: &[ProjectionItem],
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let aliases = projection_aliases(items);
        if mode == ProjectMode::PreserveVisible
            && aliases
                .iter()
                .all(|alias| !has_exact_col(&input.plan, alias))
            && items.iter().all(|item| {
                self.project_item_exprs(&input.plan, &item.alias, &item.expr)
                    .is_ok_and(|exprs| exprs.is_empty())
            })
        {
            return Ok(input);
        }
        let mut projections = match mode {
            ProjectMode::PreserveVisible => existing_columns(&input.plan, &aliases),
            ProjectMode::ReplaceScope => Vec::new(),
            ProjectMode::ReplaceCurrent => {
                let mut excluded = aliases;
                excluded.insert("current".to_string());
                existing_columns_excluding_binding(&input.plan, "current", &excluded)
            }
        };
        for item in items {
            projections.extend(self.project_item_exprs(&input.plan, &item.alias, &item.expr)?);
        }
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections)?
            .build()?;
        Ok(input.with_plan(plan))
    }

    fn lower_current_project(
        &mut self,
        expr: &IrExpr,
        fields: &[String],
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let alias = fields.first().map(String::as_str).unwrap_or("current");
        let mut projections = apply_correlation_key_columns(&input.plan)
            .iter()
            .map(col_exact)
            .collect::<Vec<_>>();
        let item_projections = self.project_item_exprs(&input.plan, alias, expr)?;
        if item_projections.is_empty() {
            return Err(RelError::Unsupported(
                "current projection produced no relational columns".into(),
            ));
        }
        projections.extend(item_projections);
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections.split_off(0))?
            .filter(col_exact(alias).is_not_null())?
            .build()?;
        Ok(input.with_plan(plan))
    }

    fn lower_join(
        &mut self,
        kind: JoinKind,
        left: &Node,
        right: &Node,
        condition: Option<&IrExpr>,
    ) -> RelResult<LoweredNode> {
        let left = self.lower_node(left)?;
        let right = self.lower_node(right)?;
        let join_type = match kind {
            JoinKind::Inner => JoinType::Inner,
            JoinKind::LeftOuter => JoinType::Left,
            JoinKind::RightOuter => JoinType::Right,
            JoinKind::FullOuter => JoinType::Full,
            JoinKind::Cross => JoinType::Inner,
        };
        let plan = if matches!(kind, JoinKind::Cross) && condition.is_none() {
            LogicalPlanBuilder::from(left.plan.clone())
                .cross_join(right.plan.clone())?
                .build()?
        } else {
            let expr = match condition {
                Some(condition) => {
                    vec![self.lower_expr_for_join(&left.plan, &right.plan, condition)?]
                }
                None => Vec::new(),
            };
            LogicalPlanBuilder::from(left.plan.clone())
                .join_on(right.plan.clone(), join_type, expr)?
                .build()?
        };
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

    fn lower_apply(
        &mut self,
        kind: ApplyKind,
        correlation: &[String],
        outputs: &[String],
        left: &Node,
        right: &Node,
    ) -> RelResult<LoweredNode> {
        let left = self.lower_node(left)?;
        let (left_plan, key_cols, cleanup) =
            with_apply_correlation_keys(left.plan.clone(), correlation)?;
        let left = left.with_plan(left_plan);
        let previous = self.correlate_plan.replace(left.plan.clone());
        let right = self.lower_node(right);
        self.correlate_plan = previous;
        let right = right?;
        match kind {
            ApplyKind::Inner => {
                let mut right = right;
                right.islands.merge(left.islands);
                if !cleanup.is_empty() {
                    let projections = existing_columns_by_name(&right.plan, &cleanup);
                    right.plan = LogicalPlanBuilder::from(right.plan)
                        .project(projections)?
                        .build()?;
                }
                Ok(right)
            }
            ApplyKind::Semi | ApplyKind::Anti => {
                self.lower_existence_apply(kind, &key_cols, cleanup, left, right)
            }
            ApplyKind::Optional | ApplyKind::Scalar => {
                self.lower_left_apply(&key_cols, outputs, cleanup, left, right)
            }
        }
    }

    fn lower_existence_apply(
        &mut self,
        kind: ApplyKind,
        key_cols: &[String],
        mut cleanup: BTreeSet<String>,
        left: LoweredNode,
        right: LoweredNode,
    ) -> RelResult<LoweredNode> {
        let (left_plan, right_plan, join_exprs, right_cleanup) =
            prepare_apply_join_inputs(left.plan.clone(), right.plan.clone(), key_cols, &[])?;
        cleanup.extend(right_cleanup);
        let join_type = match kind {
            ApplyKind::Semi => JoinType::LeftSemi,
            ApplyKind::Anti => JoinType::LeftAnti,
            _ => unreachable!("existence apply only handles semi/anti"),
        };
        let mut plan = LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, join_type, join_exprs)?
            .build()?;
        if !cleanup.is_empty() {
            let projections = existing_columns_by_name(&plan, &cleanup);
            plan = LogicalPlanBuilder::from(plan)
                .project(projections)?
                .build()?;
        }
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

    fn lower_left_apply(
        &mut self,
        key_cols: &[String],
        outputs: &[String],
        mut cleanup: BTreeSet<String>,
        left: LoweredNode,
        right: LoweredNode,
    ) -> RelResult<LoweredNode> {
        let outputs = right_apply_output_columns(&right.plan, outputs)?;
        let (left_plan, right_plan, join_exprs, right_cleanup) =
            prepare_apply_join_inputs(left.plan.clone(), right.plan.clone(), key_cols, &outputs)?;
        cleanup.extend(right_cleanup);
        let mut plan = LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, JoinType::Left, join_exprs)?
            .build()?;
        if !cleanup.is_empty() {
            let projections = existing_columns_by_name(&plan, &cleanup);
            plan = LogicalPlanBuilder::from(plan)
                .project(projections)?
                .build()?;
        }
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

    fn lower_unwind(
        &mut self,
        input_expr: &IrExpr,
        bind: &str,
        outer: bool,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let Some(values) = constant_unwind_values(input_expr, outer)? else {
            return Err(RelError::Unsupported(
                "GraphUnwind over non-constant list expression".into(),
            ));
        };
        let value_rows = values
            .into_iter()
            .map(|value| vec![value])
            .collect::<Vec<_>>();
        let values_plan = self.lower_values(&[bind.to_string()], &value_rows)?;
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .cross_join(values_plan.plan.clone())?
            .build()?;
        let mut islands = input.islands;
        islands.merge(values_plan.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    fn lower_union(
        &mut self,
        all: bool,
        align: UnionAlign,
        left: &Node,
        right: &Node,
    ) -> RelResult<LoweredNode> {
        let left = self.lower_node(left)?;
        let right = self.lower_node(right)?;
        let builder = LogicalPlanBuilder::from(left.plan.clone());
        let plan = match (all, align) {
            (true, UnionAlign::ByPosition) if self.language == Language::Gremlin => {
                builder.union_by_name(right.plan.clone())?.build()?
            }
            (false, UnionAlign::ByPosition) if self.language == Language::Gremlin => builder
                .union_by_name_distinct(right.plan.clone())?
                .build()?,
            (true, UnionAlign::ByPosition) => builder.union(right.plan.clone())?.build()?,
            (false, UnionAlign::ByPosition) => {
                builder.union_distinct(right.plan.clone())?.build()?
            }
            (true, UnionAlign::ByVariableName) => {
                builder.union_by_name(right.plan.clone())?.build()?
            }
            (false, UnionAlign::ByVariableName) => builder
                .union_by_name_distinct(right.plan.clone())?
                .build()?,
        };
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

    fn lower_coalesce(
        &mut self,
        success: CoalesceSuccess,
        _output: &str,
        correlation: &[String],
        input: &Node,
        arms: &[Node],
    ) -> RelResult<LoweredNode> {
        if success != CoalesceSuccess::FirstNonEmpty || arms.len() != 2 {
            return Err(RelError::Unsupported("GraphCoalesce".into()));
        }
        if !matches!(arms.get(1), Some(Node::GraphCorrelate { .. })) {
            return Err(RelError::Unsupported(
                "GraphCoalesce with non-pass-through fallback".into(),
            ));
        }

        let input = self.lower_node(input)?;
        let (left_plan, key_cols, cleanup) =
            with_apply_correlation_keys(input.plan.clone(), correlation)?;
        let left = input.with_plan(left_plan);

        let previous = self.correlate_plan.replace(left.plan.clone());
        let first = self.lower_node(&arms[0]);
        self.correlate_plan = previous;
        let first = first?;

        let mut matched_plan = first.plan.clone();
        if !cleanup.is_empty() {
            let projections = existing_columns_by_name(&matched_plan, &cleanup);
            matched_plan = LogicalPlanBuilder::from(matched_plan)
                .project(projections)?
                .build()?;
        }

        let (left_plan, right_plan, join_exprs, right_cleanup) =
            prepare_apply_join_inputs(left.plan.clone(), first.plan.clone(), &key_cols, &[])?;
        let mut fallback_cleanup = cleanup;
        fallback_cleanup.extend(right_cleanup);
        let mut fallback_plan = LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, JoinType::LeftAnti, join_exprs)?
            .build()?;
        if !fallback_cleanup.is_empty() {
            let projections = existing_columns_by_name(&fallback_plan, &fallback_cleanup);
            fallback_plan = LogicalPlanBuilder::from(fallback_plan)
                .project(projections)?
                .build()?;
        }

        let plan = LogicalPlanBuilder::from(matched_plan)
            .union_by_name(fallback_plan)?
            .build()?;
        let mut islands = left.islands;
        islands.merge(first.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

    fn lower_choose(
        &mut self,
        selector: &ChooseSelector,
        arms: &[ChooseArm],
        default: Option<&Node>,
        unmatched: ChooseUnmatched,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let arm_conditions = self.choose_arm_conditions(&input.plan, selector, arms)?;
        let mut branches = Vec::<LoweredNode>::new();
        let mut unmatched_condition: Option<Expr> = None;

        for (arm, condition) in arms.iter().zip(arm_conditions.iter()) {
            let filtered = LogicalPlanBuilder::from(input.plan.clone())
                .filter(condition.clone())?
                .build()?;
            branches.push(self.lower_with_correlate(filtered, &arm.body)?);
            unmatched_condition = Some(match unmatched_condition {
                Some(acc) => Expr::or(acc, condition.clone()),
                None => condition.clone(),
            });
        }

        let unmatched_filter =
            unmatched_condition.map(|condition| Expr::IsNotTrue(Box::new(condition)));
        if let Some(default) = default {
            let default_input = match unmatched_filter {
                Some(condition) => LogicalPlanBuilder::from(input.plan.clone())
                    .filter(condition)?
                    .build()?,
                None => input.plan.clone(),
            };
            branches.push(self.lower_with_correlate(default_input, default)?);
        } else if unmatched == ChooseUnmatched::PassThrough {
            let pass_input = match unmatched_filter {
                Some(condition) => LogicalPlanBuilder::from(input.plan.clone())
                    .filter(condition)?
                    .build()?,
                None => input.plan.clone(),
            };
            branches.push(LoweredNode {
                plan: pass_input,
                islands: IslandReport::default(),
                fields: input.fields.clone(),
                result_form: input.result_form,
            });
        } else if unmatched == ChooseUnmatched::Error {
            return Err(RelError::Unsupported(
                "GraphChoose unmatched=Error is not relationally lowered yet".into(),
            ));
        }

        let Some(first) = branches.first().cloned() else {
            return Ok(input);
        };
        let mut plan = first.plan;
        let mut islands = input.islands;
        islands.merge(first.islands);
        for branch in branches.into_iter().skip(1) {
            plan = LogicalPlanBuilder::from(plan)
                .union_by_name(branch.plan)?
                .build()?;
            islands.merge(branch.islands);
        }
        Ok(LoweredNode {
            plan,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    fn choose_arm_conditions(
        &self,
        plan: &LogicalPlan,
        selector: &ChooseSelector,
        arms: &[ChooseArm],
    ) -> RelResult<Vec<Expr>> {
        match selector {
            ChooseSelector::Boolean(condition) => {
                let condition = self.lower_expr(plan, condition)?;
                let mut out = Vec::with_capacity(arms.len());
                if !arms.is_empty() {
                    out.push(condition.clone());
                }
                if arms.len() >= 2 {
                    out.push(Expr::IsNotTrue(Box::new(condition)));
                }
                for _ in 2..arms.len() {
                    out.push(lit(false));
                }
                Ok(out)
            }
            ChooseSelector::Value(expr) => {
                let selector = self.lower_expr(plan, expr)?;
                arms.iter()
                    .map(|arm| {
                        let Some(key) = &arm.key else {
                            return Ok(lit(false));
                        };
                        let key = value_literal_expr(key)?;
                        Ok(binary(selector.clone(), BinaryOp::Eq, key))
                    })
                    .collect()
            }
        }
    }

    fn lower_with_correlate(&mut self, plan: LogicalPlan, node: &Node) -> RelResult<LoweredNode> {
        let previous = self.correlate_plan.replace(plan);
        let lowered = self.lower_node(node);
        self.correlate_plan = previous;
        lowered
    }

    fn return_projection(&self, plan: &LogicalPlan, fields: &[String]) -> RelResult<Vec<Expr>> {
        let mut projections = Vec::new();
        for field in fields {
            if has_exact_col(plan, field) {
                projections.push(col_exact(field));
            } else if has_binding_shape(plan, field).is_some() {
                if self.language == Language::Gremlin {
                    projections.push(gremlin_element_display_expr(plan, field)?.alias(field));
                    continue;
                }
                return Err(RelError::Unsupported(format!(
                    "returning graph element binding `{field}` as a value"
                )));
            } else {
                return Err(RelError::Unsupported(format!(
                    "return field `{field}` is not available relationally"
                )));
            }
        }
        Ok(projections)
    }

    fn project_item_exprs(
        &self,
        plan: &LogicalPlan,
        alias: &str,
        expr: &IrExpr,
    ) -> RelResult<Vec<Expr>> {
        if self.options.tolerate_internal_path_state
            && alias.starts_with("__gremlin_select_history_")
            && matches!(expr, IrExpr::Call { name, .. } if name == "select_history_append")
        {
            return Ok(Vec::new());
        }
        if self.language == Language::Gremlin
            && alias.starts_with("select_source_")
            && matches!(expr, IrExpr::Binding(binding) if binding == "current")
        {
            return Ok(Vec::new());
        }
        if let IrExpr::Binding(binding) = expr {
            if has_binding_shape(plan, binding).is_some() {
                return duplicate_binding_projection_only(plan, binding, alias);
            }
        }
        if self.options.tolerate_internal_path_state
            && alias == "__path"
            && matches!(expr, IrExpr::Call { name, .. } if name.starts_with("path_"))
        {
            return Ok(vec![lit(ScalarValue::Utf8(None)).alias(alias)]);
        }
        Ok(vec![self.lower_expr(plan, expr)?.alias(alias)])
    }

    fn lower_expr(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        match expr {
            IrExpr::Lit(lit_value) => Ok(lit_to_expr(lit_value)),
            IrExpr::Binding(binding) => {
                if has_exact_col(plan, binding) {
                    Ok(col_exact(binding))
                } else if has_binding_shape(plan, binding).is_some() {
                    if self.language == Language::Gremlin {
                        gremlin_element_display_expr(plan, binding)
                    } else {
                        Err(RelError::Unsupported(format!(
                            "element binding `{binding}` needs scalar context"
                        )))
                    }
                } else {
                    Err(RelError::Unsupported(format!(
                        "unavailable binding `{binding}`"
                    )))
                }
            }
            IrExpr::Property { binding, name, .. } => {
                let col = prop_col(binding, name);
                if has_exact_col(plan, &col) {
                    Ok(col_exact(col))
                } else if has_binding_shape(plan, binding).is_some() {
                    Ok(lit(ScalarValue::Utf8(None)))
                } else {
                    Err(RelError::Unsupported(format!(
                        "property `{binding}.{name}` without element binding"
                    )))
                }
            }
            IrExpr::Id(binding) => {
                let col = id_col(binding);
                if has_exact_col(plan, &col) {
                    Ok(col_exact(col))
                } else {
                    Err(RelError::Unsupported(format!("id({binding})")))
                }
            }
            IrExpr::Label(binding) => {
                let col = label_col(binding);
                if has_exact_col(plan, &col) {
                    Ok(col_exact(col))
                } else {
                    Err(RelError::Unsupported(format!("label({binding})")))
                }
            }
            IrExpr::HasLabel { binding, label } => {
                let col = label_col(binding);
                if has_exact_col(plan, &col) {
                    Ok(binary(
                        col_exact(col),
                        BinaryOp::Eq,
                        lit(ScalarValue::Utf8(Some(label.clone()))),
                    ))
                } else {
                    Err(RelError::Unsupported(format!(
                        "has_label({binding}, {label})"
                    )))
                }
            }
            IrExpr::Binary { op, lhs, rhs } => {
                let lhs = self.lower_expr(plan, lhs)?;
                let rhs = self.lower_expr(plan, rhs)?;
                Ok(binary(lhs, *op, rhs))
            }
            IrExpr::Not(inner) => Ok(Expr::Not(Box::new(self.lower_expr(plan, inner)?))),
            IrExpr::StringPredicate {
                op,
                target,
                pattern,
            } => {
                let target = self.lower_expr(plan, target)?;
                let IrExpr::Lit(Lit::String(pattern)) = pattern.as_ref() else {
                    return Err(RelError::Unsupported(
                        "dynamic string predicate pattern".into(),
                    ));
                };
                let like_pattern = match op {
                    StringOp::StartsWith => format!("{pattern}%"),
                    StringOp::EndsWith => format!("%{pattern}"),
                    StringOp::Contains => format!("%{pattern}%"),
                };
                Ok(target.like(lit(ScalarValue::Utf8(Some(like_pattern)))))
            }
            IrExpr::IsNull(inner) => Ok(self.lower_expr(plan, inner)?.is_null()),
            IrExpr::IsNotNull(inner) => Ok(self.lower_expr(plan, inner)?.is_not_null()),
            IrExpr::IsBound(binding) => {
                if has_exact_col(plan, binding) {
                    Ok(col_exact(binding).is_not_null())
                } else if has_binding_shape(plan, binding).is_some() {
                    Ok(col_exact(id_col(binding)).is_not_null())
                } else {
                    Ok(lit(false))
                }
            }
            IrExpr::Case { arms, otherwise } => {
                let when_then_expr = arms
                    .iter()
                    .map(|(when, then)| {
                        Ok((
                            Box::new(self.lower_expr(plan, when)?),
                            Box::new(self.lower_expr(plan, then)?),
                        ))
                    })
                    .collect::<RelResult<Vec<_>>>()?;
                let else_expr = otherwise
                    .as_ref()
                    .map(|expr| self.lower_expr(plan, expr).map(Box::new))
                    .transpose()?;
                Ok(Expr::Case(Case::new(None, when_then_expr, else_expr)))
            }
            IrExpr::Call { name, args } if name == "path_or_self" => {
                let Some(fallback) = args.get(1) else {
                    return Err(RelError::Unsupported("path_or_self arity".into()));
                };
                self.lower_expr(plan, fallback)
            }
            IrExpr::Call { name, args } if is_label_function(name) && args.len() == 1 => {
                match &args[0] {
                    IrExpr::Binding(binding) => {
                        self.lower_expr(plan, &IrExpr::Label(binding.clone()))
                    }
                    arg => self.lower_expr(plan, arg),
                }
            }
            IrExpr::Call { name, args } if is_id_function(name) && args.len() == 1 => {
                match &args[0] {
                    IrExpr::Binding(binding) => self.lower_expr(plan, &IrExpr::Id(binding.clone())),
                    arg => self.lower_expr(plan, arg),
                }
            }
            IrExpr::Call { name, args } if is_cast_function(name, args) => {
                let (value, data_type, lenient) = self.cast_parts(plan, name, args)?;
                if lenient {
                    Ok(Expr::TryCast(TryCast::new(Box::new(value), data_type)))
                } else {
                    Ok(Expr::Cast(Cast::new(Box::new(value), data_type)))
                }
            }
            IrExpr::Call { name, args } if is_mod_function(name) && args.len() == 2 => {
                Ok(Expr::BinaryExpr(BinaryExpr::new(
                    Box::new(self.lower_expr(plan, &args[0])?),
                    Operator::Modulo,
                    Box::new(self.lower_expr(plan, &args[1])?),
                )))
            }
            IrExpr::Call { name, args } if is_abs_function(name) && args.len() == 1 => {
                Ok(df_math::abs(self.lower_expr(plan, &args[0])?))
            }
            IrExpr::Call { name, args } if is_pow_function(name) && args.len() == 2 => {
                Ok(df_math::power(
                    self.lower_expr(plan, &args[0])?,
                    self.lower_expr(plan, &args[1])?,
                ))
            }
            IrExpr::Call { name, args } if is_date_function(name) && args.len() == 1 => {
                Ok(cast_utf8(self.lower_expr(plan, &args[0])?))
            }
            IrExpr::Call { name, args } if is_exists_function(name) && args.len() == 1 => {
                Ok(self.lower_expr(plan, &args[0])?.is_not_null())
            }
            IrExpr::Call { name, args } if is_in_function(name) && args.len() == 2 => {
                let expr = self.lower_expr(plan, &args[0])?;
                let IrExpr::List(values) = &args[1] else {
                    return Err(RelError::Unsupported("dynamic IN list".into()));
                };
                let list = values
                    .iter()
                    .map(|value| self.lower_expr(plan, value))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(Expr::InList(InList::new(Box::new(expr), list, false)))
            }
            IrExpr::Call { name, args } if name == "typeof_matches" && args.len() == 2 => {
                let IrExpr::Lit(Lit::String(type_name)) = &args[1] else {
                    return Err(RelError::Unsupported("dynamic typeOf target".into()));
                };
                self.lower_typeof_matches(plan, &args[0], type_name)
            }
            IrExpr::Call { name, args } if name == "map_has_key" && args.len() == 2 => {
                Ok(lit(false))
            }
            IrExpr::Call { name, args }
                if name == "select_key_or_binding_pop" && args.len() == 5 =>
            {
                self.lower_select_key_or_binding(plan, &args[1])
            }
            IrExpr::Call { name, args } if name == "make_map" => self.lower_make_map(plan, args),
            IrExpr::Call { name, args } if name.starts_with("cypher_") && args.len() == 2 => {
                let op = match name.as_str() {
                    "cypher_eq" => BinaryOp::Eq,
                    "cypher_neq" => BinaryOp::Neq,
                    "cypher_lt" => BinaryOp::Lt,
                    "cypher_lte" => BinaryOp::Lte,
                    "cypher_gt" => BinaryOp::Gt,
                    "cypher_gte" => BinaryOp::Gte,
                    _ => {
                        return Err(RelError::Unsupported(format!(
                            "function `{name}` is not relationally lowered yet"
                        )));
                    }
                };
                Ok(binary(
                    self.lower_expr(plan, &args[0])?,
                    op,
                    self.lower_expr(plan, &args[1])?,
                ))
            }
            IrExpr::Call { name, .. } => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
            other => Err(RelError::Unsupported(format!(
                "expression `{other:?}` is not relationally lowered yet"
            ))),
        }
    }

    fn cast_parts(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<(Expr, DataType, bool)> {
        let normalized = name.to_ascii_lowercase();
        let (value_arg, target_name, lenient) = if normalized == "cast" {
            let [value, target] = args else {
                return Err(RelError::Unsupported("cast arity".into()));
            };
            let IrExpr::Lit(Lit::String(target_name)) = target else {
                return Err(RelError::Unsupported("dynamic cast target".into()));
            };
            (value, target_name.as_str(), false)
        } else {
            let [value] = args else {
                return Err(RelError::Unsupported(format!("{name} arity")));
            };
            let lenient = matches!(
                normalized.as_str(),
                "tointeger" | "tofloat" | "toboolean" | "tostring"
            );
            (value, cast_target_from_function_name(&normalized)?, lenient)
        };
        let value = self.lower_expr(plan, value_arg)?;
        let data_type = data_type_for_cast_target(target_name)?;
        Ok((value, data_type, lenient))
    }

    fn lower_typeof_matches(
        &self,
        plan: &LogicalPlan,
        target: &IrExpr,
        type_name: &str,
    ) -> RelResult<Expr> {
        let normalized = normalize_type_name(type_name);
        if let IrExpr::Binding(binding) = target
            && let Some(shape) = has_binding_shape(plan, binding)
        {
            let matched = match shape {
                BindingShape::Node => matches!(normalized.as_str(), "vertex" | "node"),
                BindingShape::Edge => matches!(normalized.as_str(), "edge" | "relationship"),
            };
            return Ok(lit(matched));
        }
        let expr = self.lower_expr(plan, target)?;
        let data_type = expr.get_type(plan.schema())?;
        Ok(lit(data_type_matches_gremlin_type(&data_type, &normalized)))
    }

    fn lower_expr_for_join(
        &self,
        left: &LogicalPlan,
        right: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Expr> {
        let joined = LogicalPlanBuilder::from(left.clone())
            .cross_join(right.clone())?
            .build()?;
        self.lower_expr(&joined, expr)
    }

    fn lower_select_key_or_binding(
        &self,
        plan: &LogicalPlan,
        binding_arg: &IrExpr,
    ) -> RelResult<Expr> {
        let IrExpr::Binding(binding) = binding_arg else {
            return Err(RelError::Unsupported(
                "dynamic Gremlin select binding".into(),
            ));
        };
        if has_exact_col(plan, binding) {
            Ok(col_exact(binding))
        } else if has_binding_shape(plan, binding).is_some() {
            gremlin_element_display_expr(plan, binding)
        } else {
            Err(RelError::Unsupported(format!(
                "Gremlin select binding `{binding}` is not available relationally"
            )))
        }
    }

    fn lower_make_map(&self, plan: &LogicalPlan, args: &[IrExpr]) -> RelResult<Expr> {
        if args.len() % 2 != 0 {
            return Err(RelError::Unsupported("make_map arity".into()));
        }
        let mut pieces = Vec::new();
        pieces.push(lit("Map({"));
        for (idx, pair) in args.chunks(2).enumerate() {
            let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
                return Err(RelError::Unsupported("dynamic make_map key".into()));
            };
            if idx > 0 {
                pieces.push(lit(", "));
            }
            pieces.push(lit(format!("\"{}\": String(\"", escape_debug_string(key))));
            pieces.push(cast_utf8(self.lower_expr(plan, &pair[1])?));
            pieces.push(lit("\")"));
        }
        pieces.push(lit("})"));
        Ok(concat_exprs(pieces))
    }

    fn sort_exprs(
        &self,
        plan: &LogicalPlan,
        key: &crate::ir::plan::SortKey,
    ) -> RelResult<Vec<datafusion::logical_expr::SortExpr>> {
        let asc = matches!(key.dir, SortDir::Asc);
        let nulls_first = matches!(key.nulls, NullsOrder::First);
        if let IrExpr::Call { name, args } = &key.expr
            && name == "gremlin_scan_order"
            && let Some(IrExpr::Binding(binding)) = args.first()
        {
            return Ok(vec![
                col_exact(label_col(binding)).sort(asc, nulls_first),
                col_exact(id_col(binding)).sort(asc, nulls_first),
            ]);
        }
        self.lower_expr(plan, &key.expr)
            .map(|expr| vec![expr.sort(asc, nulls_first)])
    }

    fn scan_batches(&mut self, prefix: &str, batches: Vec<RecordBatch>) -> RelResult<LoweredNode> {
        let schema = batches
            .first()
            .map(RecordBatch::schema)
            .unwrap_or_else(|| Arc::new(Schema::empty()));
        let provider = Arc::new(MemTable::try_new(schema, vec![batches])?);
        let table_name = format!("__graph_rel_{}_{}", prefix, self.scan_counter);
        self.scan_counter += 1;
        let plan =
            LogicalPlanBuilder::scan(table_name, provider_as_source(provider), None)?.build()?;
        Ok(LoweredNode::new(plan))
    }

    fn node_labels(&self, labels: &LabelExpr) -> RelResult<Vec<String>> {
        let mut out = match labels {
            LabelExpr::Any => self.graph.labels(),
            LabelExpr::AnyOf(labels) => labels.clone(),
            LabelExpr::AllOf(labels) if labels.len() == 1 => labels.clone(),
            LabelExpr::AllOf(labels) => {
                return Err(RelError::Unsupported(format!(
                    "multi-label node scan {labels:?}"
                )));
            }
            LabelExpr::Not(_) => return Err(RelError::Unsupported("negated label scan".into())),
        };
        out.sort();
        out.dedup();
        Ok(out)
    }

    fn rel_types(&self, types: &LabelExpr) -> RelResult<Vec<String>> {
        let mut out = match types {
            LabelExpr::Any => self.graph.rel_types(),
            LabelExpr::AnyOf(types) => types.clone(),
            LabelExpr::AllOf(types) if types.len() == 1 => types.clone(),
            LabelExpr::AllOf(types) => {
                return Err(RelError::Unsupported(format!(
                    "multi-type relationship scan {types:?}"
                )));
            }
            LabelExpr::Not(_) => {
                return Err(RelError::Unsupported(
                    "negated relationship type scan".into(),
                ));
            }
        };
        out.sort();
        out.dedup();
        Ok(out)
    }

    fn node_property_defs(&self, labels: &[String]) -> RelResult<Vec<PropertyDef>> {
        let mut defs = BTreeMap::<String, DataType>::new();
        for label in labels {
            let table = self.graph.node_table(label)?;
            merge_property_defs(&mut defs, table.batch.schema().as_ref(), &["id"])?;
        }
        Ok(defs
            .into_iter()
            .map(|(name, data_type)| PropertyDef { name, data_type })
            .collect())
    }

    fn edge_property_defs(&self, rel_types: &[String]) -> RelResult<Vec<PropertyDef>> {
        let mut defs = BTreeMap::<String, DataType>::new();
        for rel_type in rel_types {
            for table in self.graph.edge_tables(rel_type)? {
                merge_property_defs(
                    &mut defs,
                    table.batch.schema().as_ref(),
                    &["src", "dst", "id", "__src_id", "__dst_id"],
                )?;
            }
        }
        Ok(defs
            .into_iter()
            .map(|(name, data_type)| PropertyDef { name, data_type })
            .collect())
    }
}

impl LoweredNode {
    fn new(plan: LogicalPlan) -> Self {
        Self {
            plan,
            islands: IslandReport {
                lowerable_nodes: 1,
                unsupported: Vec::new(),
            },
            fields: None,
            result_form: None,
        }
    }

    fn with_plan(self, plan: LogicalPlan) -> Self {
        Self {
            plan,
            islands: self.islands,
            fields: self.fields,
            result_form: self.result_form,
        }
    }
}

impl IslandReport {
    fn merge(&mut self, other: IslandReport) {
        self.lowerable_nodes += other.lowerable_nodes;
        self.unsupported.extend(other.unsupported);
    }
}

#[derive(Debug, Clone)]
struct PropertyDef {
    name: String,
    data_type: DataType,
}

fn merge_property_defs(
    defs: &mut BTreeMap<String, DataType>,
    schema: &Schema,
    excluded: &[&str],
) -> RelResult<()> {
    for field in schema.fields() {
        if excluded.contains(&field.name().as_str()) {
            continue;
        }
        match defs.get(field.name()) {
            Some(existing) if existing != field.data_type() => {
                return Err(RelError::Unsupported(format!(
                    "property `{}` has mixed types `{existing:?}` and `{:?}`",
                    field.name(),
                    field.data_type()
                )));
            }
            Some(_) => {}
            None => {
                defs.insert(field.name().clone(), field.data_type().clone());
            }
        }
    }
    Ok(())
}

fn node_schema(binding: &str, props: &[PropertyDef]) -> SchemaRef {
    let mut fields = vec![
        Field::new(id_col(binding), DataType::Int64, false),
        Field::new(label_col(binding), DataType::Utf8, false),
    ];
    for prop in props {
        fields.push(Field::new(
            prop_col(binding, &prop.name),
            prop.data_type.clone(),
            true,
        ));
    }
    Arc::new(Schema::new(fields))
}

fn edge_schema(binding: &str, props: &[PropertyDef]) -> SchemaRef {
    let mut fields = vec![
        Field::new(id_col(binding), DataType::Int64, false),
        Field::new(label_col(binding), DataType::Utf8, false),
        Field::new(src_label_col(binding), DataType::Utf8, false),
        Field::new(src_id_col(binding), DataType::Int64, false),
        Field::new(dst_label_col(binding), DataType::Utf8, false),
        Field::new(dst_id_col(binding), DataType::Int64, false),
    ];
    for prop in props {
        fields.push(Field::new(
            prop_col(binding, &prop.name),
            prop.data_type.clone(),
            true,
        ));
    }
    Arc::new(Schema::new(fields))
}

fn normalize_node_table(
    binding: &str,
    table: &NodeTable,
    props: &[PropertyDef],
    schema: SchemaRef,
) -> RelResult<RecordBatch> {
    let rows = table.batch.num_rows();
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from_iter_values(
            (0..rows).map(|row| row as i64),
        )),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.label.as_str()),
        )),
    ];
    for prop in props {
        arrays.push(property_array(
            &table.batch,
            &prop.name,
            &prop.data_type,
            rows,
        )?);
    }
    debug_assert_eq!(schema.field(0).name(), &id_col(binding));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

fn normalize_edge_table(
    binding: &str,
    table: &EdgeTable,
    base_id: i64,
    props: &[PropertyDef],
    schema: SchemaRef,
) -> RelResult<RecordBatch> {
    let rows = table.batch.num_rows();
    let src =
        table.batch.schema().index_of("__src_id").map_err(|_| {
            CatalogError::Schema(format!("edge `{}` missing __src_id", table.rel_type))
        })?;
    let dst =
        table.batch.schema().index_of("__dst_id").map_err(|_| {
            CatalogError::Schema(format!("edge `{}` missing __dst_id", table.rel_type))
        })?;
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from_iter_values(
            (0..rows).map(|row| base_id + row as i64),
        )),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.rel_type.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.src_label.as_str()),
        )),
        table.batch.column(src).clone(),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.dst_label.as_str()),
        )),
        table.batch.column(dst).clone(),
    ];
    for prop in props {
        arrays.push(property_array(
            &table.batch,
            &prop.name,
            &prop.data_type,
            rows,
        )?);
    }
    debug_assert_eq!(schema.field(0).name(), &id_col(binding));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

fn property_array(
    batch: &RecordBatch,
    name: &str,
    expected: &DataType,
    rows: usize,
) -> RelResult<ArrayRef> {
    match schema_index(batch.schema().as_ref(), name) {
        Some(idx) => {
            let schema = batch.schema();
            let field = schema.field(idx);
            if field.data_type() != expected {
                return Err(RelError::Unsupported(format!(
                    "property `{name}` has type {:?}, expected {expected:?}",
                    field.data_type()
                )));
            }
            Ok(batch.column(idx).clone())
        }
        None => Ok(new_null_array(expected, rows)),
    }
}

fn schema_index(schema: &Schema, name: &str) -> Option<usize> {
    schema.index_of(name).ok().or_else(|| {
        let mut matches = schema
            .fields()
            .iter()
            .enumerate()
            .filter_map(|(idx, field)| field.name().eq_ignore_ascii_case(name).then_some(idx));
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

fn values_batch(bindings: &[String], rows: &[Vec<Value>]) -> RelResult<RecordBatch> {
    if rows.iter().any(|row| row.len() != bindings.len()) {
        return Err(RelError::Unsupported(
            "GraphValues row width does not match bindings".into(),
        ));
    }
    let types = (0..bindings.len())
        .map(|idx| infer_value_type(rows.iter().map(|row| &row[idx])))
        .collect::<RelResult<Vec<_>>>()?;
    let fields = bindings
        .iter()
        .zip(types.iter())
        .map(|(binding, data_type)| Field::new(binding, data_type.clone(), true))
        .collect::<Vec<_>>();
    let schema = Arc::new(Schema::new(fields));
    let arrays = types
        .iter()
        .enumerate()
        .map(|(idx, data_type)| values_array(rows.iter().map(|row| &row[idx]), data_type))
        .collect::<RelResult<Vec<_>>>()?;
    Ok(RecordBatch::try_new(schema, arrays)?)
}

fn infer_value_type<'a>(values: impl Iterator<Item = &'a Value>) -> RelResult<DataType> {
    let mut data_type = DataType::Utf8;
    for value in values {
        match value {
            Value::Null => {}
            Value::Bool(_) => data_type = promote_type(data_type, DataType::Boolean)?,
            Value::Byte(_)
            | Value::UInt8(_)
            | Value::Short(_)
            | Value::UInt16(_)
            | Value::Int(_)
            | Value::UInt32(_)
            | Value::Long(_)
            | Value::UInt64(_) => data_type = promote_type(data_type, DataType::Int64)?,
            Value::Float32(_) | Value::Float(_) => {
                data_type = promote_type(data_type, DataType::Float64)?
            }
            Value::String(_) | Value::DateTime(_) => {
                data_type = promote_type(data_type, DataType::Utf8)?
            }
            other => {
                return Err(RelError::Unsupported(format!(
                    "GraphValues value type `{}`",
                    other.type_name()
                )));
            }
        }
    }
    Ok(data_type)
}

fn promote_type(current: DataType, next: DataType) -> RelResult<DataType> {
    match (&current, &next) {
        (DataType::Utf8, _) => Ok(next),
        (_, DataType::Utf8) => Ok(current),
        (DataType::Int64, DataType::Float64) => Ok(DataType::Float64),
        (DataType::Float64, DataType::Int64) => Ok(DataType::Float64),
        _ if current == next => Ok(current),
        _ => Err(RelError::Unsupported(format!(
            "mixed GraphValues types `{current:?}` and `{next:?}`"
        ))),
    }
}

fn values_array<'a>(
    values: impl Iterator<Item = &'a Value>,
    data_type: &DataType,
) -> RelResult<ArrayRef> {
    match data_type {
        DataType::Boolean => {
            let mut builder = BooleanBuilder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    Value::Bool(value) => builder.append_value(*value),
                    other => {
                        return Err(RelError::Unsupported(format!(
                            "cannot put `{}` in Boolean GraphValues column",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Int64 => {
            let mut builder = Int64Builder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    _ => match value.as_i64() {
                        Some(value) => builder.append_value(value),
                        None => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in Int64 GraphValues column",
                                value.type_name()
                            )));
                        }
                    },
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Float64 => {
            let mut builder = Float64Builder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    Value::Float(value) => builder.append_value(*value),
                    Value::Float32(value) => builder.append_value(*value as f64),
                    _ => match value.as_i64() {
                        Some(value) => builder.append_value(value as f64),
                        None => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in Float64 GraphValues column",
                                value.type_name()
                            )));
                        }
                    },
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Utf8 => {
            let mut builder = StringBuilder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    Value::String(value) | Value::DateTime(value) => builder.append_value(value),
                    other => {
                        return Err(RelError::Unsupported(format!(
                            "cannot put `{}` in Utf8 GraphValues column",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        other => Err(RelError::Unsupported(format!(
            "GraphValues type `{other:?}`"
        ))),
    }
}

fn lit_to_expr(value: &Lit) -> Expr {
    match value {
        Lit::Null => lit(ScalarValue::Utf8(None)),
        Lit::Bool(value) => lit(*value),
        Lit::Int(value) => lit(*value),
        Lit::Float(value) => lit(*value),
        Lit::String(value) => lit(value.clone()),
    }
}

fn value_literal_expr(value: &Value) -> RelResult<Expr> {
    match value {
        Value::Null => Ok(lit(ScalarValue::Utf8(None))),
        Value::Bool(value) => Ok(lit(*value)),
        Value::Byte(value) => Ok(lit(*value as i64)),
        Value::UInt8(value) => Ok(lit(*value as i64)),
        Value::Short(value) => Ok(lit(*value as i64)),
        Value::UInt16(value) => Ok(lit(*value as i64)),
        Value::Int(value) | Value::Long(value) => Ok(lit(*value)),
        Value::UInt32(value) => Ok(lit(*value as i64)),
        Value::UInt64(value) => i64::try_from(*value)
            .map(lit)
            .map_err(|_| RelError::Unsupported("UINT64 choose key overflows INT64".into())),
        Value::Float32(value) => Ok(lit(*value as f64)),
        Value::Float(value) => Ok(lit(*value)),
        Value::String(value) | Value::DateTime(value) => Ok(lit(value.clone())),
        other => Err(RelError::Unsupported(format!(
            "GraphChoose key type `{}` is not relationally lowered yet",
            other.type_name()
        ))),
    }
}

fn lit_to_value(value: &Lit) -> Value {
    match value {
        Lit::Null => Value::Null,
        Lit::Bool(value) => Value::Bool(*value),
        Lit::Int(value) => Value::Int(*value),
        Lit::Float(value) => Value::Float(*value),
        Lit::String(value) => Value::String(value.clone()),
    }
}

fn constant_unwind_values(expr: &IrExpr, outer: bool) -> RelResult<Option<Vec<Value>>> {
    let mut values = match expr {
        IrExpr::Lit(Lit::Null) => Vec::new(),
        IrExpr::Lit(value) => vec![lit_to_value(value)],
        IrExpr::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let IrExpr::Lit(value) = item else {
                    return Ok(None);
                };
                values.push(lit_to_value(value));
            }
            values
        }
        IrExpr::Call { name, args } if name.eq_ignore_ascii_case("range") => {
            constant_range_values(args)?
        }
        _ => return Ok(None),
    };
    if values.is_empty() && outer {
        values.push(Value::Null);
    }
    Ok(Some(values))
}

fn constant_range_values(args: &[IrExpr]) -> RelResult<Vec<Value>> {
    let ([start, stop] | [start, stop, _]) = args else {
        return Err(RelError::Unsupported("range arity".into()));
    };
    let start = literal_i64(start)
        .ok_or_else(|| RelError::Unsupported("range start must be a literal integer".into()))?;
    let stop = literal_i64(stop)
        .ok_or_else(|| RelError::Unsupported("range stop must be a literal integer".into()))?;
    let step = if let Some(step) = args.get(2) {
        literal_i64(step)
            .ok_or_else(|| RelError::Unsupported("range step must be a literal integer".into()))?
    } else if start <= stop {
        1
    } else {
        -1
    };
    if step == 0 {
        return Err(RelError::Unsupported("range step cannot be zero".into()));
    }
    let mut values = Vec::new();
    let mut current = start;
    while (step > 0 && current <= stop) || (step < 0 && current >= stop) {
        if values.len() > 100_000 {
            return Err(RelError::Unsupported(
                "range literal is too large for eager relational expansion".into(),
            ));
        }
        values.push(Value::Int(current));
        current = current.saturating_add(step);
        if (step > 0 && current == i64::MAX) || (step < 0 && current == i64::MIN) {
            break;
        }
    }
    Ok(values)
}

fn literal_i64(expr: &IrExpr) -> Option<i64> {
    match expr {
        IrExpr::Lit(Lit::Int(value)) => Some(*value),
        _ => None,
    }
}

fn binary(lhs: Expr, op: BinaryOp, rhs: Expr) -> Expr {
    let op = match op {
        BinaryOp::Eq => Operator::Eq,
        BinaryOp::Neq => Operator::NotEq,
        BinaryOp::Lt => Operator::Lt,
        BinaryOp::Lte => Operator::LtEq,
        BinaryOp::Gt => Operator::Gt,
        BinaryOp::Gte => Operator::GtEq,
        BinaryOp::Add => Operator::Plus,
        BinaryOp::Sub => Operator::Minus,
        BinaryOp::Mul => Operator::Multiply,
        BinaryOp::Div => Operator::Divide,
        BinaryOp::And => Operator::And,
        BinaryOp::Or => Operator::Or,
    };
    Expr::BinaryExpr(BinaryExpr::new(Box::new(lhs), op, Box::new(rhs)))
}

fn string_concat(lhs: Expr, rhs: Expr) -> Expr {
    Expr::BinaryExpr(BinaryExpr::new(
        Box::new(lhs),
        Operator::StringConcat,
        Box::new(rhs),
    ))
}

fn concat_exprs(mut exprs: Vec<Expr>) -> Expr {
    let first = exprs.remove(0);
    exprs.into_iter().fold(first, string_concat)
}

fn cast_utf8(expr: Expr) -> Expr {
    Expr::Cast(Cast::new(Box::new(expr), DataType::Utf8))
}

fn binding_pair_eq(binding: &str, id_column: &str, label_column: &str) -> Expr {
    Expr::and(
        binary(
            col_exact(id_col(binding)),
            BinaryOp::Eq,
            col_exact(id_column),
        ),
        binary(
            col_exact(label_col(binding)),
            BinaryOp::Eq,
            col_exact(label_column),
        ),
    )
}

fn gremlin_element_display_expr(plan: &LogicalPlan, binding: &str) -> RelResult<Expr> {
    let Some(_) = has_binding_shape(plan, binding) else {
        return Err(RelError::Unsupported(format!(
            "binding `{binding}` is not an element binding"
        )));
    };
    Ok(string_concat(
        string_concat(col_exact(label_col(binding)), lit("#")),
        cast_utf8(col_exact(id_col(binding))),
    ))
}

fn escape_debug_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_label_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("label")
}

fn is_id_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("id") || name.eq_ignore_ascii_case("ID")
}

fn is_mod_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("mod")
}

fn is_abs_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("abs")
}

fn is_pow_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("pow") || name.eq_ignore_ascii_case("power")
}

fn is_date_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("date") || name.eq_ignore_ascii_case("to_date")
}

fn is_exists_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("exists")
}

fn is_in_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("in")
}

fn is_cast_function(name: &str, args: &[IrExpr]) -> bool {
    let normalized = name.to_ascii_lowercase();
    (normalized == "cast" && args.len() == 2) || cast_target_from_function_name(&normalized).is_ok()
}

fn cast_target_from_function_name(name: &str) -> RelResult<&'static str> {
    match name {
        "tointeger" => Ok("INT64"),
        "tofloat" => Ok("DOUBLE"),
        "toboolean" => Ok("BOOL"),
        "tostring" => Ok("STRING"),
        "to_bool" | "to_boolean" => Ok("BOOL"),
        "to_string" | "string" | "cast_string" => Ok("STRING"),
        "cast_byte" => Ok("INT8"),
        "cast_short" => Ok("INT16"),
        "to_int8" => Ok("INT8"),
        "to_int16" => Ok("INT16"),
        "to_int32" | "cast_int" => Ok("INT32"),
        "to_int64" | "to_serial" | "cast_long" => Ok("INT64"),
        "to_uint8" => Ok("UINT8"),
        "to_uint16" => Ok("UINT16"),
        "to_uint32" => Ok("UINT32"),
        "to_uint64" => Ok("UINT64"),
        "to_float" | "cast_float" => Ok("FLOAT"),
        "to_double" | "cast_double" => Ok("DOUBLE"),
        "cast_bool" | "cast_boolean" => Ok("BOOL"),
        _ => Err(RelError::Unsupported(format!(
            "function `{name}` is not relationally lowered yet"
        ))),
    }
}

fn data_type_for_cast_target(type_name: &str) -> RelResult<DataType> {
    let normalized = type_name
        .trim()
        .trim_matches('"')
        .to_ascii_uppercase()
        .replace(' ', "");
    match normalized.as_str() {
        "BOOL" | "BOOLEAN" => Ok(DataType::Boolean),
        "INT8" => Ok(DataType::Int8),
        "INT16" => Ok(DataType::Int16),
        "INT32" => Ok(DataType::Int32),
        "INT64" | "SERIAL" => Ok(DataType::Int64),
        "UINT8" => Ok(DataType::UInt8),
        "UINT16" => Ok(DataType::UInt16),
        "UINT32" => Ok(DataType::UInt32),
        "UINT64" => Ok(DataType::UInt64),
        "FLOAT" => Ok(DataType::Float32),
        "DOUBLE" | "FLOAT64" => Ok(DataType::Float64),
        "STRING" | "VARCHAR" => Ok(DataType::Utf8),
        other => Err(RelError::Unsupported(format!(
            "cast target `{other}` is not relationally lowered yet"
        ))),
    }
}

fn normalize_type_name(type_name: &str) -> String {
    type_name
        .trim()
        .trim_start_matches("GType.")
        .trim_start_matches("java.lang.")
        .trim_start_matches("java.math.")
        .to_ascii_lowercase()
}

fn data_type_matches_gremlin_type(data_type: &DataType, type_name: &str) -> bool {
    match data_type {
        DataType::Null => type_name == "null",
        DataType::Boolean => matches!(type_name, "boolean" | "bool"),
        DataType::Int8 => type_name == "byte",
        DataType::UInt8 => matches!(type_name, "uint8" | "byte"),
        DataType::Int16 => type_name == "short",
        DataType::UInt16 => type_name == "uint16",
        DataType::Int32 => matches!(type_name, "int" | "integer"),
        DataType::UInt32 => type_name == "uint32",
        DataType::Int64 => matches!(type_name, "long" | "int" | "integer"),
        DataType::UInt64 => type_name == "uint64",
        DataType::Float32 => type_name == "float",
        DataType::Float64 => type_name == "double",
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
            matches!(type_name, "string" | "char" | "character")
        }
        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
            matches!(type_name, "list" | "set" | "graph")
        }
        _ => false,
    }
}

fn col_exact(name: impl Into<String>) -> Expr {
    Expr::Column(Column::new_unqualified(name.into()))
}

fn id_col(binding: &str) -> String {
    format!("{binding}{ID_SUFFIX}")
}

fn label_col(binding: &str) -> String {
    format!("{binding}{LABEL_SUFFIX}")
}

fn src_id_col(binding: &str) -> String {
    format!("{binding}{SRC_ID_SUFFIX}")
}

fn src_label_col(binding: &str) -> String {
    format!("{binding}{SRC_LABEL_SUFFIX}")
}

fn dst_id_col(binding: &str) -> String {
    format!("{binding}{DST_ID_SUFFIX}")
}

fn dst_label_col(binding: &str) -> String {
    format!("{binding}{DST_LABEL_SUFFIX}")
}

fn prop_col(binding: &str, property: &str) -> String {
    format!("{binding}{PROP_MARKER}{property}")
}

fn output_fields(plan: &LogicalPlan) -> Vec<String> {
    plan.schema()
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect()
}

fn has_exact_col(plan: &LogicalPlan, name: &str) -> bool {
    plan.schema()
        .fields()
        .iter()
        .any(|field| field.name() == name)
}

fn has_binding_shape(plan: &LogicalPlan, binding: &str) -> Option<BindingShape> {
    if has_exact_col(plan, &id_col(binding)) && has_exact_col(plan, &label_col(binding)) {
        if has_exact_col(plan, &src_id_col(binding)) && has_exact_col(plan, &dst_id_col(binding)) {
            Some(BindingShape::Edge)
        } else {
            Some(BindingShape::Node)
        }
    } else {
        None
    }
}

fn projection_aliases(items: &[ProjectionItem]) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    for item in items {
        aliases.insert(item.alias.clone());
        aliases.insert(id_col(&item.alias));
        aliases.insert(label_col(&item.alias));
        aliases.insert(src_id_col(&item.alias));
        aliases.insert(src_label_col(&item.alias));
        aliases.insert(dst_id_col(&item.alias));
        aliases.insert(dst_label_col(&item.alias));
    }
    aliases
}

fn existing_columns(plan: &LogicalPlan, excluded: &BTreeSet<String>) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| !excluded.contains(field.name()))
        .map(|field| col_exact(field.name()))
        .collect()
}

fn existing_columns_by_name(plan: &LogicalPlan, excluded: &BTreeSet<String>) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| !excluded.contains(field.name()))
        .map(|field| col_exact(field.name()))
        .collect()
}

fn apply_correlation_key_columns(plan: &LogicalPlan) -> Vec<String> {
    plan.schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .filter(|name| name.starts_with("__apply_corr_key_"))
        .collect()
}

fn partitioned_limit(
    plan: LogicalPlan,
    partition_cols: &[String],
    offset: u64,
    fetch: Option<u64>,
) -> RelResult<LogicalPlan> {
    if partition_cols.is_empty() {
        return LogicalPlanBuilder::from(plan)
            .limit(offset as usize, fetch.map(|n| n as usize))?
            .build()
            .map_err(RelError::from);
    }
    let row_number = unique_internal_alias(&plan, &BTreeSet::new(), "__apply_row_number");
    let partition_by = partition_cols.iter().map(col_exact).collect::<Vec<_>>();
    let mut predicate = binary(col_exact(&row_number), BinaryOp::Gt, lit(offset));
    if let Some(fetch) = fetch {
        predicate = Expr::and(
            predicate,
            binary(col_exact(&row_number), BinaryOp::Lte, lit(offset + fetch)),
        );
    }
    let window = df_window::row_number()
        .partition_by(partition_by)
        .build()?
        .alias(row_number.clone());
    let mut cleanup = BTreeSet::new();
    cleanup.insert(row_number);
    let windowed = LogicalPlanBuilder::from(plan)
        .window(vec![window])?
        .filter(predicate)?
        .build()?;
    let projections = existing_columns_by_name(&windowed, &cleanup);
    LogicalPlanBuilder::from(windowed)
        .project(projections)?
        .build()
        .map_err(RelError::from)
}

fn correlation_key_columns(plan: &LogicalPlan, bindings: &[String]) -> RelResult<Vec<String>> {
    let mut out = Vec::new();
    for binding in bindings {
        if has_exact_col(plan, binding) {
            out.push(binding.clone());
        } else if binding.starts_with("__") {
            continue;
        } else if has_binding_shape(plan, binding).is_some() {
            out.push(id_col(binding));
            out.push(label_col(binding));
        } else {
            return Err(RelError::Unsupported(format!(
                "apply correlation `{binding}` is not available relationally"
            )));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn unique_internal_alias(
    plan: &LogicalPlan,
    reserved: &BTreeSet<String>,
    base: impl AsRef<str>,
) -> String {
    let base = base.as_ref();
    if !has_exact_col(plan, base) && !reserved.contains(base) {
        return base.to_string();
    }
    for suffix in 1.. {
        let candidate = format!("{base}_{suffix}");
        if !has_exact_col(plan, &candidate) && !reserved.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded alias search")
}

fn with_apply_correlation_keys(
    plan: LogicalPlan,
    correlation: &[String],
) -> RelResult<(LogicalPlan, Vec<String>, BTreeSet<String>)> {
    let key_cols = correlation_key_columns(&plan, correlation)?;
    if key_cols.is_empty() {
        return Ok((plan, Vec::new(), BTreeSet::new()));
    }
    let mut cleanup = BTreeSet::new();
    let mut projections = existing_columns(&plan, &BTreeSet::new());
    let mut aliases = Vec::with_capacity(key_cols.len());
    for (idx, key) in key_cols.iter().enumerate() {
        let alias = unique_internal_alias(&plan, &cleanup, format!("__apply_corr_key_{idx}"));
        cleanup.insert(alias.clone());
        projections.push(col_exact(key).alias(alias.clone()));
        aliases.push(alias);
    }
    let plan = LogicalPlanBuilder::from(plan)
        .project(projections)?
        .build()?;
    Ok((plan, aliases, cleanup))
}

fn right_apply_output_columns(right: &LogicalPlan, outputs: &[String]) -> RelResult<Vec<String>> {
    let mut out = Vec::new();
    for output in outputs {
        if output.starts_with("__") {
            continue;
        }
        if has_exact_col(right, output) {
            out.push(output.clone());
        } else if has_binding_shape(right, output).is_some() {
            out.extend(binding_column_names(right, output)?);
        } else {
            return Err(RelError::Unsupported(format!(
                "apply output `{output}` is not available relationally"
            )));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn binding_column_names(plan: &LogicalPlan, binding: &str) -> RelResult<Vec<String>> {
    let Some(_) = has_binding_shape(plan, binding) else {
        return Err(RelError::Unsupported(format!(
            "binding `{binding}` is not an element binding"
        )));
    };
    Ok(plan
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .filter(|name| is_binding_column(name, binding))
        .collect())
}

fn prepare_apply_join_inputs(
    left: LogicalPlan,
    right: LogicalPlan,
    key_cols: &[String],
    output_cols: &[String],
) -> RelResult<(LogicalPlan, LogicalPlan, Vec<Expr>, BTreeSet<String>)> {
    let mut cleanup = BTreeSet::new();
    let (left, key_pairs) = if key_cols.is_empty() {
        let left_key = unique_internal_alias(&left, &cleanup, "__apply_left_key_0");
        cleanup.insert(left_key.clone());
        let right_key = unique_internal_alias(&right, &cleanup, "__apply_right_key_0");
        cleanup.insert(right_key.clone());
        let mut projections = existing_columns(&left, &BTreeSet::new());
        projections.push(lit(1_i64).alias(left_key.clone()));
        let left = LogicalPlanBuilder::from(left)
            .project(projections)?
            .build()?;
        (left, vec![(left_key, right_key)])
    } else {
        (
            left,
            key_cols
                .iter()
                .enumerate()
                .map(|(idx, key)| {
                    let alias =
                        unique_internal_alias(&right, &cleanup, format!("__apply_right_key_{idx}"));
                    cleanup.insert(alias.clone());
                    (key.clone(), alias)
                })
                .collect::<Vec<_>>(),
        )
    };

    let mut right_projections = Vec::new();
    if key_cols.is_empty() {
        let right_key = &key_pairs[0].1;
        right_projections.push(lit(1_i64).alias(right_key.clone()));
    } else {
        for (key, alias) in key_cols
            .iter()
            .zip(key_pairs.iter().map(|(_, alias)| alias))
        {
            if !has_exact_col(&right, key) {
                return Err(RelError::Unsupported(format!(
                    "apply right side dropped correlation key `{key}`"
                )));
            }
            right_projections.push(col_exact(key).alias(alias));
        }
    }
    for col in output_cols {
        if has_exact_col(&right, col) {
            right_projections.push(col_exact(col));
        }
    }
    let right = LogicalPlanBuilder::from(right)
        .project(right_projections)?
        .build()?;
    let join_exprs = key_pairs
        .into_iter()
        .map(|(left_key, right_key)| {
            binary(col_exact(left_key), BinaryOp::Eq, col_exact(right_key))
        })
        .collect::<Vec<_>>();
    Ok((left, right, join_exprs, cleanup))
}

fn existing_columns_excluding_binding(
    plan: &LogicalPlan,
    binding: &str,
    excluded: &BTreeSet<String>,
) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| !excluded.contains(field.name()))
        .filter(|field| !is_binding_column(field.name(), binding))
        .map(|field| col_exact(field.name()))
        .collect()
}

fn existing_columns_excluding_bindings(plan: &LogicalPlan, bindings: &[&str]) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| {
            !bindings
                .iter()
                .any(|binding| is_binding_column(field.name(), binding))
        })
        .map(|field| col_exact(field.name()))
        .collect()
}

fn is_binding_column(name: &str, binding: &str) -> bool {
    name == binding
        || name == id_col(binding)
        || name == label_col(binding)
        || name == src_id_col(binding)
        || name == src_label_col(binding)
        || name == dst_id_col(binding)
        || name == dst_label_col(binding)
        || name.starts_with(&format!("{binding}{PROP_MARKER}"))
}

fn duplicate_binding_projection(plan: &LogicalPlan, from: &str, to: &str) -> RelResult<Vec<Expr>> {
    let mut projections = existing_columns(plan, &BTreeSet::new());
    projections.extend(duplicate_binding_projection_only(plan, from, to)?);
    Ok(projections)
}

fn duplicate_binding_projection_only(
    plan: &LogicalPlan,
    from: &str,
    to: &str,
) -> RelResult<Vec<Expr>> {
    let Some(shape) = has_binding_shape(plan, from) else {
        return Err(RelError::Unsupported(format!(
            "binding `{from}` is not an element binding"
        )));
    };
    let mut projections = vec![
        col_exact(id_col(from)).alias(id_col(to)),
        col_exact(label_col(from)).alias(label_col(to)),
    ];
    if shape == BindingShape::Edge {
        projections.extend([
            col_exact(src_label_col(from)).alias(src_label_col(to)),
            col_exact(src_id_col(from)).alias(src_id_col(to)),
            col_exact(dst_label_col(from)).alias(dst_label_col(to)),
            col_exact(dst_id_col(from)).alias(dst_id_col(to)),
        ]);
    }
    let prefix = format!("{from}{PROP_MARKER}");
    for field in plan.schema().fields() {
        let name = field.name();
        if let Some(property) = name.strip_prefix(&prefix) {
            projections.push(col_exact(name).alias(prop_col(to, property)));
        }
    }
    Ok(projections)
}

fn unsupported_node_name(node: &Node) -> &'static str {
    match node {
        Node::GraphCorrelate { .. } => "GraphCorrelate",
        Node::GraphRdfQuadScan { .. } => "GraphRdfQuadScan",
        Node::GraphPathPattern { .. } => "GraphPathPattern",
        Node::GraphRdfPropertyPath { .. } => "GraphRdfPropertyPath",
        Node::GraphRepeat { .. } => "GraphRepeat",
        Node::GraphPathFilter { .. } => "GraphPathFilter",
        Node::GraphCreate { .. } => "GraphCreate",
        Node::GraphSetProperty { .. } => "GraphSetProperty",
        Node::GraphDelete { .. } => "GraphDelete",
        Node::GraphGroupMap { .. } => "GraphGroupMap",
        Node::GraphGroupCountSideEffect { .. } => "GraphGroupCountSideEffect",
        Node::GraphCap { .. } => "GraphCap",
        Node::GraphShortestPath { .. } => "GraphShortestPath",
        Node::GraphSliceExpr { .. } => "GraphSliceExpr",
        Node::GraphBarrier { .. } => "GraphBarrier",
        Node::GraphApply { .. } => "GraphApply",
        Node::GraphUnwind { .. } => "GraphUnwind",
        Node::GraphQuantifier { .. } => "GraphQuantifier",
        Node::GraphCollect { .. } => "GraphCollect",
        Node::GraphListComprehension { .. } => "GraphListComprehension",
        Node::GraphCoalesce { .. } => "GraphCoalesce",
        Node::GraphChoose { .. } => "GraphChoose",
        Node::GraphSelect { .. } => "GraphSelect",
        Node::GraphSparqlMinus { .. } => "GraphSparqlMinus",
        Node::GraphService { .. } => "GraphService",
        Node::GraphProcedureCall { .. } => "GraphProcedureCall",
        Node::GraphExtension { .. } => "GraphExtension",
        Node::GraphConstructTriples { .. } => "GraphConstructTriples",
        Node::GraphDescribe { .. } => "GraphDescribe",
        Node::GraphAsk { .. } => "GraphAsk",
        _ => "Graph IR node",
    }
}
