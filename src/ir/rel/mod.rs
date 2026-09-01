//! Executable relational lowering for Graph IR.
//!
//! This module decomposes supported Graph IR regions into ordinary
//! DataFusion logical plans. It intentionally sits beside `ir::df`: that
//! module preserves graph operators as DataFusion extension nodes for rules
//! and round-tripping, while this module lowers graph-shaped operators into
//! base relational scans, joins, projections, filters, and aggregates that
//! DataFusion can execute directly.

pub mod mapping;
pub mod sql;
mod varlen;

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Array, Int64Builder, ListBuilder, RecordBatch,
    StringArray, StringBuilder, new_null_array,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow_select::concat::concat_batches;
use datafusion::common::{Column, ScalarValue};
use datafusion::datasource::{MemTable, provider_as_source};
use datafusion::error::DataFusionError;
use datafusion::functions::core::expr_fn as df_core;
use datafusion::functions::datetime::expr_fn as df_datetime;
use datafusion::functions::math::expr_fn as df_math;
use datafusion::functions::regex::expr_fn as df_regex;
use datafusion::functions::string::expr_fn as df_string;
use datafusion::functions::unicode::expr_fn as df_unicode;
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
use num_bigint::BigInt;
use num_traits::ToPrimitive;

use crate::ir::catalog::{CatalogError, EdgeTable, NodeTable, PropertyGraph};
use crate::ir::expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit, StringOp};
use crate::ir::interpreter::{
    ReturnedBatches, Row as InterpreterRow, compare_values, eval as interpreter_eval,
};
use crate::ir::plan::{
    ApplyKind, BindKind, ChooseArm, ChooseSelector, ChooseUnmatched, CoalesceSuccess, Direction,
    GraphPlan, JoinKind, LabelExpr, Node, NullsOrder, ProjectMode, ProjectionItem, QuantifierKind,
    Slice, SortDir, TargetMode, UnionAlign,
};
use crate::ir::policy::{Language, ResultForm};
use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY, Value};

const ID_SUFFIX: &str = "__id";
const LABEL_SUFFIX: &str = "__label";
const PROP_MARKER: &str = "__prop__";
const SRC_ID_SUFFIX: &str = "__src_id";
const SRC_LABEL_SUFFIX: &str = "__src_label";
const DST_ID_SUFFIX: &str = "__dst_id";
const DST_LABEL_SUFFIX: &str = "__dst_label";
/// Separator between a `x.*` projection alias and the property name each
/// expanded column carries (`a.*__star__ID`).
const STAR_SEP: &str = "__star__";
/// Hop count of a materialized variable-length path binding. The path value
/// itself lives in a column named after the binding.
const PATH_LEN_SUFFIX: &str = "__pathlen";
const MAX_EXECUTABLE_PLAN_NODES: usize = 200;
const MAX_EXECUTABLE_PLAN_DEPTH: usize = 64;

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
    /// "Bring your own schema": when set, node/relationship scans resolve
    /// through this mapping (user tables, views, or SQL queries) instead of
    /// the `PropertyGraph` catalog. See [`mapping::GraphMapping`].
    pub mapping: Option<Arc<mapping::GraphMapping>>,
    /// Optional, explicitly requested guard on recursive variable-length
    /// expansion depth. `None` preserves complete trail semantics and is the
    /// default; setting a value trades completeness for a workload ceiling.
    pub varlen_recursive_ceiling: Option<u32>,
}

impl Default for RelBackendOptions {
    fn default() -> Self {
        Self {
            tolerate_internal_path_state: true,
            mapping: None,
            varlen_recursive_ceiling: None,
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

pub async fn execute_lowered(lowered: LoweredPlan) -> RelResult<ReturnedBatches> {
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
            Node::GraphMerge {
                input,
                match_arm,
                create_arm,
                ..
            } => {
                stack.push((input, depth + 1));
                stack.push((match_arm, depth + 1));
                stack.push((create_arm, depth + 1));
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
            | Node::GraphSparqlTriplePattern { .. }
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
            // A zero-column EmptyRelation executes correctly in DataFusion,
            // but its SQL unparser emits an empty SELECT list when it is one
            // side of a cross join. A private, non-null dummy column gives
            // the SQL representation a concrete one-row relation.
            GraphOneRow => self.scan_batches(
                "one_row",
                vec![RecordBatch::try_new(
                    Arc::new(Schema::new(vec![Field::new(
                        "__w_one_row",
                        DataType::Int64,
                        false,
                    )])),
                    vec![Arc::new(Int64Array::from(vec![0_i64])) as ArrayRef],
                )?],
            )?,
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
                path,
                input,
                ..
            } => {
                if length.is_variable_length() {
                    // Cypher represents a variable relationship through its
                    // synthetic path binding, then projects the user-visible
                    // relationship variable from that path. Fixed expands use
                    // `rel_binding` directly.
                    let path_binding = path.as_ref().or(rel_binding.as_ref());
                    self.lower_expand_varlen(
                        input,
                        source,
                        target,
                        *target_mode,
                        target_labels,
                        path_binding,
                        rel_types,
                        *dir,
                        length,
                    )?
                } else {
                    self.lower_expand(
                        input,
                        source,
                        target,
                        *target_mode,
                        target_labels,
                        rel_binding.as_ref(),
                        rel_types,
                        *dir,
                        path.as_deref(),
                    )?
                }
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
                if aggs.len() == 1
                    && aggs[0].distinct
                    && matches!(
                        aggs[0].kind,
                        AggKind::CollectRows | AggKind::CollectTraversers
                    )
                {
                    return self.lower_first_distinct_collect(input, group, &aggs[0]);
                }
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
                let needs_row_count_barrier = aggs.iter().any(|agg| {
                    matches!(agg.kind, AggKind::CountRows | AggKind::CountBulk) && agg.arg.is_none()
                });
                let aggs = aggs
                    .iter()
                    .map(|agg| {
                        let expr = match agg.kind {
                            AggKind::CountRows | AggKind::CountBulk => match &agg.arg {
                                Some(arg) => df_count(self.lower_expr(&input.plan, arg)?),
                                None => count_input_rows(&input.plan),
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
                            AggKind::CountIf => {
                                let original = agg.arg.as_ref().ok_or_else(|| {
                                    RelError::Unsupported("count_if requires an argument".into())
                                })?;
                                let arg = self.lower_expr(&input.plan, original)?;
                                self.lower_count_if(&input.plan, original, arg, agg.distinct)?
                            }
                            // `DISTINCT` changes the result of these, unlike
                            // MIN/MAX where it is a no-op, so it has to be
                            // carried onto the aggregate rather than dropped.
                            // SQL's SUM/AVG are NULL over an empty or all-NULL
                            // group; Kuzu's identity for these is zero. Only
                            // the `OrNull`/plain-AVG kinds want SQL's answer.
                            AggKind::Sum | AggKind::SumOrZero => df_core::coalesce(vec![
                                distinct_if(
                                    df_sum(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                                    agg.distinct,
                                )?,
                                lit(0_i64),
                            ]),
                            AggKind::AvgOrZero => df_core::coalesce(vec![
                                distinct_if(
                                    df_avg(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                                    agg.distinct,
                                )?,
                                lit(0.0_f64),
                            ]),
                            AggKind::Avg | AggKind::AvgOrNull => distinct_if(
                                df_avg(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                                agg.distinct,
                            )?,
                            AggKind::Min | AggKind::MinOrNull => {
                                let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                                if agg
                                    .arg
                                    .as_ref()
                                    .is_some_and(|expr| self.is_blob_property_expr(expr))
                                {
                                    blob_extreme(arg, false)
                                } else {
                                    df_min(arg)
                                }
                            }
                            AggKind::Max | AggKind::MaxOrNull => {
                                let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                                if agg
                                    .arg
                                    .as_ref()
                                    .is_some_and(|expr| self.is_blob_property_expr(expr))
                                {
                                    blob_extreme(arg, true)
                                } else {
                                    df_max(arg)
                                }
                            }
                            AggKind::CollectRows | AggKind::CollectTraversers => {
                                let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                                // Kuzu's COLLECT ignores null inputs and
                                // returns NULL when every input is null.
                                // DuckDB's array_agg retains nulls unless the
                                // aggregate carries an explicit filter.
                                let collect = df_array_agg(arg.clone()).filter(arg.is_not_null());
                                if agg.distinct {
                                    // `DISTINCT` collects in first-appearance
                                    // order, which no SQL ordering expresses,
                                    // so leave it to the engine.
                                    distinct_if(collect.build()?, true)?
                                } else {
                                    // Direct evaluation collects in scan
                                    // order; SQL aggregates have no inherent
                                    // order at all. Pin it to the element ids
                                    // so both sides agree.
                                    let keys = scan_order_keys(&input.plan);
                                    if keys.is_empty() {
                                        collect.build()?
                                    } else {
                                        collect.order_by(keys).build()?
                                    }
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
                // The DataFusion unparser can incorrectly discard the FROM
                // side of COUNT(*) when it contains an UNWIND/cross join.
                // Give that input a real SQL CTE boundary; the SQL wrapper
                // extracts this internal alias before unparsing.
                let aggregate_input = if needs_row_count_barrier {
                    let barrier_id = self.scan_counter;
                    let name = format!("__w_sql_cte_aggregate_{barrier_id}");
                    self.scan_counter += 1;
                    let mut columns = input
                        .plan
                        .schema()
                        .fields()
                        .iter()
                        .map(|field| col_exact(field.name()))
                        .collect::<Vec<_>>();
                    // Keep this projection from being removed as an identity:
                    // a standalone join needs a SELECT list when unparsed.
                    columns.push(lit(1_i64).alias(format!("__w_cte_guard_{barrier_id}")));
                    LogicalPlanBuilder::from(input.plan.clone())
                        .project(columns)?
                        .alias(name)?
                        .build()?
                } else {
                    input.plan.clone()
                };
                let plan = LogicalPlanBuilder::from(aggregate_input)
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
                // Gremlin inserts an internal source-order marker to make
                // interpreter scans deterministic. The relational scan is
                // already emitted in that catalog order. Materializing this
                // marker as a SQL SORT can cause DataFusion to discard a
                // later user-facing order().by(...) as redundant.
                if keys.len() == 1
                    && matches!(
                        &keys[0].expr,
                        IrExpr::Call { name, .. } if name == "gremlin_scan_order"
                    )
                {
                    return Ok(input);
                }
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
            GraphGroupMap {
                key,
                value,
                output,
                input,
            } => self.lower_group_map(key, value, output, input)?,
            GraphQuantifier {
                kind,
                item_binding,
                input_expr,
                predicate,
                output,
                input,
            } => {
                self.lower_quantifier(*kind, item_binding, input_expr, predicate, output, input)?
            }
            GraphRepeat {
                times,
                emit,
                until,
                until_traversal,
                path,
                prefix_predicate,
                prefix_traversal,
                seed,
                body,
                ..
            } => self.lower_repeat(
                *times,
                emit,
                until.as_ref(),
                until_traversal.as_deref(),
                path.as_deref(),
                prefix_predicate.as_ref(),
                prefix_traversal.as_deref(),
                seed,
                body,
            )?,
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

    fn lower_count_if(
        &self,
        plan: &LogicalPlan,
        original: &IrExpr,
        value: Expr,
        distinct: bool,
    ) -> RelResult<Expr> {
        let data_type = value
            .get_type(plan.schema())
            .map_err(|err| RelError::Unsupported(format!("count_if argument type: {err}")))?;
        let truthy = match data_type {
            DataType::Boolean => value.clone(),
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _) => binary(value.clone(), BinaryOp::Neq, lit(0_i64)),
            DataType::Float16 | DataType::Float32 | DataType::Float64 => Expr::and(
                binary(value.clone(), BinaryOp::Neq, lit(0.0_f64)),
                Expr::Not(Box::new(df_math::isnan(value.clone()))),
            ),
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
                if expression_has_wide_numeric_cast(original) =>
            {
                let numeric = Expr::TryCast(TryCast::new(
                    Box::new(value.clone()),
                    DataType::Decimal128(38, 0),
                ));
                binary(numeric, BinaryOp::Neq, lit(0_i64))
            }
            _ => lit(false),
        };
        let count = df_count(value).filter(truthy);
        if distinct {
            Ok(count.distinct().build()?)
        } else {
            Ok(count.build()?)
        }
    }

    fn is_blob_property_expr(&self, expr: &IrExpr) -> bool {
        let IrExpr::Property { name, .. } = expr else {
            return false;
        };
        let is_blob = |schema: &Schema| {
            schema
                .fields()
                .iter()
                .find(|field| field.name().eq_ignore_ascii_case(name))
                .and_then(|field| field.metadata().get("new_graph.value_type"))
                .is_some_and(|kind| kind == "blob")
        };
        self.graph.labels().into_iter().any(|label| {
            self.graph
                .node_table(&label)
                .is_ok_and(|table| is_blob(table.batch.schema().as_ref()))
        }) || self.graph.rel_types().into_iter().any(|rel_type| {
            self.graph.edge_tables(&rel_type).is_ok_and(|tables| {
                tables
                    .iter()
                    .any(|table| is_blob(table.batch.schema().as_ref()))
            })
        })
    }

    /// Lower `collect(DISTINCT x)` as two aggregates. The first keeps the
    /// earliest scan position for each `(group, x)` pair; the second orders
    /// those unique values by that position. A plain SQL
    /// `array_agg(DISTINCT x)` is allowed to return hash order and therefore
    /// does not implement Cypher's first-appearance rule.
    fn lower_first_distinct_collect(
        &self,
        input: LoweredNode,
        group: &[ProjectionItem],
        agg: &AggCall,
    ) -> RelResult<LoweredNode> {
        let value_name = "__w_collect_value";
        let value = self.lower_required_agg_arg(&input.plan, &agg.arg)?;

        let mut group_projection = apply_correlation_key_columns(&input.plan)
            .iter()
            .map(|key| (key.clone(), col_exact(key)))
            .collect::<Vec<_>>();
        group_projection.extend(
            group
                .iter()
                .map(|item| {
                    Ok((
                        item.alias.clone(),
                        self.lower_expr(&input.plan, &item.expr)?,
                    ))
                })
                .collect::<RelResult<Vec<_>>>()?,
        );

        let order = scan_order_keys(&input.plan);
        if order.is_empty() {
            return Err(RelError::Unsupported(
                "collect(DISTINCT) without a stable scan-order key".into(),
            ));
        }
        let mut projection = group_projection
            .iter()
            .map(|(name, expr)| expr.clone().alias(name))
            .collect::<Vec<_>>();
        projection.push(value.alias(value_name));
        for (index, sort) in order.iter().enumerate() {
            projection.push(
                sort.expr
                    .clone()
                    .alias(format!("__w_collect_order_{index}")),
            );
        }
        let projected = LogicalPlanBuilder::from(input.plan.clone())
            .project(projection)?
            .filter(col_exact(value_name).is_not_null())?
            .build()?;

        let mut unique_groups = group_projection
            .iter()
            .map(|(name, _)| col_exact(name))
            .collect::<Vec<_>>();
        unique_groups.push(col_exact(value_name));
        let earliest = (0..order.len())
            .map(|index| {
                df_min(col_exact(format!("__w_collect_order_{index}")))
                    .alias(format!("__w_collect_first_{index}"))
            })
            .collect::<Vec<_>>();
        let unique = LogicalPlanBuilder::from(projected)
            .aggregate(unique_groups, earliest)?
            .alias("__w_collect_unique")?
            .build()?;

        let final_groups = group_projection
            .iter()
            .map(|(name, _)| col_exact(name))
            .collect::<Vec<_>>();
        let order = (0..order.len())
            .map(|index| col_exact(format!("__w_collect_first_{index}")).sort(true, false))
            .collect::<Vec<_>>();
        let collected = df_array_agg(col_exact(value_name))
            .order_by(order)
            .build()?
            .alias(agg.alias.clone());
        let plan = LogicalPlanBuilder::from(unique)
            .aggregate(final_groups, vec![collected])?
            .build()?;
        Ok(input.with_plan(plan))
    }

    fn lower_node_scan(&mut self, binding: &str, labels: &LabelExpr) -> RelResult<LoweredNode> {
        if let Some(user_mapping) = self.options.mapping.clone() {
            return mapping::lower_mapped_node_scan(self, &user_mapping, binding, labels);
        }
        let labels = self.node_labels(labels)?;
        let prop_defs = self.node_property_defs(&labels)?;
        let schema = node_schema(binding, &prop_defs);
        let mut batches = Vec::new();
        for label in labels {
            let table = match self.graph.node_table(&label) {
                Ok(table) => table,
                Err(CatalogError::UnknownLabel(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            batches.push(normalize_node_table(
                binding,
                table,
                &prop_defs,
                schema.clone(),
                self.language,
            )?);
        }
        if batches.is_empty() {
            batches.push(RecordBatch::new_empty(schema));
        }
        self.scan_batches("nodes", batches)
    }

    fn lower_rel_scan(&mut self, binding: &str, types: &LabelExpr) -> RelResult<LoweredNode> {
        if let Some(user_mapping) = self.options.mapping.clone() {
            return mapping::lower_mapped_rel_scan(self, &user_mapping, binding, types);
        }
        let rel_types = self.rel_types(types)?;
        let prop_defs = self.edge_property_defs(&rel_types)?;
        let schema = edge_schema(binding, &prop_defs);
        let mut batches = Vec::new();
        for rel_type in rel_types {
            let mut base_id = 0_i64;
            let tables = match self.graph.edge_tables(&rel_type) {
                Ok(tables) => tables,
                Err(CatalogError::UnknownRelType(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            for table in tables {
                batches.push(normalize_edge_table(
                    binding,
                    table,
                    base_id,
                    &prop_defs,
                    schema.clone(),
                    self.language,
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
        let batch = values_batch(self.language, bindings, rows)?;
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
        path: Option<&str>,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        if has_binding_shape(&input.plan, source).is_none() {
            return Err(RelError::Unsupported(format!(
                "expand source `{source}` is not an element binding"
            )));
        }

        let rel = rel_binding
            .cloned()
            .unwrap_or_else(|| format!("__rel_{}", self.scan_counter));
        let mut expanded = match dir {
            Direction::Out | Direction::In => self.lower_expand_direction(
                input,
                source,
                target,
                target_mode,
                target_labels,
                Some(&rel),
                rel_types,
                dir,
            ),
            Direction::Both => self.lower_expand_both(
                input,
                source,
                target,
                target_mode,
                target_labels,
                Some(&rel),
                rel_types,
            ),
        }?;
        if let Some(path) = path {
            expanded = self.materialize_fixed_path(expanded, path, source, target, &rel)?;
        } else if rel_binding.is_none() {
            let excluded = binding_column_names(&expanded.plan, &rel)?
                .into_iter()
                .collect::<BTreeSet<_>>();
            let projections = existing_columns(&expanded.plan, &excluded);
            let plan = LogicalPlanBuilder::from(expanded.plan.clone())
                .project(projections)?
                .build()?;
            expanded = expanded.with_plan(plan);
        }
        Ok(expanded)
    }

    fn materialize_fixed_path(
        &self,
        expanded: LoweredNode,
        path: &str,
        source: &str,
        target: &str,
        rel: &str,
    ) -> RelResult<LoweredNode> {
        let source_display =
            self.cypher_element_display_expr(&expanded.plan, source, BindingShape::Node)?;
        let target_display =
            self.cypher_element_display_expr(&expanded.plan, target, BindingShape::Node)?;
        let rel_display =
            self.cypher_element_display_expr(&expanded.plan, rel, BindingShape::Edge)?;
        let (expanded, rendered) = if has_exact_col(&expanded.plan, path) {
            let with_target = df_string::replace(
                col_exact(path),
                lit("], _RELS: ["),
                concat_exprs(vec![lit(","), target_display, lit("], _RELS: [")]),
            );
            let path_stage = "__w_path_with_target";
            let rel_stage = "__w_path_next_rel";
            let excluded = BTreeSet::from([path_stage.to_string(), rel_stage.to_string()]);
            let mut stage_projection = existing_columns(&expanded.plan, &excluded);
            stage_projection.push(with_target.alias(path_stage));
            stage_projection.push(rel_display.alias(rel_stage));
            let stage_plan = LogicalPlanBuilder::from(expanded.plan.clone())
                .project(stage_projection)?
                .build()?;
            let expanded = expanded.with_plan(stage_plan);
            let prefix = df_unicode::substring(
                col_exact(path_stage),
                lit(1_i64),
                binary(
                    df_unicode::length(col_exact(path_stage)),
                    BinaryOp::Sub,
                    lit(2_i64),
                ),
            );
            (
                expanded,
                concat_exprs(vec![prefix, lit(","), col_exact(rel_stage), lit("]}")]),
            )
        } else {
            (
                expanded,
                concat_exprs(vec![
                    lit("{_NODES: ["),
                    source_display,
                    lit(","),
                    target_display,
                    lit("], _RELS: ["),
                    rel_display,
                    lit("]}"),
                ]),
            )
        };
        let previous_len = path_len_col(path);
        let path_len = if has_exact_col(&expanded.plan, &previous_len) {
            binary(col_exact(&previous_len), BinaryOp::Add, lit(1_i64))
        } else {
            lit(1_i64)
        };
        let excluded = BTreeSet::from([
            path.to_string(),
            previous_len.clone(),
            "__w_path_with_target".to_string(),
            "__w_path_next_rel".to_string(),
        ]);
        let mut projections = existing_columns(&expanded.plan, &excluded);
        projections.push(rendered.alias(path));
        projections.push(path_len.alias(previous_len));
        let plan = LogicalPlanBuilder::from(expanded.plan.clone())
            .project(projections)?
            .build()?;
        Ok(expanded.with_plan(plan))
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
        // A join widens the row: both sides' fields survive. Keeping only
        // `left.fields` silently drops everything the right side binds, so
        // any later reference to it fails to resolve ("Referenced column
        // … was not found"). This stayed latent while nothing in the Cypher
        // path emitted `GraphJoin`; uncorrelated `MATCH (a), (b)` now does.
        // Left order wins on collision, matching the interpreter's
        // `join_op`, which inserts right bindings with `or_insert_with`.
        let fields = match (left.fields, right.fields) {
            (Some(mut left_fields), Some(right_fields)) => {
                for field in right_fields {
                    if !left_fields.contains(&field) {
                        left_fields.push(field);
                    }
                }
                Some(left_fields)
            }
            (left_fields, right_fields) => left_fields.or(right_fields),
        };
        Ok(LoweredNode {
            plan,
            islands,
            fields,
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
                // The right side normally absorbs the left through
                // `correlate_plan`, which is why returning it alone is
                // usually right. When it did not — an uncorrelated pattern
                // such as `UNWIND ... MATCH ...` or a comma-separated match —
                // the left is a genuine cross-product factor, and dropping it
                // loses both its multiplicity and its bindings.
                if !absorbed_correlation(&left.plan, &right.plan) {
                    // Keep a joined MATCH subtree on the right side of the
                    // Cartesian product. Without this boundary SQL join
                    // precedence can bind its first INNER JOIN to the UNWIND
                    // input on the left, changing both names and row counts.
                    let barrier_id = self.scan_counter;
                    let name = format!("__w_sql_cte_apply_{barrier_id}");
                    self.scan_counter += 1;
                    let mut columns = right
                        .plan
                        .schema()
                        .fields()
                        .iter()
                        .map(|field| col_exact(field.name()))
                        .collect::<Vec<_>>();
                    columns.push(lit(1_i64).alias(format!("__w_cte_guard_{barrier_id}")));
                    let right_input = LogicalPlanBuilder::from(right.plan.clone())
                        .project(columns)?
                        .alias(name)?
                        .build()?;
                    right.plan = LogicalPlanBuilder::from(left.plan.clone())
                        .cross_join(right_input)?
                        .build()?;
                }
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
        let input = self
            .lower_node(input)
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind input: {err}")))?;
        let Some(values) = constant_unwind_values(input_expr, outer)? else {
            return self.lower_unwind_dynamic(input, input_expr, bind, outer);
        };
        let value_rows = values
            .into_iter()
            .map(|value| vec![value])
            .collect::<Vec<_>>();
        let values_plan = self
            .lower_values(&[bind.to_string()], &value_rows)
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind values: {err}")))?;
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .cross_join(values_plan.plan.clone())
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind cross join: {err}")))?
            .build()
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind build: {err}")))?;
        let mut islands = input.islands;
        islands.merge(values_plan.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    /// Gremlin `groupCount()` — a single-row map rendered in the tagged
    /// `m[{"key":"d[count].l"}]` form. Entry order is irrelevant: the
    /// harness comparator sorts map entries on both sides.
    fn lower_group_map(
        &mut self,
        key: &IrExpr,
        value: &crate::ir::plan::GroupValue,
        output: &str,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        use crate::ir::plan::GroupValue;
        let input = self.lower_node(input)?;
        let key_expr = self.lower_expr(&input.plan, key)?;
        let key_type = key_expr
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("group key type: {err}")))?;
        let key_text = gremlin_tagged_text_expr(key_expr, &key_type);
        let value_alias = "__gm_value";
        let mut collected_value = false;
        let value_agg = match value {
            GroupValue::CountBulk => count_all(),
            GroupValue::Aggregate(agg) => match agg.kind {
                AggKind::CountRows | AggKind::CountBulk => match &agg.arg {
                    Some(arg) => df_count(self.lower_expr(&input.plan, arg)?),
                    None => count_all(),
                },
                AggKind::CountDistinct => {
                    let Some(arg) = &agg.arg else {
                        return Err(RelError::Unsupported(
                            "group count distinct without argument".into(),
                        ));
                    };
                    datafusion::functions_aggregate::count::count_distinct(
                        self.lower_expr(&input.plan, arg)?,
                    )
                }
                AggKind::CountIf => {
                    let original = agg.arg.as_ref().ok_or_else(|| {
                        RelError::Unsupported("count_if requires an argument".into())
                    })?;
                    let arg = self.lower_expr(&input.plan, original)?;
                    self.lower_count_if(&input.plan, original, arg, agg.distinct)?
                }
                AggKind::Sum | AggKind::SumOrZero => df_core::coalesce(vec![
                    distinct_if(
                        df_sum(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                        agg.distinct,
                    )?,
                    lit(0_i64),
                ]),
                AggKind::AvgOrZero => df_core::coalesce(vec![
                    distinct_if(
                        df_avg(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                        agg.distinct,
                    )?,
                    lit(0.0_f64),
                ]),
                AggKind::Avg | AggKind::AvgOrNull => distinct_if(
                    df_avg(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                    agg.distinct,
                )?,
                AggKind::Min | AggKind::MinOrNull => {
                    df_min(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                }
                AggKind::Max | AggKind::MaxOrNull => {
                    df_max(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                }
                AggKind::CollectRows | AggKind::CollectTraversers => {
                    let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                    if agg.alias.contains("unwrap") {
                        df_min(arg)
                    } else {
                        let arg_type = arg.get_type(input.plan.schema()).map_err(|err| {
                            RelError::Unsupported(format!("group value type: {err}"))
                        })?;
                        let rendered = gremlin_tagged_text_expr(arg.clone(), &arg_type);
                        collected_value = true;
                        let collect = df_array_agg(rendered).filter(arg.is_not_null());
                        if agg.distinct {
                            distinct_if(collect.build()?, true)?
                        } else {
                            collect.build()?
                        }
                    }
                }
                other => {
                    return Err(RelError::Unsupported(format!(
                        "GraphGroupMap aggregate `{other:?}`"
                    )));
                }
            },
        };
        let grouped = LogicalPlanBuilder::from(input.plan.clone())
            .aggregate(
                vec![key_text.alias("__gm_key")],
                vec![value_agg.alias(value_alias)],
            )?
            .build()?;
        let value_text = if collected_value {
            concat_exprs(vec![
                lit("l["),
                df_core::coalesce(vec![
                    datafusion::functions_nested::expr_fn::array_to_string(
                        col_exact(value_alias),
                        lit(","),
                    ),
                    lit(""),
                ]),
                lit("]"),
            ])
        } else {
            let value_type = plan_column_type(&grouped, value_alias)
                .ok_or_else(|| RelError::Unsupported("group value type is unavailable".into()))?;
            gremlin_tagged_text_expr(col_exact(value_alias), &value_type)
        };
        let entry = concat_exprs(vec![
            lit("\""),
            df_core::coalesce(vec![col_exact("__gm_key"), lit("null")]),
            lit("\":\""),
            value_text,
            lit("\""),
        ]);
        let entries = LogicalPlanBuilder::from(grouped)
            .project(vec![entry.alias("__gm_entry")])?
            .aggregate(
                Vec::<Expr>::new(),
                vec![df_array_agg(col_exact("__gm_entry")).alias("__gm_entries")],
            )?
            .build()?;
        let rendered = concat_exprs(vec![
            lit("m[{"),
            df_core::coalesce(vec![
                datafusion::functions_nested::expr_fn::array_to_string(
                    col_exact("__gm_entries"),
                    lit(","),
                ),
                lit(""),
            ]),
            lit("}]"),
        ]);
        let plan = LogicalPlanBuilder::from(entries)
            .project(vec![rendered.alias(output)])?
            .build()?;
        Ok(input.with_plan(plan))
    }

    /// Lower three-valued ALL/ANY/NONE/SINGLE semantics by assigning each
    /// input row an identity, unnesting its list, and reducing predicate
    /// outcomes back to one boolean per original row. The interpreter keeps
    /// the same truth table and remains the semantic oracle for these cases.
    fn lower_quantifier(
        &mut self,
        kind: QuantifierKind,
        item_binding: &str,
        input_expr: &IrExpr,
        predicate: &IrExpr,
        output: &str,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let original_columns = output_fields(&input.plan);
        let suffix = self.scan_counter;
        self.scan_counter += 1;
        let row_id = format!("__w_quantifier_row_{suffix}");
        let list_col = format!("__w_quantifier_list_{suffix}");
        let input_null = format!("__w_quantifier_null_{suffix}");
        let total = format!("__w_quantifier_total_{suffix}");
        let true_count = format!("__w_quantifier_true_{suffix}");
        let null_count = format!("__w_quantifier_unknown_{suffix}");
        let result_row = format!("__w_quantifier_result_row_{suffix}");

        let list = self.lower_expr(&input.plan, input_expr)?;
        let list_type = list
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("quantifier input type: {err}")))?;
        if !matches!(
            list_type,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
        ) {
            return Err(RelError::Unsupported(
                "GraphQuantifier over non-list expression".into(),
            ));
        }

        let row_number = df_window::row_number().alias(row_id.clone());
        let numbered = LogicalPlanBuilder::from(input.plan.clone())
            .window(vec![row_number])?
            .build()?;
        let list_length = datafusion::functions_nested::expr_fn::array_length(list.clone());
        let mut base_projection = existing_columns(&numbered, &BTreeSet::new());
        base_projection.extend([
            list.clone().alias(list_col.clone()),
            list.is_null().alias(input_null.clone()),
            Expr::Cast(Cast::new(Box::new(list_length), DataType::Int64)).alias(total.clone()),
        ]);
        let base = LogicalPlanBuilder::from(numbered)
            .project(base_projection)?
            .build()?;

        let mut expanded_projection =
            existing_columns(&base, &BTreeSet::from([item_binding.into()]));
        expanded_projection.push(col_exact(&list_col).alias(item_binding));
        let mut options = datafusion::common::UnnestOptions::default();
        options.preserve_nulls = true;
        let expanded = LogicalPlanBuilder::from(base.clone())
            .project(expanded_projection)?
            .unnest_column_with_options(Column::new_unqualified(item_binding), options)?
            .build()?;
        let predicate = self.lower_expr(&expanded, predicate)?;
        let has_item = binary(col_exact(&total), BinaryOp::Gt, lit(0_i64));
        let true_value = Expr::and(has_item.clone(), Expr::IsTrue(Box::new(predicate.clone())));
        let unknown_value = Expr::and(has_item, predicate.is_null());
        let count_case = |condition: Expr| {
            Expr::Case(Case::new(
                None,
                vec![(Box::new(condition), Box::new(lit(1_i64)))],
                Some(Box::new(lit(0_i64))),
            ))
        };
        let reduced = LogicalPlanBuilder::from(expanded)
            .aggregate(
                vec![
                    col_exact(&row_id),
                    col_exact(&input_null),
                    col_exact(&total),
                ],
                vec![
                    df_sum(count_case(true_value)).alias(true_count.clone()),
                    df_sum(count_case(unknown_value)).alias(null_count.clone()),
                ],
            )?
            .build()?;

        let true_is_zero = binary(col_exact(&true_count), BinaryOp::Eq, lit(0_i64));
        let true_is_one = binary(col_exact(&true_count), BinaryOp::Eq, lit(1_i64));
        let true_gt_one = binary(col_exact(&true_count), BinaryOp::Gt, lit(1_i64));
        let no_unknown = binary(col_exact(&null_count), BinaryOp::Eq, lit(0_i64));
        let false_count = binary(
            binary(col_exact(&total), BinaryOp::Sub, col_exact(&true_count)),
            BinaryOp::Sub,
            col_exact(&null_count),
        );
        let has_false = binary(false_count, BinaryOp::Gt, lit(0_i64));
        let null_bool = lit(ScalarValue::Boolean(None));
        let value = match kind {
            QuantifierKind::All => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (Box::new(has_false), Box::new(lit(false))),
                    (Box::new(no_unknown.clone()), Box::new(lit(true))),
                ],
                Some(Box::new(null_bool.clone())),
            )),
            QuantifierKind::Any => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (
                        Box::new(Expr::IsTrue(Box::new(binary(
                            col_exact(&true_count),
                            BinaryOp::Gt,
                            lit(0_i64),
                        )))),
                        Box::new(lit(true)),
                    ),
                    (Box::new(no_unknown.clone()), Box::new(lit(false))),
                ],
                Some(Box::new(null_bool.clone())),
            )),
            QuantifierKind::None => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (
                        Box::new(binary(col_exact(&true_count), BinaryOp::Gt, lit(0_i64))),
                        Box::new(lit(false)),
                    ),
                    (Box::new(no_unknown.clone()), Box::new(lit(true))),
                ],
                Some(Box::new(null_bool.clone())),
            )),
            QuantifierKind::Single => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (Box::new(true_gt_one), Box::new(lit(false))),
                    (
                        Box::new(Expr::and(true_is_one, no_unknown.clone())),
                        Box::new(lit(true)),
                    ),
                    (
                        Box::new(Expr::and(true_is_zero, no_unknown)),
                        Box::new(lit(false)),
                    ),
                ],
                Some(Box::new(null_bool)),
            )),
        };
        let result = LogicalPlanBuilder::from(reduced)
            .project(vec![
                col_exact(&row_id).alias(result_row.clone()),
                value.alias(output),
            ])?
            .build()?;
        let joined = LogicalPlanBuilder::from(base)
            .join_on(
                result,
                JoinType::Inner,
                vec![binary(
                    col_exact(&row_id),
                    BinaryOp::Eq,
                    col_exact(&result_row),
                )],
            )?
            .build()?;
        let mut final_projection = original_columns
            .into_iter()
            .map(col_exact)
            .collect::<Vec<_>>();
        final_projection.push(col_exact(output));
        let plan = LogicalPlanBuilder::from(joined)
            .project(final_projection)?
            .build()?;
        Ok(input.with_plan(plan))
    }

    /// `repeat(body).times(n)` via bounded unrolling: apply the lowered
    /// body n times, feeding each iteration's plan into the body's
    /// `GraphCorrelate` leaf. Only the times-terminated subset lowers;
    /// until-terminated loops keep their typed unsupported error.
    #[allow(clippy::too_many_arguments)]
    fn lower_repeat(
        &mut self,
        times: Option<u32>,
        emit: &crate::ir::plan::EmitMode,
        until: Option<&IrExpr>,
        until_traversal: Option<&Node>,
        path: Option<&str>,
        prefix_predicate: Option<&IrExpr>,
        prefix_traversal: Option<&Node>,
        seed: &Node,
        body: &Node,
    ) -> RelResult<LoweredNode> {
        use crate::ir::plan::EmitMode;
        const REPEAT_CAP: u32 = 8;
        if until.is_some() || until_traversal.is_some() {
            return Err(RelError::Unsupported(
                "GraphRepeat with until termination".into(),
            ));
        }
        if prefix_traversal.is_some() {
            return Err(RelError::Unsupported(
                "GraphRepeat with emit sub-traversal".into(),
            ));
        }
        if path.is_some() && !self.options.tolerate_internal_path_state {
            return Err(RelError::Unsupported("GraphRepeat with path".into()));
        }
        let Some(times) = times else {
            return Err(RelError::Unsupported(
                "GraphRepeat without times bound".into(),
            ));
        };
        if times > REPEAT_CAP {
            return Err(RelError::Unsupported(format!(
                "GraphRepeat times {times} exceeds unroll cap {REPEAT_CAP}"
            )));
        }
        // Whether the seed itself is emitted (`emit()` before `repeat`).
        let emit_seed = match (emit, prefix_predicate) {
            (EmitMode::AfterLoop, _) => false,
            (_, Some(predicate)) => match constant_value_expr(predicate) {
                Ok(Some(Value::Bool(value))) => value,
                _ => {
                    return Err(RelError::Unsupported(
                        "GraphRepeat with non-constant emit predicate".into(),
                    ));
                }
            },
            // Mirrors the interpreter: the seed is only emitted when a
            // prefix-emit predicate/traversal was attached.
            (EmitMode::AfterEachIteration, None) => false,
            (EmitMode::AfterEachIfPredicate(_) | EmitMode::AfterEachIfTraversal(_), None) => {
                return Err(RelError::Unsupported(
                    "GraphRepeat with conditional emit".into(),
                ));
            }
        };
        let emit_each = !matches!(emit, EmitMode::AfterLoop);

        let seed = self.lower_node(seed)?;
        let mut islands = seed.islands.clone();
        let mut current = seed.plan.clone();
        let mut emitted: Vec<LogicalPlan> = Vec::new();
        if emit_seed {
            emitted.push(current.clone());
        }
        let correlate_bindings = first_correlate_bindings(body);
        for _ in 0..times {
            // The body re-lowers with fixed binding names each iteration;
            // restrict the incoming plan to the bindings its correlate leaf
            // consumes so re-introduced scans do not collide with leftover
            // columns from the previous iteration.
            let feed = match &correlate_bindings {
                Some(bindings) => {
                    let mut projections = apply_correlation_key_columns(&current)
                        .iter()
                        .map(col_exact)
                        .collect::<Vec<_>>();
                    for field in output_fields(&current) {
                        if bindings
                            .iter()
                            .any(|binding| field == *binding || is_binding_column(&field, binding))
                            && !projections
                                .iter()
                                .any(|expr| matches!(expr, Expr::Column(col) if col.name == field))
                        {
                            projections.push(col_exact(&field));
                        }
                    }
                    if projections.is_empty() {
                        current.clone()
                    } else {
                        LogicalPlanBuilder::from(current.clone())
                            .project(projections)?
                            .build()?
                    }
                }
                None => current.clone(),
            };
            let iteration = self.lower_with_correlate(feed, body)?;
            islands.merge(iteration.islands);
            current = iteration.plan;
            if emit_each {
                emitted.push(current.clone());
            }
        }
        if !emit_each {
            emitted.push(current);
        }
        let mut union_plan: Option<LogicalPlan> = None;
        for branch in emitted {
            union_plan = Some(match union_plan {
                None => branch,
                Some(plan) => LogicalPlanBuilder::from(plan)
                    .union_by_name(branch)?
                    .build()?,
            });
        }
        let plan = union_plan
            .ok_or_else(|| RelError::Unsupported("GraphRepeat emitted no iterations".into()))?;
        Ok(LoweredNode {
            plan,
            islands,
            fields: seed.fields,
            result_form: seed.result_form,
        })
    }

    /// UNWIND over a non-constant expression. When the lowered expression
    /// has a real Arrow list type (e.g. it came from `collect(...)` /
    /// `array_agg`), DataFusion's unnest expands it directly.
    fn lower_unwind_dynamic(
        &mut self,
        input: LoweredNode,
        input_expr: &IrExpr,
        bind: &str,
        outer: bool,
    ) -> RelResult<LoweredNode> {
        let list_expr = if let IrExpr::List(items) = input_expr {
            datafusion::functions_nested::expr_fn::make_array(
                items
                    .iter()
                    .map(|item| self.lower_expr(&input.plan, item))
                    .collect::<RelResult<Vec<_>>>()?,
            )
        } else {
            self.lower_expr(&input.plan, input_expr)?
        };
        let data_type = list_expr
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("unwind expression type: {err}")))?;
        if !matches!(
            data_type,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
        ) {
            return Err(RelError::Unsupported(
                "GraphUnwind over non-constant list expression".into(),
            ));
        }
        let mut projections = existing_columns(&input.plan, &BTreeSet::from([bind.to_string()]));
        projections.push(list_expr.alias(bind));
        let mut options = datafusion::common::UnnestOptions::default();
        options.preserve_nulls = outer;
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections)?
            .unnest_column_with_options(Column::new_unqualified(bind), options)?
            .build()?;
        Ok(input.with_plan(plan))
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
            } else if let Some(star_cols) = star_expansion_columns(plan, field) {
                projections.extend(star_cols);
            } else if let Some(shape) = has_binding_shape(plan, field) {
                if self.language == Language::Gremlin {
                    projections.push(gremlin_element_display_expr(plan, field)?.alias(field));
                    continue;
                }
                projections.push(
                    self.cypher_element_display_expr(plan, field, shape)?
                        .alias(field),
                );
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
        // Edge endpoint helpers are element-valued expressions. Preserve
        // their node shape as separate id/label columns so the following
        // vertex subgraph join can attach the user's mapped properties.
        if let IrExpr::Call { name, args } = expr
            && matches!(name.as_str(), "edge_src" | "edge_dst")
            && let [IrExpr::Binding(binding)] = args.as_slice()
            && has_binding_shape(plan, binding) == Some(BindingShape::Edge)
        {
            let (id, label) = if name == "edge_src" {
                (src_id_col(binding), src_label_col(binding))
            } else {
                (dst_id_col(binding), dst_label_col(binding))
            };
            return Ok(vec![
                col_exact(id).alias(id_col(alias)),
                col_exact(label).alias(label_col(alias)),
            ]);
        }
        if let IrExpr::Call { name, args } = expr
            && matches!(name.as_str(), "make_map" | "map")
            && args.len() % 2 == 0
            && args
                .chunks(2)
                .all(|pair| matches!(pair[0], IrExpr::Lit(Lit::String(_))))
        {
            let rendered = if name == "make_map" {
                self.lower_make_map(plan, args)?
            } else {
                self.lower_cypher_map(plan, args)?
            };
            let mut projections = vec![rendered.alias(alias)];
            for pair in args.chunks(2) {
                let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
                    return Err(RelError::Unsupported("dynamic make_map key".into()));
                };
                let value = if let IrExpr::List(items) = &pair[1] {
                    datafusion::functions_nested::expr_fn::make_array(
                        items
                            .iter()
                            .map(|item| self.lower_expr(plan, item))
                            .collect::<RelResult<Vec<_>>>()?,
                    )
                } else {
                    self.lower_expr(plan, &pair[1])?
                };
                projections.push(value.alias(prop_col(alias, key)));
            }
            return Ok(projections);
        }
        // `RETURN a.*` expands into one column per known property, in the
        // catalog's schema order, so the row formatter renders each value
        // with its native type (floats keep six decimals, booleans print
        // True/False, nulls print empty).
        if let IrExpr::Call { name, args } = expr
            && name == "cypher_property_star"
            && args.len() == 1
            && let IrExpr::Binding(binding) = &args[0]
            && let Some(shape) = has_binding_shape(plan, binding)
        {
            let keys = self.element_property_keys(plan, binding, shape);
            if keys.is_empty() {
                // Element with no properties: `RETURN x.*` prints one empty
                // cell per row.
                return Ok(vec![
                    lit(ScalarValue::Utf8(None)).alias(format!("{alias}{STAR_SEP}")),
                ]);
            }
            return Ok(keys
                .iter()
                .map(|key| {
                    col_exact(prop_col(binding, key)).alias(format!("{alias}{STAR_SEP}{key}"))
                })
                .collect());
        }
        if let IrExpr::Call { name, args } = expr
            && name == "cypher_property_star"
            && let [
                IrExpr::Property {
                    binding,
                    name: property,
                    ..
                },
            ] = args.as_slice()
        {
            let prefix = format!("{}__w_struct__", prop_col(binding, property));
            let fields = output_fields(plan)
                .into_iter()
                .filter_map(|column| {
                    column
                        .strip_prefix(&prefix)
                        .map(|field| (column.clone(), field.to_string()))
                })
                .collect::<Vec<_>>();
            if !fields.is_empty() {
                return Ok(fields
                    .into_iter()
                    .map(|(column, field)| {
                        col_exact(column).alias(format!("{alias}{STAR_SEP}{field}"))
                    })
                    .collect());
            }
        }
        if self.options.tolerate_internal_path_state
            && alias == "__path"
            && matches!(expr, IrExpr::Call { name, .. } if name.starts_with("path_"))
        {
            return Ok(vec![lit(ScalarValue::Utf8(None)).alias(alias)]);
        }
        if let IrExpr::Call { name, args } = expr
            && name == "recursive_relationship_path"
        {
            // The variable-length expand materializes the path under the
            // binding's own name, one branch per hop count.
            if let Some(IrExpr::Binding(binding)) = args.first()
                && has_exact_col(plan, binding)
            {
                let mut projections = vec![col_exact(binding).alias(alias)];
                let hops = path_len_col(binding);
                if has_exact_col(plan, &hops) {
                    projections.push(col_exact(hops).alias(path_len_col(alias)));
                }
                return Ok(projections);
            }
            // Otherwise the path was not materialized. A null placeholder
            // keeps count/exists-style queries over paths working, while
            // queries that actually print the path surface as mismatches.
            if self.options.tolerate_internal_path_state {
                return Ok(vec![lit(ScalarValue::Utf8(None)).alias(alias)]);
            }
        }
        Ok(vec![self.lower_expr(plan, expr)?.alias(alias)])
    }

    fn lower_expr(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        if matches!(
            expr,
            IrExpr::Binary { .. }
                | IrExpr::Not(_)
                | IrExpr::IsNull(_)
                | IrExpr::IsNotNull(_)
                | IrExpr::StringPredicate { .. }
                | IrExpr::Case { .. }
                | IrExpr::Call { .. }
                | IrExpr::ListTransform { .. }
                | IrExpr::ListFilter { .. }
                | IrExpr::ListReduce { .. }
        ) && expr_is_constant(expr, &[])
        {
            if let Some(folded) = self.try_constant_fold(expr)? {
                return Ok(folded);
            }
        }
        match expr {
            IrExpr::Lit(lit_value) => Ok(lit_to_expr(lit_value)),
            IrExpr::List(items) => self.lower_collection_expr(plan, items),
            IrExpr::Binding(binding) => {
                if let Some(column) = resolve_column_name(plan, binding) {
                    Ok(col_exact(column))
                } else if let Some(shape) = has_binding_shape(plan, binding) {
                    if self.language == Language::Gremlin {
                        gremlin_element_display_expr(plan, binding)
                    } else {
                        self.cypher_element_display_expr(plan, binding, shape)
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
                if !has_exact_col(plan, &col) {
                    return Err(RelError::Unsupported(format!("id({binding})")));
                }
                // Gremlin source and hasId filters pair a label predicate
                // with the per-label row id. Cypher's ID() result is the
                // provider-qualified `table:offset` value below.
                if self.language == Language::Gremlin {
                    return Ok(col_exact(col));
                }
                // Kuzu's `ID()` yields an internal id that prints as
                // `table:offset`, not a bare offset. Mirror
                // `interpreter::element_id` so the same element gets the same
                // id whichever path evaluated it.
                let table_index = match has_binding_shape(plan, binding) {
                    Some(BindingShape::Node) => label_index_case(
                        col_exact(label_col(binding)),
                        self.graph.node_label_order(),
                        0,
                        1,
                    ),
                    Some(BindingShape::Edge) => {
                        rel_index_case(col_exact(label_col(binding)), self.graph)
                    }
                    // Not an element binding — nothing to qualify it with.
                    None => return Ok(col_exact(col)),
                };
                Ok(concat_exprs(vec![
                    table_index,
                    lit(":"),
                    cast_utf8(col_exact(col)),
                ]))
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
                let pattern = self.lower_expr(plan, pattern)?;
                Ok(match op {
                    StringOp::StartsWith => df_string::starts_with(target, pattern),
                    StringOp::EndsWith => df_string::ends_with(target, pattern),
                    StringOp::Contains => df_string::contains(target, pattern),
                })
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
            IrExpr::Call { name, args } if name.eq_ignore_ascii_case("range") => {
                let values = constant_range_values(args)?;
                Ok(lit(rel_display_value(
                    &Value::List(values),
                    self.language,
                    literal_collection_context(self.language),
                )))
            }
            IrExpr::Call { name, args } if name == "list_slice" && args.len() == 3 => {
                let array = self.lower_expr(plan, &args[0])?;
                let start = self.lower_expr(plan, &args[1])?;
                let end = self.lower_expr(plan, &args[2])?;
                Ok(datafusion::functions_nested::expr_fn::array_slice(
                    array, start, end, None,
                ))
            }
            IrExpr::Call { name, args } if is_constant_collection_function(name) => {
                self.lower_constant_collection_function(plan, name, args)
            }
            IrExpr::Call { name, args } if name == "integer_literal" && args.len() == 1 => {
                self.lower_integer_literal(&args[0])
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
            IrExpr::Call { name, args } if name.eq_ignore_ascii_case("uuid") && args.len() == 1 => {
                Ok(df_string::lower(self.lower_expr(plan, &args[0])?))
            }
            IrExpr::Call { name, args }
                if name.eq_ignore_ascii_case("gen_random_uuid") && args.is_empty() =>
            {
                Ok(df_string::uuid())
            }
            IrExpr::Call { name, args }
                if name.eq_ignore_ascii_case("gremlin_cast_int") && args.len() == 1 =>
            {
                // TinkerPop narrows fractional values toward zero. DuckDB's
                // direct floating-to-integer cast rounds, so make the
                // language choice explicit in the relational expression.
                let value = self.lower_expr(plan, &args[0])?;
                Ok(Expr::Cast(Cast::new(
                    Box::new(df_math::trunc(vec![value])),
                    DataType::Int32,
                )))
            }
            IrExpr::Call { name, args } if name == "cast_number" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                Ok(match data_type {
                    DataType::Int8
                    | DataType::Int16
                    | DataType::Int32
                    | DataType::Int64
                    | DataType::UInt8
                    | DataType::UInt16
                    | DataType::UInt32
                    | DataType::UInt64
                    | DataType::Float32
                    | DataType::Float64
                    | DataType::Decimal128(_, _) => value,
                    DataType::Boolean => Expr::Cast(Cast::new(Box::new(value), DataType::Int64)),
                    _ => Expr::TryCast(TryCast::new(Box::new(value), DataType::Float64)),
                })
            }
            IrExpr::Call { name, args } if name == "gremlin_cast_date" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                Ok(match data_type {
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => value,
                    DataType::Date32 | DataType::Timestamp(_, _) => cast_utf8(value),
                    _ => cast_utf8(Expr::TryCast(TryCast::new(
                        Box::new(value),
                        DataType::Timestamp(arrow::datatypes::TimeUnit::Millisecond, None),
                    ))),
                })
            }
            IrExpr::Call { name, args } if is_cast_function(name, args) => {
                if cast_target_text(name, args).is_some_and(|target| target.trim().ends_with("[]"))
                {
                    // Catalog list properties are already normalized to
                    // canonical display strings at the relational boundary.
                    // Casting only changes their element type; every supported
                    // list element here has the same Cypher display text.
                    return self.lower_expr(plan, &args[0]);
                }
                let (value, data_type, lenient) = self.cast_parts(plan, name, args)?;
                let cast = if lenient {
                    Expr::TryCast(TryCast::new(Box::new(value), data_type))
                } else {
                    Expr::Cast(Cast::new(Box::new(value), data_type))
                };
                Ok(cast)
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
            IrExpr::Call { name, args } if is_unary_math_function(name) && args.len() == 1 => {
                self.lower_unary_math_function(plan, name, &args[0])
            }
            IrExpr::Call { name, args } if is_binary_math_function(name) && args.len() == 2 => {
                self.lower_binary_math_function(plan, name, &args[0], &args[1])
            }
            IrExpr::Call { name, args } if is_date_function(name) && args.len() == 1 => {
                Ok(cast_utf8(self.lower_expr(plan, &args[0])?))
            }
            IrExpr::Call { name, args }
                if matches!(
                    normalize_function_name(name).as_str(),
                    "date_part" | "date_trunc"
                ) =>
            {
                self.lower_temporal_function(plan, name, args)
            }
            IrExpr::Call { name, args } if is_string_function(name) => {
                self.lower_string_function(plan, name, args)
            }
            IrExpr::Call { name, args } if is_core_variadic_function(name) => {
                self.lower_core_variadic_function(plan, name, args)
            }
            IrExpr::Call { name, args } if name == "gremlin_math_bin" && args.len() == 3 => {
                let IrExpr::Lit(Lit::String(op)) = &args[0] else {
                    return Err(RelError::Unsupported(
                        "dynamic Gremlin math operator".into(),
                    ));
                };
                let number =
                    |expr: Expr| Expr::TryCast(TryCast::new(Box::new(expr), DataType::Float64));
                let lhs = number(self.lower_expr(plan, &args[1])?);
                let rhs = number(self.lower_expr(plan, &args[2])?);
                let op = match op.as_str() {
                    "add" => BinaryOp::Add,
                    "sub" => BinaryOp::Sub,
                    "mul" => BinaryOp::Mul,
                    "div" => BinaryOp::Div,
                    _ => {
                        return Err(RelError::Unsupported(format!(
                            "Gremlin math operator `{op}`"
                        )));
                    }
                };
                Ok(binary(lhs, op, rhs))
            }
            IrExpr::Call { name, args } if name == "format_concat" && !args.is_empty() => {
                let pieces = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(concat_exprs(pieces))
            }
            IrExpr::Call { name, args } if name == "conjoin" && args.len() == 2 => {
                let value = self.lower_expr(plan, &args[0])?;
                let delimiter = cast_utf8(self.lower_expr(plan, &args[1])?);
                let data_type = value.get_type(plan.schema())?;
                Ok(
                    if matches!(
                        data_type,
                        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                    ) {
                        datafusion::functions_nested::expr_fn::array_to_string(value, delimiter)
                    } else if matches!(
                        data_type,
                        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
                    ) {
                        concat_exprs(vec![cast_utf8(value), delimiter])
                    } else {
                        cast_utf8(value)
                    },
                )
            }
            IrExpr::Call { name, args } if name == "null_to_sentinel" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(value.clone().is_null()),
                        Box::new(lit("\0gremlin.null")),
                    )],
                    Some(Box::new(cast_utf8(value))),
                )))
            }
            IrExpr::Call { name, args } if name == "gremlin_dedup_key" && args.len() == 1 => {
                // Relational maps are already canonical display values. For
                // scalar and element keys, Gremlin dedup uses the value as-is.
                self.lower_expr(plan, &args[0])
            }
            IrExpr::Call { name, args }
                if name == "list_restore_null_sentinels" && args.len() == 1 =>
            {
                let value = self.lower_native_list(plan, &args[0])?;
                Ok(datafusion::functions_nested::expr_fn::array_replace_all(
                    value,
                    lit("\0gremlin.null"),
                    lit(ScalarValue::Utf8(None)),
                ))
            }
            IrExpr::Call { name, args } if name.eq_ignore_ascii_case("xor") && args.len() == 2 => {
                let lhs = self.lower_expr(plan, &args[0])?;
                let rhs = self.lower_expr(plan, &args[1])?;
                Ok(Expr::or(
                    Expr::and(lhs.clone(), Expr::IsNotTrue(Box::new(rhs.clone()))),
                    Expr::and(Expr::IsNotTrue(Box::new(lhs)), rhs),
                ))
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
            IrExpr::Call { name, args } if name == "union_value" => {
                let (_, value) = union_constructor_field(args)?;
                self.lower_expr(plan, value)
            }
            IrExpr::Call { name, args } if name == "union_tag" && args.len() == 1 => {
                if let IrExpr::Call {
                    name: constructor,
                    args,
                } = &args[0]
                    && constructor == "union_value"
                {
                    let (tag, _) = union_constructor_field(args)?;
                    Ok(lit(tag.to_string()))
                } else if let IrExpr::Property { binding, name, .. } = &args[0] {
                    let column = union_tag_col(binding, name);
                    if has_exact_col(plan, &column) {
                        Ok(col_exact(column))
                    } else {
                        Err(RelError::Unsupported(format!(
                            "union_tag metadata is unavailable for `{binding}.{name}`"
                        )))
                    }
                } else {
                    Err(RelError::Unsupported(
                        "union_tag over a stored/dynamic union".into(),
                    ))
                }
            }
            IrExpr::Call { name, args } if name == "union_extract" && args.len() == 2 => {
                let IrExpr::Call {
                    name: constructor,
                    args: constructor_args,
                } = &args[0]
                else {
                    return Err(RelError::Unsupported(
                        "union_extract over a stored/dynamic union".into(),
                    ));
                };
                if constructor != "union_value" {
                    return Err(RelError::Unsupported(
                        "union_extract over a stored/dynamic union".into(),
                    ));
                }
                let (tag, value) = union_constructor_field(constructor_args)?;
                let IrExpr::Lit(Lit::String(requested)) = &args[1] else {
                    return Err(RelError::Unsupported(
                        "union_extract with a dynamic tag".into(),
                    ));
                };
                if tag.eq_ignore_ascii_case(requested) {
                    self.lower_expr(plan, value)
                } else {
                    Ok(lit(ScalarValue::Utf8(None)))
                }
            }
            IrExpr::Call { name, args }
                if name == "select_key_or_binding_pop" && args.len() == 5 =>
            {
                self.lower_select_key_or_binding(plan, &args[1])
            }
            IrExpr::Call { name, args }
                if (name == "value_map" || name == "value_map_tokens") && !args.is_empty() =>
            {
                self.lower_value_map(plan, name, args)
            }
            IrExpr::Call { name, args } if name == "gremlin_unfold_items" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                if matches!(
                    data_type,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                ) {
                    Ok(value)
                } else {
                    // Gremlin unfolds a scalar, including null, as one item.
                    Ok(datafusion::functions_nested::expr_fn::make_array(vec![
                        value,
                    ]))
                }
            }
            IrExpr::Call { name, args } if name == "local_count" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                if matches!(
                    data_type,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                ) {
                    Ok(Expr::Cast(Cast::new(
                        Box::new(datafusion::functions_nested::expr_fn::array_length(value)),
                        DataType::Int64,
                    )))
                } else {
                    Ok(lit(1_i64))
                }
            }
            IrExpr::Call { name, args }
                if matches!(name.as_str(), "local_min" | "local_max") && args.len() == 1 =>
            {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                if matches!(
                    data_type,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                ) {
                    Ok(if name == "local_min" {
                        datafusion::functions_nested::expr_fn::array_min(value)
                    } else {
                        datafusion::functions_nested::expr_fn::array_max(value)
                    })
                } else {
                    Ok(value)
                }
            }
            IrExpr::Call { name, args }
                if matches!(
                    name.as_str(),
                    "list_combine" | "list_merge" | "list_intersect"
                ) && args.len() == 2 =>
            {
                let lhs = self.lower_native_list(plan, &args[0])?;
                let rhs = self.lower_native_list(plan, &args[1])?;
                Ok(match name.as_str() {
                    "list_combine" => {
                        datafusion::functions_nested::expr_fn::array_concat(vec![lhs, rhs])
                    }
                    "list_merge" => datafusion::functions_nested::expr_fn::array_distinct(
                        datafusion::functions_nested::expr_fn::array_concat(vec![lhs, rhs]),
                    ),
                    "list_intersect" => {
                        datafusion::functions_nested::expr_fn::array_intersect(lhs, rhs)
                    }
                    _ => unreachable!(),
                })
            }
            IrExpr::Call { name, args } if name == "map" => self.lower_cypher_map(plan, args),
            IrExpr::Call { name, args } if name == "make_map" => self.lower_make_map(plan, args),
            IrExpr::Call { name, args } if name == "cypher_subscript" && args.len() == 2 => {
                self.lower_cypher_subscript(plan, &args[0], &args[1])
            }
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

    /// Evaluate a constant expression through the interpreter so folding
    /// matches engine semantics exactly (including Kuzu-style error text).
    /// Returns `Ok(None)` when the interpreter cannot evaluate it for an
    /// internal reason, letting the relational lowering take over.
    fn try_constant_fold(&self, expr: &IrExpr) -> RelResult<Option<Expr>> {
        let row = InterpreterRow::new();
        match interpreter_eval(expr, &row, self.graph) {
            Ok(value) => Ok(Some(constant_fold_result_expr(&value, self.language))),
            Err(err) => {
                let message = err.to_string();
                if looks_like_engine_error(&message) {
                    Err(RelError::Unsupported(message))
                } else {
                    Ok(None)
                }
            }
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

    fn lower_collection_expr(&self, plan: &LogicalPlan, items: &[IrExpr]) -> RelResult<Expr> {
        if let Some(value) = constant_value_expr(&IrExpr::List(items.to_vec()))? {
            return Ok(lit(rel_display_value(
                &value,
                self.language,
                literal_collection_context(self.language),
            )));
        }
        let mut pieces = vec![lit("[")];
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                pieces.push(lit(","));
            }
            let value = self.lower_expr(plan, item)?;
            let data_type = value.get_type(plan.schema())?;
            pieces.push(match data_type {
                DataType::Boolean => Expr::Case(Case::new(
                    None,
                    vec![(Box::new(value.clone()), Box::new(lit("True")))],
                    Some(Box::new(Expr::Case(Case::new(
                        None,
                        vec![(
                            Box::new(value.is_null()),
                            Box::new(lit(ScalarValue::Utf8(None))),
                        )],
                        Some(Box::new(lit("False"))),
                    )))),
                )),
                _ => cast_utf8(value),
            });
        }
        pieces.push(lit("]"));
        Ok(concat_exprs(pieces))
    }

    fn lower_constant_collection_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        if let Some(value) = constant_collection_function_value(name, args)? {
            return Ok(constant_result_expr(
                &value,
                self.language,
                literal_collection_context(self.language),
            ));
        }
        if let Some(expr) = self.lower_dynamic_collection_function(plan, name, args)? {
            return Ok(expr);
        }
        Err(RelError::Unsupported(format!(
            "function `{name}` is not relationally lowered yet"
        )))
    }

    fn lower_dynamic_collection_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Option<Expr>> {
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "list_append" | "array_append" | "array_push_back" => {
                let [items, item] = args else {
                    return Ok(None);
                };
                Ok(Some(self.lower_list_insert(plan, items, item, false)?))
            }
            "list_prepend" | "array_prepend" | "array_push_front" => {
                let [items, item] = args else {
                    return Ok(None);
                };
                Ok(Some(self.lower_list_insert(plan, items, item, true)?))
            }
            "list_element" | "list_extract" | "element_at" => {
                let [IrExpr::List(items), index] = args else {
                    return Ok(None);
                };
                let Some(index) = literal_i64(index) else {
                    return Ok(None);
                };
                let Some(item) = list_element_1_based_expr(items, index) else {
                    return Ok(Some(lit(ScalarValue::Utf8(None))));
                };
                Ok(Some(self.lower_expr(plan, item)?))
            }
            "list_unique" => {
                let [items] = args else {
                    return Ok(None);
                };
                let distinct = datafusion::functions_nested::expr_fn::array_distinct(
                    self.lower_native_list(plan, items)?,
                );
                let count = Expr::Cast(Cast::new(
                    Box::new(datafusion::functions_nested::expr_fn::array_length(
                        distinct,
                    )),
                    DataType::Int64,
                ));
                // DataFusion and DuckDB both omit nulls from array_distinct.
                // Cypher's list_unique follows the same rule, so the resulting
                // array length is already the desired count.
                Ok(Some(count))
            }
            "list_contains" | "list_has" | "array_contains" | "array_has" => {
                let [items, needle] = args else {
                    return Ok(None);
                };
                Ok(Some(datafusion::functions_nested::expr_fn::array_has(
                    self.lower_native_list(plan, items)?,
                    self.lower_expr(plan, needle)?,
                )))
            }
            "list_has_all" => {
                let [items, needles] = args else {
                    return Ok(None);
                };
                Ok(Some(datafusion::functions_nested::expr_fn::array_has_all(
                    self.lower_native_list(plan, items)?,
                    self.lower_native_list(plan, needles)?,
                )))
            }
            _ => Ok(None),
        }
    }

    fn lower_native_list(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        if let IrExpr::List(items) = expr {
            return Ok(datafusion::functions_nested::expr_fn::make_array(
                items
                    .iter()
                    .map(|item| self.lower_expr(plan, item))
                    .collect::<RelResult<Vec<_>>>()?,
            ));
        }
        let lowered = self.lower_expr(plan, expr)?;
        let data_type = lowered.get_type(plan.schema())?;
        if matches!(
            data_type,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
        ) {
            Ok(lowered)
        } else {
            Err(RelError::Unsupported(format!(
                "list operation over non-list type {data_type}"
            )))
        }
    }

    fn lower_list_insert(
        &self,
        plan: &LogicalPlan,
        items: &IrExpr,
        item: &IrExpr,
        prepend: bool,
    ) -> RelResult<Expr> {
        let items = if matches!(items, IrExpr::List(_)) {
            self.lower_native_list(plan, items)?
        } else {
            self.lower_expr(plan, items)?
        };
        let item = self.lower_expr(plan, item)?;
        let items_type = items.get_type(plan.schema())?;
        let item_type = item.get_type(plan.schema())?;
        match items_type {
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
                if prepend {
                    Ok(datafusion::functions_nested::expr_fn::array_prepend(
                        item, items,
                    ))
                } else {
                    Ok(datafusion::functions_nested::expr_fn::array_append(
                        items, item,
                    ))
                }
            }
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                let items = cast_utf8(items);
                let rendered_item = render_property_text_expr(item, &item_type);
                let empty = binary(items.clone(), BinaryOp::Eq, lit("[]"));
                if prepend {
                    let suffix = Expr::Case(Case::new(
                        None,
                        vec![(Box::new(empty), Box::new(lit("]")))],
                        Some(Box::new(concat_exprs(vec![
                            lit(","),
                            df_unicode::substring(
                                items.clone(),
                                lit(2_i64),
                                binary(df_unicode::length(items), BinaryOp::Sub, lit(1_i64)),
                            ),
                        ]))),
                    ));
                    Ok(concat_exprs(vec![lit("["), rendered_item, suffix]))
                } else {
                    let prefix = Expr::Case(Case::new(
                        None,
                        vec![(Box::new(empty), Box::new(lit("[")))],
                        Some(Box::new(concat_exprs(vec![
                            df_unicode::substring(
                                items.clone(),
                                lit(1_i64),
                                binary(df_unicode::length(items), BinaryOp::Sub, lit(1_i64)),
                            ),
                            lit(","),
                        ]))),
                    ));
                    Ok(concat_exprs(vec![prefix, rendered_item, lit("]")]))
                }
            }
            other => Err(RelError::Unsupported(format!(
                "list operation over non-list type {other}"
            ))),
        }
    }

    fn lower_temporal_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let [unit, value] = args else {
            return Err(RelError::Unsupported(format!("{name} arity")));
        };
        let unit = match constant_value_expr(unit)? {
            Some(Value::String(unit)) => lit(normalize_temporal_unit(&unit)),
            _ => cast_utf8(self.lower_expr(plan, unit)?),
        };
        let value = self.lower_expr(plan, value)?;
        let original_type = value.get_type(plan.schema())?;
        let temporal = if matches!(
            original_type,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
        ) {
            Expr::TryCast(TryCast::new(
                Box::new(value.clone()),
                DataType::Timestamp(arrow::datatypes::TimeUnit::Nanosecond, None),
            ))
        } else {
            value.clone()
        };
        match normalize_function_name(name).as_str() {
            "date_part" => Ok(df_datetime::date_part(unit, temporal)),
            "date_trunc" => {
                let rendered = cast_utf8(df_datetime::date_trunc(unit, temporal));
                if matches!(
                    original_type,
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
                ) {
                    // Date values and timestamps share the catalog's textual
                    // boundary. Preserve date-only output when the source has
                    // no time component; timestamps retain midnight fields.
                    let is_date = binary(
                        df_unicode::length(cast_utf8(value)),
                        BinaryOp::Eq,
                        lit(10_i64),
                    );
                    Ok(Expr::Case(Case::new(
                        None,
                        vec![(
                            Box::new(is_date),
                            Box::new(df_unicode::substring(
                                rendered.clone(),
                                lit(1_i64),
                                lit(10_i64),
                            )),
                        )],
                        Some(Box::new(rendered)),
                    )))
                } else {
                    Ok(rendered)
                }
            }
            _ => unreachable!(),
        }
    }

    fn lower_cypher_subscript(
        &self,
        plan: &LogicalPlan,
        target: &IrExpr,
        index: &IrExpr,
    ) -> RelResult<Expr> {
        let target_expr = if matches!(target, IrExpr::List(_)) {
            self.lower_native_list(plan, target)?
        } else {
            self.lower_expr(plan, target)?
        };
        let data_type = target_expr.get_type(plan.schema())?;
        let index = Expr::Cast(Cast::new(
            Box::new(self.lower_expr(plan, index)?),
            DataType::Int64,
        ));
        match data_type {
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => Ok(
                datafusion::functions_nested::expr_fn::array_element(target_expr, index),
            ),
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                let length = df_unicode::length(cast_utf8(target_expr.clone()));
                let absolute = df_math::abs(index.clone());
                let valid = Expr::and(
                    binary(index.clone(), BinaryOp::Neq, lit(0_i64)),
                    binary(absolute, BinaryOp::Lte, length.clone()),
                );
                let position = Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(binary(index.clone(), BinaryOp::Lt, lit(0_i64))),
                        Box::new(binary(
                            binary(length, BinaryOp::Add, index.clone()),
                            BinaryOp::Add,
                            lit(1_i64),
                        )),
                    )],
                    Some(Box::new(index.clone())),
                ));
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(valid),
                        Box::new(df_unicode::substring(
                            cast_utf8(target_expr),
                            position,
                            lit(1_i64),
                        )),
                    )],
                    Some(Box::new(lit(ScalarValue::Utf8(None)))),
                )))
            }
            DataType::Null => Ok(lit(ScalarValue::Utf8(None))),
            other => Err(RelError::Unsupported(format!(
                "cypher subscript over type {other}"
            ))),
        }
    }

    fn lower_integer_literal(&self, arg: &IrExpr) -> RelResult<Expr> {
        let Some(text) = integer_literal_text(arg) else {
            return Err(RelError::Unsupported(
                "integer_literal argument must be a literal string".into(),
            ));
        };
        if let Ok(value) = text.parse::<i64>() {
            return Ok(lit(value));
        }
        Ok(lit(rel_display_value(
            &Value::BigInt(
                BigInt::from_str(&text.replace('_', "")).map_err(|_| {
                    RelError::Unsupported(format!("invalid integer literal `{text}`"))
                })?,
            ),
            self.language,
            DisplayContext::Scalar,
        )))
    }

    fn lower_unary_math_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        arg: &IrExpr,
    ) -> RelResult<Expr> {
        let arg = self.lower_expr(plan, arg)?;
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "acos" => Ok(df_math::acos(arg)),
            "acosh" => Ok(df_math::acosh(arg)),
            "asin" => Ok(df_math::asin(arg)),
            "asinh" => Ok(df_math::asinh(arg)),
            "atan" => Ok(df_math::atan(arg)),
            "atanh" => Ok(df_math::atanh(arg)),
            "cbrt" => Ok(df_math::cbrt(arg)),
            "ceil" | "ceiling" => Ok(df_math::ceil(arg)),
            "cos" => Ok(df_math::cos(arg)),
            "cosh" => Ok(df_math::cosh(arg)),
            "cot" => Ok(df_math::cot(arg)),
            "degrees" => Ok(df_math::degrees(arg)),
            "exp" => Ok(df_math::exp(arg)),
            "factorial" => Ok(df_math::factorial(arg)),
            "floor" => Ok(df_math::floor(arg)),
            "ln" | "log" => Ok(df_math::ln(arg)),
            "log2" => Ok(df_math::log2(arg)),
            "log10" => Ok(df_math::log10(arg)),
            "radians" => Ok(df_math::radians(arg)),
            "round" => Ok(df_math::round(vec![arg])),
            "sign" | "signum" => Ok(df_math::signum(arg)),
            "sin" => Ok(df_math::sin(arg)),
            "sinh" => Ok(df_math::sinh(arg)),
            "sqrt" => Ok(df_math::sqrt(arg)),
            "tan" => Ok(df_math::tan(arg)),
            "tanh" => Ok(df_math::tanh(arg)),
            "trunc" | "truncate" => Ok(df_math::trunc(vec![arg])),
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
    }

    fn lower_binary_math_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        lhs: &IrExpr,
        rhs: &IrExpr,
    ) -> RelResult<Expr> {
        let lhs = self.lower_expr(plan, lhs)?;
        let rhs = self.lower_expr(plan, rhs)?;
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "atan2" => Ok(df_math::atan2(lhs, rhs)),
            "gcd" => Ok(df_math::gcd(lhs, rhs)),
            "lcm" => Ok(df_math::lcm(lhs, rhs)),
            "log" => Ok(df_math::log(lhs, rhs)),
            "nanvl" => Ok(df_math::nanvl(lhs, rhs)),
            "round" => Ok(df_math::round(vec![lhs, rhs])),
            "trunc" | "truncate" => Ok(df_math::trunc(vec![lhs, rhs])),
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
    }

    fn lower_string_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "concat" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::concat(args))
            }
            "concat_ws" => {
                let [delimiter, rest @ ..] = args else {
                    return Err(RelError::Unsupported("concat_ws arity".into()));
                };
                let delimiter = self.lower_expr(plan, delimiter)?;
                let rest = rest
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::concat_ws(delimiter, rest))
            }
            "contains" | "strcontains" => {
                let [value, needle] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::contains(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, needle)?),
                ))
            }
            "prefix" | "starts_with" | "startswith" => {
                let [value, prefix] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::starts_with(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, prefix)?),
                ))
            }
            "suffix" | "ends_with" | "endswith" => {
                let [value, suffix] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::ends_with(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, suffix)?),
                ))
            }
            "lcase" | "lower" | "tolower" | "gremlin_lcase" | "local_lcase" => {
                let [value] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::lower(cast_utf8(self.lower_expr(plan, value)?)))
            }
            "ucase" | "upper" | "toupper" | "gremlin_ucase" | "local_ucase" => {
                let [value] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::upper(cast_utf8(self.lower_expr(plan, value)?)))
            }
            "trim" | "local_trim" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::trim(args))
            }
            "ltrim" | "local_ltrim" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::ltrim(args))
            }
            "rtrim" | "local_rtrim" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::rtrim(args))
            }
            "replace" => {
                let [value, from, to] = args else {
                    return Err(RelError::Unsupported("replace arity".into()));
                };
                Ok(df_string::replace(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, from)?),
                    cast_utf8(self.lower_expr(plan, to)?),
                ))
            }
            "reverse" | "local_reverse_strings" => {
                let [value] = args else {
                    return Err(RelError::Unsupported("reverse arity".into()));
                };
                Ok(df_unicode::reverse(cast_utf8(
                    self.lower_expr(plan, value)?,
                )))
            }
            "left" => {
                let [value, count] = args else {
                    return Err(RelError::Unsupported("left arity".into()));
                };
                Ok(df_unicode::left(
                    cast_utf8(self.lower_expr(plan, value)?),
                    self.lower_expr(plan, count)?,
                ))
            }
            "right" => {
                let [value, count] = args else {
                    return Err(RelError::Unsupported("right arity".into()));
                };
                Ok(df_unicode::right(
                    cast_utf8(self.lower_expr(plan, value)?),
                    self.lower_expr(plan, count)?,
                ))
            }
            "substring" | "substr" => {
                let [value, start, rest @ ..] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                let start = binary(self.lower_expr(plan, start)?, BinaryOp::Add, lit(1_i64));
                let value = cast_utf8(self.lower_expr(plan, value)?);
                match rest {
                    [] => Ok(df_unicode::substr(value, start)),
                    [len] => Ok(df_unicode::substring(
                        value,
                        start,
                        self.lower_expr(plan, len)?,
                    )),
                    _ => Err(RelError::Unsupported(format!("{name} arity"))),
                }
            }
            "gremlin_substring" => {
                let [value, start, rest @ ..] = args else {
                    return Err(RelError::Unsupported("gremlin_substring arity".into()));
                };
                let value = cast_utf8(self.lower_expr(plan, value)?);
                let start_expr = self.lower_expr(plan, start)?;
                let pos = binary(start_expr.clone(), BinaryOp::Add, lit(1_i64));
                match rest {
                    [] => Ok(df_unicode::substr(value, pos)),
                    [end] => {
                        let end = self.lower_expr(plan, end)?;
                        let len = binary(end, BinaryOp::Sub, start_expr);
                        Ok(df_unicode::substring(value, pos, len))
                    }
                    _ => Err(RelError::Unsupported("gremlin_substring arity".into())),
                }
            }
            "length" | "local_length" | "char_length" | "character_length" => {
                let [value] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                // `length(p)` over a path is its hop count. The path renders
                // as text, so without this it would measure that text.
                if let IrExpr::Binding(binding) = value {
                    let hops = path_len_col(binding);
                    if has_exact_col(plan, &hops) {
                        return Ok(col_exact(hops));
                    }
                }
                Ok(df_unicode::length(cast_utf8(self.lower_expr(plan, value)?)))
            }
            "size" => {
                let [value] = args else {
                    return Err(RelError::Unsupported("size arity".into()));
                };
                if let Some(Value::List(items)) = constant_value_expr(value)? {
                    return Ok(lit(items.len() as i64));
                }
                let lowered = self.lower_expr(plan, value)?;
                let data_type = lowered.get_type(plan.schema())?;
                match data_type {
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
                        Ok(Expr::Cast(Cast::new(
                            Box::new(datafusion::functions_nested::expr_fn::array_length(lowered)),
                            DataType::Int64,
                        )))
                    }
                    DataType::Map(_, _) => Ok(Expr::Cast(Cast::new(
                        Box::new(datafusion::functions_nested::expr_fn::cardinality(lowered)),
                        DataType::Int64,
                    ))),
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                        Ok(df_unicode::length(cast_utf8(lowered)))
                    }
                    other => Err(RelError::Unsupported(format!(
                        "size over non-collection type {other}"
                    ))),
                }
            }
            "lpad" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_unicode::lpad(args))
            }
            "rpad" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_unicode::rpad(args))
            }
            "regexp_replace" => {
                let [value, pattern, replacement, rest @ ..] = args else {
                    return Err(RelError::Unsupported("regexp_replace arity".into()));
                };
                let flags = match rest {
                    [] => None,
                    [flags] => Some(cast_utf8(self.lower_expr(plan, flags)?)),
                    _ => return Err(RelError::Unsupported("regexp_replace arity".into())),
                };
                Ok(df_regex::regexp_replace(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, pattern)?),
                    cast_utf8(self.lower_expr(plan, replacement)?),
                    flags,
                ))
            }
            "regexp_full_match" | "regexp_matches" | "regexp_like" => {
                let [value, pattern, rest @ ..] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                let pattern = if normalized == "regexp_full_match" {
                    match constant_value_expr(pattern)? {
                        Some(Value::String(pattern)) => lit(format!("^({pattern})$")),
                        _ => cast_utf8(self.lower_expr(plan, pattern)?),
                    }
                } else {
                    cast_utf8(self.lower_expr(plan, pattern)?)
                };
                let flags = match rest {
                    [] => None,
                    [flags] => Some(cast_utf8(self.lower_expr(plan, flags)?)),
                    _ => return Err(RelError::Unsupported(format!("{name} arity"))),
                };
                Ok(df_regex::regexp_like(
                    cast_utf8(self.lower_expr(plan, value)?),
                    pattern,
                    flags,
                ))
            }
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
    }

    fn lower_core_variadic_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let lowered = args
            .iter()
            .map(|arg| self.lower_expr(plan, arg))
            .collect::<RelResult<Vec<_>>>()?;
        match normalize_function_name(name).as_str() {
            "coalesce" | "ifnull" => Ok(df_core::coalesce(lowered)),
            "greatest" => Ok(df_core::greatest(lowered)),
            "least" => Ok(df_core::least(lowered)),
            "nullif" if lowered.len() == 2 => {
                let value = lowered[0].clone();
                let sentinel = lowered[1].clone();
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(binary(value.clone(), BinaryOp::Eq, sentinel)),
                        Box::new(lit(ScalarValue::Null)),
                    )],
                    Some(Box::new(value)),
                )))
            }
            "constant_or_null" if lowered.len() == 2 => {
                let value = lowered[0].clone();
                let nullable = lowered[1].clone();
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(nullable.is_null()),
                        Box::new(lit(ScalarValue::Null)),
                    )],
                    Some(Box::new(value)),
                )))
            }
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
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

    /// Gremlin `valueMap()` over an element binding: renders the tagged
    /// map text (`m[{"age":"[29]","name":"[marko]"}]`) that the harness
    /// comparator normalizes identically to the interpreter's output.
    fn lower_value_map(&self, plan: &LogicalPlan, name: &str, args: &[IrExpr]) -> RelResult<Expr> {
        let IrExpr::Binding(binding) = &args[0] else {
            return Err(RelError::Unsupported(
                "value_map over a non-binding target".into(),
            ));
        };
        let Some(shape) = has_binding_shape(plan, binding) else {
            return Err(RelError::Unsupported(format!(
                "value_map target `{binding}` is not an element binding"
            )));
        };
        let requested = match args.get(1) {
            Some(IrExpr::List(items)) if !items.is_empty() => Some(
                items
                    .iter()
                    .filter_map(|item| match item {
                        IrExpr::Lit(Lit::String(key)) => Some(key.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        };
        let keys = match requested {
            Some(keys) => keys
                .into_iter()
                .filter(|key| has_exact_col(plan, &prop_col(binding, key)))
                .collect(),
            None => self.element_property_keys(plan, binding, shape),
        };
        let mut body: Vec<Expr> = Vec::new();
        if name == "value_map_tokens" {
            let literal_bool = |arg: Option<&IrExpr>, default: bool| match arg {
                Some(IrExpr::Lit(Lit::Bool(value))) => *value,
                _ => default,
            };
            if shape != BindingShape::Node {
                return Err(RelError::Unsupported(
                    "value_map_tokens over a non-node binding".into(),
                ));
            }
            if literal_bool(args.get(2), true) {
                let display = match has_exact_col(plan, &prop_col(binding, "name")) {
                    true => col_exact(prop_col(binding, "name")),
                    false => concat_exprs(vec![
                        col_exact(label_col(binding)),
                        lit("#"),
                        cast_utf8(col_exact(id_col(binding))),
                    ]),
                };
                body.push(concat_exprs(vec![
                    lit(",\"t[id]\":\"v["),
                    display,
                    lit("].id\""),
                ]));
            }
            if literal_bool(args.get(3), true) {
                body.push(concat_exprs(vec![
                    lit(",\"t[label]\":\""),
                    col_exact(label_col(binding)),
                    lit("\""),
                ]));
            }
        }
        for key in keys {
            let name = prop_col(binding, &key);
            let Some(data_type) = plan_column_type(plan, &name) else {
                continue;
            };
            let column = col_exact(&name);
            let rendered = match data_type {
                DataType::Boolean => Expr::Case(Case::new(
                    None,
                    vec![(Box::new(column.clone()), Box::new(lit("true")))],
                    Some(Box::new(lit("false"))),
                )),
                DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => column.clone(),
                _ => cast_utf8(column.clone()),
            };
            let entry = concat_exprs(vec![lit(format!(",\"{key}\":\"[")), rendered, lit("]\"")]);
            body.push(Expr::Case(Case::new(
                None,
                vec![(Box::new(column.is_null()), Box::new(lit("")))],
                Some(Box::new(entry)),
            )));
        }
        if body.is_empty() {
            return Ok(lit("m[{}]"));
        }
        let entries = cast_utf8(df_unicode::substr(concat_exprs(body), lit(2_i64)));
        Ok(concat_exprs(vec![lit("m[{"), entries, lit("}]")]))
    }

    fn lower_cypher_map(&self, plan: &LogicalPlan, args: &[IrExpr]) -> RelResult<Expr> {
        if let Some(value) = constant_cypher_map(args)? {
            return Ok(lit(rel_display_value(
                &value,
                self.language,
                DisplayContext::Tagged,
            )));
        }
        if let [IrExpr::List(keys), IrExpr::List(values)] = args {
            if keys.len() != values.len() {
                return Err(RelError::Unsupported(
                    "map key/value length mismatch".into(),
                ));
            }
            if keys
                .iter()
                .enumerate()
                .any(|(index, key)| keys[..index].contains(key))
            {
                // The duplicate-key error includes the row-dependent key.
                // Let the interpreter produce that exact public error until
                // SQL error expressions are part of the result boundary.
                return Err(RelError::Unsupported("dynamic duplicate map key".into()));
            }
            let mut pieces = vec![lit("{")];
            for (index, (key, value)) in keys.iter().zip(values).enumerate() {
                if index > 0 {
                    pieces.push(lit(", "));
                }
                pieces.push(cast_utf8(self.lower_expr(plan, key)?));
                pieces.push(lit("="));
                pieces.push(cast_utf8(self.lower_expr(plan, value)?));
            }
            pieces.push(lit("}"));
            return Ok(concat_exprs(pieces));
        }
        if args.len() % 2 != 0 {
            return Err(RelError::Unsupported("map arity".into()));
        }
        let mut pieces = Vec::new();
        pieces.push(lit("m[{"));
        for (idx, pair) in args.chunks(2).enumerate() {
            let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
                return Err(RelError::Unsupported("dynamic map key".into()));
            };
            if idx > 0 {
                pieces.push(lit(","));
            }
            pieces.push(lit(format!("\"{}\":\"", escape_debug_string(key))));
            pieces.push(cast_utf8(self.lower_expr(plan, &pair[1])?));
            pieces.push(lit("\""));
        }
        pieces.push(lit("}]"));
        Ok(concat_exprs(pieces))
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
        if let IrExpr::Call { name, args } = &key.expr
            && name == "gremlin_order_key"
            && let Some(IrExpr::Binding(binding)) = args.first()
        {
            // A relational column has one Arrow type, so its TinkerPop type
            // rank is constant within the sort. Elements order by their
            // provider scan key; scalars order by their native SQL value.
            if has_binding_shape(plan, binding).is_some() {
                return Ok(vec![
                    col_exact(label_col(binding)).sort(asc, nulls_first),
                    col_exact(id_col(binding)).sort(asc, nulls_first),
                ]);
            }
            if let Some(column) = resolve_column_name(plan, binding) {
                return Ok(vec![col_exact(column).sort(asc, nulls_first)]);
            }
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
        let mut defs = BTreeMap::<String, PropertyDef>::new();
        for label in labels {
            let table = match self.graph.node_table(label) {
                Ok(table) => table,
                Err(CatalogError::UnknownLabel(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            // Cypher fixtures use `id` as an ordinary primary-key property
            // and expect `RETURN n.*` and node printing to show it; Gremlin
            // treats element ids as separate from properties. This mirrors
            // `node_property_keys` vs `node_property_keys_with_id` — excluding
            // it unconditionally left `n.id` unresolvable, which the
            // NullOnMissing policy then turned into a silent `NULL`.
            let excluded: &[&str] = match self.language {
                Language::Gremlin => &["id"],
                _ => &[],
            };
            merge_property_defs(&mut defs, table.batch.schema().as_ref(), excluded)?;
            merge_struct_field_defs(&mut defs, &table.batch)?;
        }
        Ok(defs.into_values().collect())
    }

    /// Ordered property keys for an element binding: catalog schema order
    /// (matching the interpreter's `node_property_keys` /
    /// `edge_property_keys` iteration), filtered to the property columns
    /// actually present in the plan.
    fn element_property_keys(
        &self,
        plan: &LogicalPlan,
        binding: &str,
        shape: BindingShape,
    ) -> Vec<String> {
        let mut keys = Vec::new();
        let mut push_keys = |label_keys: Vec<String>| {
            for key in label_keys {
                if !keys.contains(&key) && has_exact_col(plan, &prop_col(binding, &key)) {
                    keys.push(key);
                }
            }
        };
        match shape {
            BindingShape::Node => {
                for label in self.graph.node_label_order() {
                    push_keys(self.graph.node_property_keys_with_id(label));
                }
            }
            BindingShape::Edge => {
                for rel_type in self.graph.edge_rel_order() {
                    push_keys(self.graph.edge_property_keys(rel_type));
                }
            }
        }
        keys
    }

    /// Render a Cypher graph element the way the interpreter's
    /// `expand_element` does: nodes as `{_ID: t:o, _LABEL: l, key: value,
    /// ...}` (null properties omitted), edges as
    /// `(st:so)-{_LABEL: r, _ID: t:o, ...}->(dt:do)`.
    fn cypher_element_display_expr(
        &self,
        plan: &LogicalPlan,
        binding: &str,
        shape: BindingShape,
    ) -> RelResult<Expr> {
        let node_index =
            |label_expr: Expr| label_index_case(label_expr, self.graph.node_label_order(), 0, 1);
        let property_segments = |parts: &mut Vec<Expr>| {
            for key in self.element_property_keys(plan, binding, shape) {
                let name = prop_col(binding, &key);
                let Some(data_type) = plan_column_type(plan, &name) else {
                    continue;
                };
                let column = col_exact(&name);
                let rendered = concat_exprs(vec![
                    lit(format!(", {key}: ")),
                    render_property_text_expr(column.clone(), &data_type),
                ]);
                parts.push(Expr::Case(Case::new(
                    None,
                    vec![(Box::new(column.is_null()), Box::new(lit("")))],
                    Some(Box::new(rendered)),
                )));
            }
        };
        match shape {
            BindingShape::Node => {
                let mut parts = vec![
                    lit("{_ID: "),
                    node_index(col_exact(label_col(binding))),
                    lit(":"),
                    cast_utf8(col_exact(id_col(binding))),
                    lit(", _LABEL: "),
                    col_exact(label_col(binding)),
                ];
                property_segments(&mut parts);
                parts.push(lit("}"));
                Ok(concat_exprs(parts))
            }
            BindingShape::Edge => {
                let mut parts = vec![
                    lit("("),
                    node_index(col_exact(src_label_col(binding))),
                    lit(":"),
                    cast_utf8(col_exact(src_id_col(binding))),
                    lit(")-{_LABEL: "),
                    col_exact(label_col(binding)),
                    lit(", _ID: "),
                    rel_index_case(col_exact(label_col(binding)), self.graph),
                    lit(":"),
                    cast_utf8(col_exact(id_col(binding))),
                ];
                property_segments(&mut parts);
                parts.push(lit("}->("));
                parts.push(node_index(col_exact(dst_label_col(binding))));
                parts.push(lit(":"));
                parts.push(cast_utf8(col_exact(dst_id_col(binding))));
                parts.push(lit(")"));
                Ok(concat_exprs(parts))
            }
        }
    }

    fn edge_property_defs(&self, rel_types: &[String]) -> RelResult<Vec<PropertyDef>> {
        let mut defs = BTreeMap::<String, PropertyDef>::new();
        for rel_type in rel_types {
            let tables = match self.graph.edge_tables(rel_type) {
                Ok(tables) => tables,
                Err(CatalogError::UnknownRelType(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            for table in tables {
                merge_property_defs(
                    &mut defs,
                    table.batch.schema().as_ref(),
                    &["src", "dst", "id", "__src_id", "__dst_id"],
                )?;
                merge_struct_field_defs(&mut defs, &table.batch)?;
            }
        }
        Ok(defs.into_values().collect())
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
    carries_union_tag: bool,
    struct_fields: Vec<String>,
}

fn merge_property_defs(
    defs: &mut BTreeMap<String, PropertyDef>,
    schema: &Schema,
    excluded: &[&str],
) -> RelResult<()> {
    for field in schema.fields() {
        if excluded.contains(&field.name().as_str()) {
            continue;
        }
        let carries_union_tag = field
            .metadata()
            .get("new_graph.value_type")
            .is_some_and(|kind| kind == "value");
        match defs.get_mut(field.name()) {
            Some(existing) if existing.data_type != *field.data_type() => {
                return Err(RelError::Unsupported(format!(
                    "property `{}` has mixed types `{:?}` and `{:?}`",
                    field.name(),
                    existing.data_type,
                    field.data_type()
                )));
            }
            Some(existing) => existing.carries_union_tag |= carries_union_tag,
            None => {
                defs.insert(
                    field.name().clone(),
                    PropertyDef {
                        name: field.name().clone(),
                        data_type: field.data_type().clone(),
                        carries_union_tag,
                        struct_fields: Vec::new(),
                    },
                );
            }
        }
    }
    Ok(())
}

fn merge_struct_field_defs(
    defs: &mut BTreeMap<String, PropertyDef>,
    batch: &RecordBatch,
) -> RelResult<()> {
    use arrow::array::Array as _;
    for (name, def) in defs.iter_mut() {
        let Some(index) = schema_index(batch.schema().as_ref(), name) else {
            continue;
        };
        let field = batch.schema().field(index).clone();
        if !field
            .metadata()
            .get("new_graph.value_type")
            .is_some_and(|kind| kind == "value")
        {
            continue;
        }
        let source = batch
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                RelError::Unsupported(format!("structured property `{name}` is not text"))
            })?;
        for row in 0..source.len() {
            if source.is_null(row) {
                continue;
            }
            let Some(Value::Map(map)) = crate::ir::catalog::parse_debug_value(source.value(row))
            else {
                continue;
            };
            if map.contains_key("__tag") || map.keys().any(|key| key.starts_with('\0')) {
                continue;
            }
            let keys = match map.get(STRUCT_ORDER_KEY) {
                Some(Value::List(order)) => order
                    .iter()
                    .filter_map(|value| match value {
                        Value::String(key) if map.contains_key(key) => Some(key.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                _ => map
                    .keys()
                    .filter(|key| !key.starts_with("__"))
                    .cloned()
                    .collect(),
            };
            for key in keys {
                if !def.struct_fields.contains(&key) {
                    def.struct_fields.push(key);
                }
            }
            break;
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
        if prop.carries_union_tag {
            fields.push(Field::new(
                union_tag_col(binding, &prop.name),
                DataType::Utf8,
                true,
            ));
        }
        for field in &prop.struct_fields {
            fields.push(Field::new(
                struct_field_col(binding, &prop.name, field),
                DataType::Utf8,
                true,
            ));
        }
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
        if prop.carries_union_tag {
            fields.push(Field::new(
                union_tag_col(binding, &prop.name),
                DataType::Utf8,
                true,
            ));
        }
        for field in &prop.struct_fields {
            fields.push(Field::new(
                struct_field_col(binding, &prop.name, field),
                DataType::Utf8,
                true,
            ));
        }
    }
    Arc::new(Schema::new(fields))
}

fn normalize_node_table(
    binding: &str,
    table: &NodeTable,
    props: &[PropertyDef],
    schema: SchemaRef,
    language: Language,
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
            language,
        )?);
        if prop.carries_union_tag {
            arrays.push(property_union_tag_array(&table.batch, &prop.name, rows)?);
        }
        for field in &prop.struct_fields {
            arrays.push(property_struct_field_array(
                &table.batch,
                &prop.name,
                field,
                rows,
                language,
            )?);
        }
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
    language: Language,
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
            language,
        )?);
        if prop.carries_union_tag {
            arrays.push(property_union_tag_array(&table.batch, &prop.name, rows)?);
        }
        for field in &prop.struct_fields {
            arrays.push(property_struct_field_array(
                &table.batch,
                &prop.name,
                field,
                rows,
                language,
            )?);
        }
    }
    debug_assert_eq!(schema.field(0).name(), &id_col(binding));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

fn property_array(
    batch: &RecordBatch,
    name: &str,
    expected: &DataType,
    rows: usize,
    language: Language,
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
            // Structured (list / map) properties are stored as
            // debug-encoded strings; decode them to the display text the
            // interpreter would print so downstream projections and
            // comparisons see the same rendering.
            let is_encoded = field
                .metadata()
                .get("new_graph.value_type")
                .is_some_and(|kind| kind == "map" || kind == "value");
            if is_encoded {
                let source = batch
                    .column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| {
                        RelError::Unsupported(format!(
                            "encoded property `{name}` is not a string column"
                        ))
                    })?;
                use arrow::array::Array as _;
                let mut builder = StringBuilder::new();
                for row in 0..source.len() {
                    if source.is_null(row) {
                        builder.append_null();
                        continue;
                    }
                    let raw = source.value(row);
                    match crate::ir::catalog::parse_debug_value(raw) {
                        Some(value) => builder.append_value(rel_display_value(
                            &value,
                            language,
                            literal_collection_context(language),
                        )),
                        None => builder.append_value(raw),
                    }
                }
                return Ok(Arc::new(builder.finish()) as ArrayRef);
            }
            Ok(batch.column(idx).clone())
        }
        None => Ok(new_null_array(expected, rows)),
    }
}

fn property_union_tag_array(batch: &RecordBatch, name: &str, rows: usize) -> RelResult<ArrayRef> {
    let Some(idx) = schema_index(batch.schema().as_ref(), name) else {
        return Ok(new_null_array(&DataType::Utf8, rows));
    };
    let source = batch
        .column(idx)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| {
            RelError::Unsupported(format!(
                "union-valued property `{name}` is not a string column"
            ))
        })?;
    use arrow::array::Array as _;
    let mut builder = StringBuilder::new();
    for row in 0..source.len() {
        if source.is_null(row) {
            builder.append_null();
            continue;
        }
        let tag = crate::ir::catalog::parse_debug_value(source.value(row)).and_then(|value| {
            let Value::Map(map) = value else {
                return None;
            };
            match map.get("__tag") {
                Some(Value::String(tag)) => Some(tag.clone()),
                _ => None,
            }
        });
        match tag {
            Some(tag) => builder.append_value(tag),
            None => builder.append_null(),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
}

fn property_struct_field_array(
    batch: &RecordBatch,
    name: &str,
    struct_field: &str,
    rows: usize,
    language: Language,
) -> RelResult<ArrayRef> {
    let Some(idx) = schema_index(batch.schema().as_ref(), name) else {
        return Ok(new_null_array(&DataType::Utf8, rows));
    };
    let source = batch
        .column(idx)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| {
            RelError::Unsupported(format!("structured property `{name}` is not text"))
        })?;
    use arrow::array::Array as _;
    let mut builder = StringBuilder::new();
    for row in 0..source.len() {
        if source.is_null(row) {
            builder.append_null();
            continue;
        }
        let value = crate::ir::catalog::parse_debug_value(source.value(row)).and_then(|value| {
            let Value::Map(map) = value else {
                return None;
            };
            map.get(struct_field).cloned()
        });
        match value {
            Some(Value::Null) | None => builder.append_null(),
            Some(value) => builder.append_value(rel_display_value(
                &value,
                language,
                literal_collection_context(language),
            )),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
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

fn values_batch(
    language: Language,
    bindings: &[String],
    rows: &[Vec<Value>],
) -> RelResult<RecordBatch> {
    if rows.iter().any(|row| row.len() != bindings.len()) {
        return Err(RelError::Unsupported(
            "GraphValues row width does not match bindings".into(),
        ));
    }
    let types = (0..bindings.len())
        .map(|idx| {
            let values = rows.iter().map(|row| &row[idx]).collect::<Vec<_>>();
            infer_value_type(&values)
        })
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
        .map(|(idx, data_type)| values_array(language, rows.iter().map(|row| &row[idx]), data_type))
        .collect::<RelResult<Vec<_>>>()?;
    Ok(RecordBatch::try_new(schema, arrays)?)
}

fn infer_value_type(values: &[&Value]) -> RelResult<DataType> {
    if values.iter().any(|value| matches!(value, Value::List(_)))
        && values
            .iter()
            .all(|value| matches!(value, Value::Null | Value::List(_)))
    {
        let elements = values
            .iter()
            .flat_map(|value| match value {
                Value::List(items) => items.iter().collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect::<Vec<_>>();
        let element_type = if elements.iter().any(|value| {
            matches!(
                value,
                Value::String(_)
                    | Value::DateTime(_)
                    | Value::BigInt(_)
                    | Value::UInt128(_)
                    | Value::BigDecimal(_)
                    | Value::InternalId { .. }
                    | Value::Node { .. }
                    | Value::Edge { .. }
                    | Value::Map(_)
                    | Value::Path(_)
                    | Value::List(_)
            )
        }) {
            DataType::Utf8
        } else {
            infer_value_type(&elements)?
        };
        return Ok(DataType::List(Arc::new(Field::new(
            "item",
            element_type,
            true,
        ))));
    }
    let mut data_type = DataType::Utf8;
    for value in values.iter().copied() {
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
            Value::BigInt(_)
            | Value::UInt128(_)
            | Value::BigDecimal(_)
            | Value::InternalId { .. }
            | Value::Node { .. }
            | Value::Edge { .. }
            | Value::List(_)
            | Value::Map(_)
            | Value::Path(_) => {
                data_type = DataType::Utf8;
                break;
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
    language: Language,
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
                    other => builder.append_value(graph_values_display(other, language)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::List(field) => match field.data_type() {
            DataType::Boolean => {
                let mut builder = ListBuilder::new(BooleanBuilder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    Value::Bool(value) => builder.values().append_value(*value),
                                    other => {
                                        return Err(RelError::Unsupported(format!(
                                            "cannot put `{}` in Boolean GraphValues list",
                                            other.type_name()
                                        )));
                                    }
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Int64 => {
                let mut builder = ListBuilder::new(Int64Builder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    item => match item.as_i64() {
                                        Some(value) => builder.values().append_value(value),
                                        None => {
                                            return Err(RelError::Unsupported(format!(
                                                "cannot put `{}` in Int64 GraphValues list",
                                                item.type_name()
                                            )));
                                        }
                                    },
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Float64 => {
                let mut builder = ListBuilder::new(Float64Builder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    Value::Float(value) => builder.values().append_value(*value),
                                    Value::Float32(value) => {
                                        builder.values().append_value(f64::from(*value))
                                    }
                                    item => match item.as_i64() {
                                        Some(value) => builder.values().append_value(value as f64),
                                        None => {
                                            return Err(RelError::Unsupported(format!(
                                                "cannot put `{}` in Float64 GraphValues list",
                                                item.type_name()
                                            )));
                                        }
                                    },
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Utf8 => {
                let mut builder = ListBuilder::new(StringBuilder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    Value::String(value) | Value::DateTime(value) => {
                                        builder.values().append_value(value)
                                    }
                                    other => builder
                                        .values()
                                        .append_value(graph_values_display(other, language)),
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            other => Err(RelError::Unsupported(format!(
                "GraphValues list element type `{other:?}`"
            ))),
        },
        other => Err(RelError::Unsupported(format!(
            "GraphValues type `{other:?}`"
        ))),
    }
}

fn lit_to_expr(value: &Lit) -> Expr {
    match value {
        Lit::Null => lit(ScalarValue::Null),
        Lit::Bool(value) => lit(*value),
        Lit::Int(value) => lit(*value),
        Lit::Float(value) => lit(*value),
        Lit::String(value) => lit(value.clone()),
    }
}

fn value_literal_expr(value: &Value) -> RelResult<Expr> {
    match value {
        Value::Null => Ok(lit(ScalarValue::Null)),
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

/// Whether an IR expression can be evaluated without any row context
/// (modulo `bound` iteration variables introduced by list lambdas).
fn expr_is_constant(expr: &IrExpr, bound: &[&str]) -> bool {
    match expr {
        IrExpr::Lit(_) => true,
        IrExpr::Binding(binding) => bound.contains(&binding.as_str()),
        IrExpr::List(items) => items.iter().all(|item| expr_is_constant(item, bound)),
        IrExpr::Binary { lhs, rhs, .. } => {
            expr_is_constant(lhs, bound) && expr_is_constant(rhs, bound)
        }
        IrExpr::Not(inner) | IrExpr::IsNull(inner) | IrExpr::IsNotNull(inner) => {
            expr_is_constant(inner, bound)
        }
        IrExpr::StringPredicate {
            target, pattern, ..
        } => expr_is_constant(target, bound) && expr_is_constant(pattern, bound),
        IrExpr::Case { arms, otherwise } => {
            arms.iter()
                .all(|(when, then)| expr_is_constant(when, bound) && expr_is_constant(then, bound))
                && otherwise
                    .as_deref()
                    .is_none_or(|expr| expr_is_constant(expr, bound))
        }
        IrExpr::ListTransform { list, item, map } => {
            expr_is_constant(list, bound) && {
                let mut inner = bound.to_vec();
                inner.push(item.as_str());
                expr_is_constant(map, &inner)
            }
        }
        IrExpr::ListFilter {
            list,
            item,
            predicate,
        } => {
            expr_is_constant(list, bound) && {
                let mut inner = bound.to_vec();
                inner.push(item.as_str());
                expr_is_constant(predicate, &inner)
            }
        }
        IrExpr::ListReduce {
            collection,
            accumulator,
            item,
            map,
        } => {
            expr_is_constant(collection, bound) && {
                let mut inner = bound.to_vec();
                inner.push(accumulator.as_str());
                inner.push(item.as_str());
                expr_is_constant(map, &inner)
            }
        }
        IrExpr::Call { name, args } => {
            constant_foldable_function(name) && args.iter().all(|arg| expr_is_constant(arg, bound))
        }
        _ => false,
    }
}

/// Functions safe to fold at lowering time: deterministic, side-effect
/// free, and not tied to internal traversal state.
fn constant_foldable_function(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    if normalized.starts_with("__") {
        return false;
    }
    const DENY: &[&str] = &[
        "rand",
        "random",
        "uuid",
        "gen_random_uuid",
        "now",
        "nextval",
        "currval",
    ];
    if DENY.contains(&normalized.as_str()) {
        return false;
    }
    const DENY_PREFIX: &[&str] = &[
        "current_",
        "select_",
        "path_",
        "history_",
        "cypher_property",
    ];
    !DENY_PREFIX
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
}

/// Kuzu-style engine errors ("Conversion exception: ...") are expected
/// outcomes for some cases; internal interpreter errors are not and should
/// fall back to relational lowering.
fn looks_like_engine_error(message: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "Conversion exception",
        "Overflow exception",
        "Runtime exception",
        "Binder exception",
        "Parser exception",
        "Catalog exception",
        "RuntimeError",
        "SyntaxError",
    ];
    PREFIXES.iter().any(|prefix| message.starts_with(prefix)) || message.contains(" exception:")
}

fn constant_fold_result_expr(value: &Value, language: Language) -> Expr {
    match value {
        Value::Null => lit(ScalarValue::Utf8(None)),
        Value::Bool(value) => lit(*value),
        Value::Byte(value) => lit(*value as i64),
        Value::UInt8(value) => lit(*value as i64),
        Value::Short(value) => lit(*value as i64),
        Value::UInt16(value) => lit(*value as i64),
        Value::Int(value) | Value::Long(value) => lit(*value),
        Value::UInt32(value) => lit(*value as i64),
        Value::UInt64(value) => match i64::try_from(*value) {
            Ok(value) => lit(value),
            Err(_) => lit(value.to_string()),
        },
        Value::Float32(value) => lit(*value as f64),
        Value::Float(value) => lit(*value),
        Value::String(value) | Value::DateTime(value) => lit(value.clone()),
        Value::BigInt(value) => match value.to_i64() {
            Some(value) => lit(value),
            None => lit(value.to_string()),
        },
        other => constant_result_expr(other, language, literal_collection_context(language)),
    }
}

fn constant_value_expr(expr: &IrExpr) -> RelResult<Option<Value>> {
    match expr {
        IrExpr::Lit(value) => Ok(Some(lit_to_value(value))),
        IrExpr::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let Some(value) = constant_value_expr(item)? else {
                    return Ok(None);
                };
                values.push(value);
            }
            Ok(Some(Value::List(values)))
        }
        IrExpr::Binary {
            op: BinaryOp::Sub,
            lhs,
            rhs,
        } if matches!(lhs.as_ref(), IrExpr::Lit(Lit::Int(0))) => Ok(constant_value_expr(rhs)?
            .and_then(|value| match value {
                Value::Byte(value) => value.checked_neg().map(Value::Byte),
                Value::Short(value) => value.checked_neg().map(Value::Short),
                Value::Int(value) => value.checked_neg().map(Value::Int),
                Value::Long(value) => value.checked_neg().map(Value::Long),
                Value::Float32(value) => Some(Value::Float32(-value)),
                Value::Float(value) => Some(Value::Float(-value)),
                Value::BigInt(value) => Some(Value::BigInt(-value)),
                Value::BigDecimal(value) => Some(Value::BigDecimal(-value)),
                _ => None,
            })),
        IrExpr::Call { name, args } if name.eq_ignore_ascii_case("range") => {
            Ok(Some(Value::List(constant_range_values(args)?)))
        }
        IrExpr::Call { name, args } if name == "integer_literal" && args.len() == 1 => {
            let Some(text) = integer_literal_text(&args[0]) else {
                return Ok(None);
            };
            if let Ok(value) = text.replace('_', "").parse::<i64>() {
                Ok(Some(Value::Int(value)))
            } else {
                let value = BigInt::from_str(&text.replace('_', "")).map_err(|_| {
                    RelError::Unsupported(format!("invalid integer literal `{text}`"))
                })?;
                Ok(Some(Value::BigInt(value)))
            }
        }
        IrExpr::Call { name, args } if is_date_constructor(name) => {
            constant_temporal_value(name, args)
        }
        IrExpr::Call { name, args } if is_cast_function(name, args) => {
            let Some(value) = constant_value_expr(
                args.first()
                    .ok_or_else(|| RelError::Unsupported(format!("{name} arity")))?,
            )?
            else {
                return Ok(None);
            };
            let Some(target) = constant_cast_target(name, args)? else {
                return Ok(None);
            };
            constant_cast_value(&value, &target)
        }
        IrExpr::Call { name, args } if name == "map" => constant_cypher_map(args),
        _ => Ok(None),
    }
}

fn union_constructor_field(args: &[IrExpr]) -> RelResult<(&str, &IrExpr)> {
    let [IrExpr::Call { name, args }] = args else {
        return Err(RelError::Unsupported(
            "union_value expects one named argument".into(),
        ));
    };
    if name != "map" {
        return Err(RelError::Unsupported(
            "union_value expects one named argument".into(),
        ));
    }
    let [IrExpr::Lit(Lit::String(tag)), value] = args.as_slice() else {
        return Err(RelError::Unsupported(
            "union_value expects one named argument".into(),
        ));
    };
    Ok((tag, value))
}

fn constant_values(args: &[IrExpr]) -> RelResult<Option<Vec<Value>>> {
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        let Some(value) = constant_value_expr(arg)? else {
            return Ok(None);
        };
        values.push(value);
    }
    Ok(Some(values))
}

fn constant_temporal_value(name: &str, args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [arg] = args else {
        return Ok(None);
    };
    let Some(value) = constant_value_expr(arg)? else {
        return Ok(None);
    };
    let normalized = normalize_function_name(name);
    match (normalized.as_str(), value) {
        ("date" | "to_date" | "timestamp", Value::String(value) | Value::DateTime(value)) => {
            Ok(Some(Value::DateTime(value)))
        }
        ("interval" | "duration", Value::String(value)) => Ok(Some(Value::String(value))),
        (_, Value::Null) => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

fn constant_collection_function_value(name: &str, args: &[IrExpr]) -> RelResult<Option<Value>> {
    let normalized = normalize_function_name(name);
    match normalized.as_str() {
        "array_slice" | "list_slice" => constant_array_slice(args),
        "array_append" | "array_push_back" => constant_list_append(args, false),
        "array_prepend" | "array_push_front" => constant_list_append(args, true),
        "array_indexof" | "array_position" | "list_indexof" | "list_position" => {
            constant_list_position(args)
        }
        "array_contains" | "array_has" | "list_contains" | "list_has" => {
            constant_list_contains(args)
        }
        "element_at" | "list_element" | "list_extract" => constant_list_extract(args),
        "list_any_value" => constant_list_any_value(args),
        "list_distinct" => constant_list_distinct(args),
        "list_has_all" => constant_list_has_all(args),
        "list_product" => constant_list_product(args),
        "list_reverse" => constant_list_reverse(args),
        "list_sort" => constant_list_sort(args, false),
        "list_reverse_sort" => constant_list_sort(args, true),
        "list_sum" => constant_list_sum(args),
        "list_to_string" | "list_join" => constant_list_to_string(args),
        "list_unique" => constant_list_unique(args),
        "list_append" => constant_list_append(args, false),
        "list_prepend" => constant_list_append(args, true),
        "list_cat" | "list_concat" | "array_cat" | "array_concat" => constant_list_concat(args),
        "map_keys" => constant_map_keys(args),
        _ => Ok(None),
    }
}

fn constant_array_slice(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [target, start, end] = args else {
        return Ok(None);
    };
    let Some(target) = constant_value_expr(target)? else {
        return Ok(None);
    };
    let Some(start) = constant_value_expr(start)? else {
        return Ok(None);
    };
    let Some(end) = constant_value_expr(end)? else {
        return Ok(None);
    };
    Ok(Some(match target {
        Value::List(items) | Value::Path(items) => {
            Value::List(list_slice_range(&items, &start, &end))
        }
        Value::String(value) => Value::String(string_slice_range(&value, &start, &end)),
        Value::Null => Value::Null,
        _ => return Ok(None),
    }))
}

fn constant_list_extract(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [target, index] = args else {
        return Ok(None);
    };
    let Some(target) = constant_value_expr(target)? else {
        return Ok(None);
    };
    let Some(index) = constant_value_expr(index)? else {
        return Ok(None);
    };
    let Some(index) = index.as_i64() else {
        return Ok(Some(Value::Null));
    };
    Ok(Some(match target {
        Value::List(items) | Value::Path(items) => list_element_1_based(&items, index),
        Value::String(value) => string_index_1_based(&value, index),
        Value::Null => Value::Null,
        _ => return Ok(None),
    }))
}

fn constant_list_position(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items, needle] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let Some(needle) = constant_value_expr(needle)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if matches!(needle, Value::Null) {
        return Ok(Some(Value::Null));
    }
    for (idx, item) in items.iter().enumerate() {
        if list_semantic_eq(item, &needle) {
            return Ok(Some(Value::Long((idx + 1) as i64)));
        }
    }
    Ok(Some(Value::Long(0)))
}

fn constant_list_contains(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items, needle] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let Some(needle) = constant_value_expr(needle)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if matches!(needle, Value::Null) {
        return Ok(Some(Value::Null));
    }
    Ok(Some(Value::Bool(
        items.iter().any(|item| list_semantic_eq(item, &needle)),
    )))
}

fn constant_list_distinct(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(items) | Value::Path(items) => {
            Ok(Some(Value::List(list_distinct_values(&items, false))))
        }
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

fn constant_list_unique(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(items) | Value::Path(items) => Ok(Some(Value::Int(
            list_distinct_values(&items, false).len() as i64,
        ))),
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

fn constant_list_any_value(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(items) | Value::Path(items) => Ok(Some(
            items
                .into_iter()
                .find(|item| !matches!(item, Value::Null))
                .unwrap_or(Value::Null),
        )),
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

fn constant_list_has_all(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [haystack, needles] = args else {
        return Ok(None);
    };
    let Some(haystack) = constant_value_expr(haystack)? else {
        return Ok(None);
    };
    let Some(needles) = constant_value_expr(needles)? else {
        return Ok(None);
    };
    let haystack = match haystack {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let needles = match needles {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    for needle in &needles {
        if matches!(needle, Value::Null) {
            continue;
        }
        if !haystack.iter().any(|item| list_semantic_eq(item, needle)) {
            return Ok(Some(Value::Bool(false)));
        }
    }
    Ok(Some(Value::Bool(true)))
}

fn constant_list_reverse(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(mut items) | Value::Path(mut items) => {
            items.reverse();
            Ok(Some(Value::List(items)))
        }
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

fn constant_list_sum(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let mut sum = 0.0;
    let mut int_only = true;
    for item in &items {
        if matches!(item, Value::Null) {
            continue;
        }
        let Some(value) = value_to_f64(item) else {
            return Ok(Some(Value::String(format!(
                "Binder exception: Unsupported inner data type for LIST_SUM: {}",
                item.type_name().to_ascii_uppercase()
            ))));
        };
        if matches!(item, Value::Float(_) | Value::Float32(_)) {
            int_only = false;
        }
        sum += value;
    }
    Ok(Some(if int_only {
        Value::Long(sum as i64)
    } else {
        Value::Float(sum)
    }))
}

fn constant_list_product(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .any(|item| value_to_bigint(item).is_none() && value_to_f64(item).is_none())
    {
        return Ok(Some(Value::String(
            "Binder exception: Unsupported inner data type for LIST_PRODUCT: STRING".to_string(),
        )));
    }
    if items.iter().any(|item| matches!(item, Value::Float(_))) {
        return Ok(Some(Value::Float(
            items
                .iter()
                .filter_map(value_to_f64)
                .fold(1.0, |product, value| product * value),
        )));
    }
    if items.iter().any(|item| matches!(item, Value::Float32(_))) {
        let product = items
            .iter()
            .filter_map(value_to_f64)
            .fold(1.0_f32, |product, value| product * value as f32);
        return Ok(Some(Value::Float32(product)));
    }
    let product = items
        .iter()
        .filter_map(value_to_bigint)
        .fold(BigInt::from(1), |product, value| product * value);
    Ok(Some(
        product
            .to_i64()
            .map(Value::Long)
            .unwrap_or(Value::BigInt(product)),
    ))
}

fn constant_list_to_string(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [first, second] = args else {
        return Ok(None);
    };
    let Some(first) = constant_value_expr(first)? else {
        return Ok(None);
    };
    let Some(second) = constant_value_expr(second)? else {
        return Ok(None);
    };
    let (items, delimiter) = match (&first, &second) {
        (Value::List(items) | Value::Path(items), Value::String(delimiter)) => {
            (items.clone(), delimiter.clone())
        }
        (Value::String(delimiter), Value::List(items) | Value::Path(items)) => {
            (items.clone(), delimiter.clone())
        }
        (Value::Null, _) | (_, Value::Null) => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let parts = items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .map(display_for_list_to_string)
        .collect::<Vec<_>>();
    Ok(Some(Value::String(parts.join(&delimiter))))
}

fn constant_list_sort(args: &[IrExpr], reverse_default: bool) -> RelResult<Option<Value>> {
    let Some(values) = constant_values(args)? else {
        return Ok(None);
    };
    let [items, rest @ ..] = values.as_slice() else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let (descending, nulls_last) = if reverse_default {
        match rest {
            [] => (true, false),
            [Value::String(nulls)] => (true, nulls.eq_ignore_ascii_case("NULLS LAST")),
            _ => return Ok(None),
        }
    } else {
        match rest {
            [] => (false, false),
            [Value::String(dir)] => (dir.eq_ignore_ascii_case("DESC"), false),
            [Value::String(dir), Value::String(nulls)] => (
                dir.eq_ignore_ascii_case("DESC"),
                nulls.eq_ignore_ascii_case("NULLS LAST"),
            ),
            _ => return Ok(None),
        }
    };
    Ok(Some(Value::List(sort_list_values(
        items, descending, nulls_last,
    ))))
}

/// Element-id columns of `plan`, in schema order, as ascending sort keys.
///
/// These reconstruct the row order direct evaluation would have seen, which
/// is what an unordered SQL aggregate otherwise loses.
fn scan_order_keys(plan: &LogicalPlan) -> Vec<datafusion::logical_expr::SortExpr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| field.name().ends_with(ID_SUFFIX) && !field.name().contains(PROP_MARKER))
        .map(|field| col_exact(field.name()).sort(true, false))
        .collect()
}

fn count_input_rows(plan: &LogicalPlan) -> Expr {
    let Some(field) = plan.schema().fields().first() else {
        return count_all();
    };
    // Referencing an input column is intentional. DataFusion's SQL unparser
    // otherwise emits `SELECT count(1)` without a FROM clause for some
    // cross-join plans. Coalescing preserves COUNT(*) semantics for nulls.
    df_count(df_core::coalesce(vec![
        cast_utf8(col_exact(field.name())),
        lit(""),
    ]))
}

/// Preserve the original blob display text while ordering its `\\xNN`
/// escapes as non-ASCII bytes rather than as a leading backslash.
fn blob_extreme(value: Expr, maximum: bool) -> Expr {
    let key = df_string::replace(value.clone(), lit("\\x"), lit("\u{00ff}"));
    let packed = concat_exprs(vec![key, lit("\u{1}"), value]);
    let extreme = if maximum {
        df_max(packed)
    } else {
        df_min(packed)
    };
    df_string::split_part(extreme, lit("\u{1}"), lit(2_i64))
}

/// Did lowering the right side of an `Apply` pull the left side in?
///
/// Correlated right sides reach the left through `GraphCorrelate`, so every
/// left column reappears in the right plan's schema. An uncorrelated right
/// side carries none of them, and the two need joining instead.
fn absorbed_correlation(left: &LogicalPlan, right: &LogicalPlan) -> bool {
    let right_names: BTreeSet<&str> = right
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().as_str())
        .collect();
    left.schema()
        .fields()
        .iter()
        .filter(|field| field.name() != "__w_one_row")
        .all(|field| right_names.contains(field.name().as_str()))
}

/// Apply `DISTINCT` to an aggregate call when the query asked for it.
fn distinct_if(expr: Expr, distinct: bool) -> RelResult<Expr> {
    if distinct {
        Ok(expr.distinct().build()?)
    } else {
        Ok(expr)
    }
}

fn constant_list_append(args: &[IrExpr], prepend: bool) -> RelResult<Option<Value>> {
    let [items, item] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let Some(item) = constant_value_expr(item)? else {
        return Ok(None);
    };
    let mut items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if prepend {
        items.insert(0, item);
    } else {
        items.push(item);
    }
    Ok(Some(Value::List(items)))
}

fn constant_list_concat(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [left, right] = args else {
        return Ok(None);
    };
    let Some(left) = constant_value_expr(left)? else {
        return Ok(None);
    };
    let Some(right) = constant_value_expr(right)? else {
        return Ok(None);
    };
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Ok(Some(Value::Null)),
        (Value::List(mut left), Value::List(right))
        | (Value::List(mut left), Value::Path(right))
        | (Value::Path(mut left), Value::List(right))
        | (Value::Path(mut left), Value::Path(right)) => {
            left.extend(right);
            Ok(Some(Value::List(left)))
        }
        _ => Ok(None),
    }
}

fn constant_map_keys(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [target] = args else {
        return Ok(None);
    };
    let Some(target) = constant_value_expr(target)? else {
        return Ok(None);
    };
    Ok(Some(match target {
        Value::Map(map) => Value::List(
            visible_map_keys(&map)
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
        Value::Null => Value::Null,
        _ => Value::List(Vec::new()),
    }))
}

fn constant_unwind_values(expr: &IrExpr, outer: bool) -> RelResult<Option<Vec<Value>>> {
    let mut values = match expr {
        IrExpr::Lit(Lit::Null) => Vec::new(),
        IrExpr::Lit(value) => vec![lit_to_value(value)],
        IrExpr::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let Some(value) = constant_value_expr(item)? else {
                    return Ok(None);
                };
                values.push(value);
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
    } else {
        1
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

fn constant_cast_target(name: &str, args: &[IrExpr]) -> RelResult<Option<String>> {
    let normalized = normalize_function_name(name);
    if normalized == "cast" {
        let Some(target) = args.get(1) else {
            return Err(RelError::Unsupported("cast arity".into()));
        };
        let Some(Value::String(target)) = constant_value_expr(target)? else {
            return Ok(None);
        };
        return Ok(Some(target));
    }
    Ok(Some(
        cast_target_from_function_name(&normalized)?.to_string(),
    ))
}

fn constant_cast_value(value: &Value, target: &str) -> RelResult<Option<Value>> {
    if matches!(value, Value::Null) {
        return Ok(Some(Value::Null));
    }
    let normalized = target
        .trim()
        .trim_matches('"')
        .to_ascii_uppercase()
        .replace(' ', "");
    if normalized.contains('[') {
        return Ok(Some(value.clone()));
    }
    let out = match normalized.as_str() {
        "BOOL" | "BOOLEAN" => match value {
            Value::Bool(value) => Value::Bool(*value),
            Value::String(value) if value.eq_ignore_ascii_case("true") => Value::Bool(true),
            Value::String(value) if value.eq_ignore_ascii_case("false") => Value::Bool(false),
            _ => return Ok(None),
        },
        "INT8" => Value::Byte(
            cast_bigint_range(value, -128_i128, 127_i128)?
                .to_i64()
                .unwrap() as i8,
        ),
        "INT16" => Value::Short(
            cast_bigint_range(value, -32768_i128, 32767_i128)?
                .to_i64()
                .unwrap() as i16,
        ),
        "INT32" => Value::Int(
            cast_bigint_range(value, i32::MIN as i128, i32::MAX as i128)?
                .to_i64()
                .unwrap(),
        ),
        "INT64" | "SERIAL" => Value::Long(
            cast_bigint_range(value, i64::MIN as i128, i64::MAX as i128)?
                .to_i64()
                .unwrap(),
        ),
        "INT128" => Value::BigInt(
            value_to_bigint(value)
                .ok_or_else(|| RelError::Unsupported("constant INT128 cast".into()))?,
        ),
        "UINT8" => Value::UInt8(
            cast_bigint_range(value, 0_i128, u8::MAX as i128)?
                .to_u8()
                .unwrap(),
        ),
        "UINT16" => Value::UInt16(
            cast_bigint_range(value, 0_i128, u16::MAX as i128)?
                .to_u16()
                .unwrap(),
        ),
        "UINT32" => Value::UInt32(
            cast_bigint_range(value, 0_i128, u32::MAX as i128)?
                .to_u32()
                .unwrap(),
        ),
        "UINT64" => Value::UInt64(
            cast_bigint_range_big_max(value, BigInt::from(0), BigInt::from(u64::MAX))?
                .to_u64()
                .unwrap(),
        ),
        "UINT128" => {
            let value = value_to_bigint(value)
                .ok_or_else(|| RelError::Unsupported("constant UINT128 cast".into()))?;
            if value < BigInt::from(0) {
                return Ok(None);
            }
            Value::UInt128(value)
        }
        "FLOAT" => Value::Float32(
            value_to_f64(value)
                .ok_or_else(|| RelError::Unsupported("constant FLOAT cast".into()))?
                as f32,
        ),
        "DOUBLE" | "FLOAT64" => Value::Float(
            value_to_f64(value)
                .ok_or_else(|| RelError::Unsupported("constant DOUBLE cast".into()))?,
        ),
        "STRING" | "VARCHAR" => Value::String(cypher_plain_value(value)),
        "DATE" | "TIMESTAMP" | "TIMESTAMP_NS" | "TIMESTAMP_MS" | "TIMESTAMP_SEC"
        | "TIMESTAMP_S" | "TIMESTAMP_TZ" => match value {
            Value::String(value) | Value::DateTime(value) => Value::DateTime(value.clone()),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    Ok(Some(out))
}

fn cast_bigint_range(value: &Value, min: i128, max: i128) -> RelResult<BigInt> {
    cast_bigint_range_big_max(value, BigInt::from(min), BigInt::from(max))
}

fn cast_bigint_range_big_max(value: &Value, min: BigInt, max: BigInt) -> RelResult<BigInt> {
    let value = value_to_bigint(value)
        .ok_or_else(|| RelError::Unsupported("constant integer cast".into()))?;
    if value < min || value > max {
        return Err(RelError::Unsupported(
            "constant integer cast out of range".into(),
        ));
    }
    Ok(value)
}

fn value_to_bigint(value: &Value) -> Option<BigInt> {
    match value {
        Value::Byte(value) => Some(BigInt::from(*value)),
        Value::UInt8(value) => Some(BigInt::from(*value)),
        Value::Short(value) => Some(BigInt::from(*value)),
        Value::UInt16(value) => Some(BigInt::from(*value)),
        Value::Int(value) | Value::Long(value) => Some(BigInt::from(*value)),
        Value::UInt32(value) => Some(BigInt::from(*value)),
        Value::UInt64(value) => Some(BigInt::from(*value)),
        Value::BigInt(value) | Value::UInt128(value) => Some(value.clone()),
        Value::BigDecimal(value) => value.to_i128().map(BigInt::from),
        Value::Bool(true) => Some(BigInt::from(1)),
        Value::Bool(false) => Some(BigInt::from(0)),
        Value::Float32(value) => Some(BigInt::from((*value as f64).round() as i64)),
        Value::Float(value) => Some(BigInt::from(value.round() as i64)),
        Value::String(value) => BigInt::from_str(value).ok(),
        _ => None,
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Byte(value) => Some(*value as f64),
        Value::UInt8(value) => Some(*value as f64),
        Value::Short(value) => Some(*value as f64),
        Value::UInt16(value) => Some(*value as f64),
        Value::Int(value) | Value::Long(value) => Some(*value as f64),
        Value::UInt32(value) => Some(*value as f64),
        Value::UInt64(value) => Some(*value as f64),
        Value::Float32(value) => Some(*value as f64),
        Value::Float(value) => Some(*value),
        Value::BigInt(value) | Value::UInt128(value) => value.to_f64(),
        Value::BigDecimal(value) => value.to_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn sort_list_values(items: &[Value], descending: bool, nulls_last: bool) -> Vec<Value> {
    let null_count = items
        .iter()
        .filter(|item| matches!(item, Value::Null))
        .count();
    let mut sorted = items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .cloned()
        .collect::<Vec<_>>();
    sorted.sort_by(compare_values);
    if descending {
        sorted.reverse();
    }

    let nulls = std::iter::repeat(Value::Null).take(null_count);
    if nulls_last {
        sorted.extend(nulls);
        sorted
    } else {
        nulls.chain(sorted).collect()
    }
}

fn slice_bounds(len: usize, start: &Value, end: &Value) -> (usize, usize) {
    let len_i = len as i64;
    let resolve_start = |value: &Value| -> i64 {
        match value {
            Value::Null => 0,
            _ => match value.as_i64() {
                Some(value) if value < 0 => len_i + value,
                Some(value) => value - 1,
                None => 0,
            },
        }
    };
    let resolve_end = |value: &Value| -> i64 {
        match value {
            Value::Null => len_i,
            _ => match value.as_i64() {
                Some(value) if value < 0 => len_i + value + 1,
                Some(value) => value,
                None => len_i,
            },
        }
    };
    let start = resolve_start(start).clamp(0, len_i) as usize;
    let end = resolve_end(end).clamp(0, len_i) as usize;
    (start.min(end), end)
}

fn list_slice_range(items: &[Value], start: &Value, end: &Value) -> Vec<Value> {
    let (start, end) = slice_bounds(items.len(), start, end);
    items[start..end].to_vec()
}

fn list_element_1_based(items: &[Value], index: i64) -> Value {
    if index == 0 {
        return Value::Null;
    }
    let zero_based = if index < 0 {
        items.len() as i64 + index
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= items.len() as i64 {
        Value::Null
    } else {
        items[zero_based as usize].clone()
    }
}

fn list_element_1_based_expr(items: &[IrExpr], index: i64) -> Option<&IrExpr> {
    if index == 0 {
        return None;
    }
    let zero_based = if index < 0 {
        items.len() as i64 + index
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= items.len() as i64 {
        None
    } else {
        items.get(zero_based as usize)
    }
}

fn string_index_1_based(text: &str, index: i64) -> Value {
    if index == 0 {
        return Value::Null;
    }
    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Value::Null;
    }
    let zero_based = if index < 0 {
        chars.len() as i64 + index
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= chars.len() as i64 {
        Value::Null
    } else {
        Value::String(chars[zero_based as usize].to_string())
    }
}

fn string_slice_range(text: &str, start: &Value, end: &Value) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let (start, end) = slice_bounds(chars.len(), start, end);
    chars[start..end].iter().collect()
}

fn list_distinct_values(items: &[Value], include_null: bool) -> Vec<Value> {
    let mut seen = Vec::new();
    for item in items {
        if !include_null && matches!(item, Value::Null) {
            continue;
        }
        if !seen.iter().any(|seen| list_semantic_eq(seen, item)) {
            seen.push(item.clone());
        }
    }
    seen
}

fn list_semantic_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::List(left), Value::List(right)) | (Value::Path(left), Value::Path(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| list_semantic_eq(left, right))
        }
        (Value::Map(left), Value::Map(right)) => {
            visible_map_keys(left).len() == visible_map_keys(right).len()
                && visible_map_keys(left).into_iter().all(|key| {
                    let Some(value) = left.get(&key) else {
                        return false;
                    };
                    right
                        .get(&key)
                        .is_some_and(|right_value| list_semantic_eq(value, right_value))
                })
        }
        _ => left.three_valued_eq(right) == Some(true),
    }
}

fn constant_cypher_map(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let mut map = BTreeMap::new();
    if args.len() == 2
        && let (IrExpr::List(keys), IrExpr::List(values)) = (&args[0], &args[1])
    {
        if keys.len() != values.len() {
            return Err(RelError::Unsupported(
                "map key/value length mismatch".into(),
            ));
        }
        for (key, value) in keys.iter().zip(values.iter()) {
            let Some(key) = constant_value_expr(key)? else {
                return Ok(None);
            };
            let Some(value) = constant_value_expr(value)? else {
                return Ok(None);
            };
            let key = cypher_plain_value(&key);
            if map.insert(key.clone(), value).is_some() {
                return Err(RelError::Unsupported(format!(
                    "Runtime exception: Found duplicate key: {key} in map."
                )));
            }
        }
        return Ok(Some(Value::Map(map)));
    }
    if args.len() % 2 != 0 {
        return Ok(None);
    }
    for pair in args.chunks(2) {
        let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
            return Ok(None);
        };
        let Some(value) = constant_value_expr(&pair[1])? else {
            return Ok(None);
        };
        map.insert(key.clone(), value);
    }
    Ok(Some(Value::Map(map)))
}

fn integer_literal_text(expr: &IrExpr) -> Option<String> {
    match expr {
        IrExpr::Lit(Lit::String(value)) => Some(value.clone()),
        IrExpr::Lit(Lit::Int(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn literal_i64(expr: &IrExpr) -> Option<i64> {
    match expr {
        IrExpr::Lit(Lit::Int(value)) => Some(*value),
        IrExpr::Call { name, args } if name == "integer_literal" && args.len() == 1 => {
            integer_literal_text(&args[0]).and_then(|value| value.replace('_', "").parse().ok())
        }
        IrExpr::Call { name, args } if is_cast_function(name, args) => {
            args.first().and_then(literal_i64)
        }
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
    assert!(!exprs.is_empty(), "concat_exprs requires an expression");
    // Keep concatenations balanced. Element/path rendering can contain dozens
    // of segments, and a left-deep expression makes DataFusion's recursive
    // unparser consume enough stack to abort an otherwise ordinary query.
    while exprs.len() > 1 {
        let mut next = Vec::with_capacity(exprs.len().div_ceil(2));
        let mut pairs = exprs.into_iter();
        while let Some(left) = pairs.next() {
            next.push(match pairs.next() {
                Some(right) => string_concat(left, right),
                None => left,
            });
        }
        exprs = next;
    }
    exprs.pop().expect("non-empty concatenation")
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

/// Columns produced by a `x.*` projection expansion for `field`, in plan
/// (schema) order.
fn star_expansion_columns(plan: &LogicalPlan, field: &str) -> Option<Vec<Expr>> {
    let prefix = format!("{field}{STAR_SEP}");
    let cols = plan
        .schema()
        .fields()
        .iter()
        .filter(|schema_field| schema_field.name().starts_with(prefix.as_str()))
        .map(|schema_field| col_exact(schema_field.name()).alias(schema_field.name()))
        .collect::<Vec<_>>();
    (!cols.is_empty()).then_some(cols)
}

fn plan_column_type(plan: &LogicalPlan, name: &str) -> Option<DataType> {
    plan.schema()
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .map(|field| field.data_type().clone())
}

/// Relationship-table index for `_ID` printing.
///
/// Kuzu numbers relationship tables in the same namespace as node tables and
/// reserves a second internal slot per relationship type for the reverse
/// adjacency table, so visible ids advance by two per type after the node
/// tables. This mirrors `interpreter::element_id::edge_table_index`; the two
/// must agree or the same edge prints with different ids depending on which
/// path ran it.
fn rel_index_case(label_expr: Expr, graph: &PropertyGraph) -> Expr {
    let base = graph.node_label_order().len() as i64;
    label_index_case(label_expr, graph.edge_rel_order(), base, 2)
}

/// `CASE label WHEN l0 THEN '0' WHEN l1 THEN '1' ... END` — the catalog
/// insertion-order table index used by Kuzu-style `_ID` printing.
///
/// `base` and `stride` place the result in Kuzu's table-id numbering, which
/// node and relationship tables share. See [`rel_index_case`].
fn label_index_case(label_expr: Expr, order: &[String], base: i64, stride: i64) -> Expr {
    let arms = order
        .iter()
        .enumerate()
        .map(|(idx, label)| {
            (
                Box::new(lit(label.clone())),
                Box::new(lit((base + idx as i64 * stride).to_string())),
            )
        })
        .collect::<Vec<_>>();
    if arms.is_empty() {
        return lit(base.to_string());
    }
    Expr::Case(Case::new(
        Some(Box::new(label_expr)),
        arms,
        Some(Box::new(lit("?"))),
    ))
}

/// Gremlin tagged text for a scalar expression, mirroring the
/// interpreter's `tagged_value` (`d[5].i`, `d[2.0].d`, raw strings,
/// `true`/`false`).
fn gremlin_tagged_text_expr(expr: Expr, data_type: &DataType) -> Expr {
    match data_type {
        DataType::Boolean => Expr::Case(Case::new(
            None,
            vec![(Box::new(expr), Box::new(lit("true")))],
            Some(Box::new(lit("false"))),
        )),
        DataType::Int8 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].b")]),
        DataType::Int16 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].s")]),
        DataType::Int32 | DataType::Int64 => {
            concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].i")])
        }
        DataType::Float32 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].f")]),
        DataType::Float64 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].d")]),
        _ => cast_utf8(expr),
    }
}

/// Render one property column as the text the interpreter's
/// `format_property_value` would produce.
fn render_property_text_expr(column: Expr, data_type: &DataType) -> Expr {
    match data_type {
        DataType::Boolean => Expr::Case(Case::new(
            None,
            vec![(Box::new(column), Box::new(lit("True")))],
            Some(Box::new(lit("False"))),
        )),
        DataType::Float64 | DataType::Float32 => cast_utf8(Expr::Cast(Cast::new(
            Box::new(column),
            DataType::Decimal128(38, 6),
        ))),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => column,
        _ => cast_utf8(column),
    }
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

#[derive(Clone, Copy)]
enum DisplayContext {
    Scalar,
    Tagged,
}

fn graph_values_display(value: &Value, language: Language) -> String {
    match language {
        Language::Cypher | Language::Gql => {
            rel_display_value(value, language, DisplayContext::Tagged)
        }
        Language::Gremlin => match value {
            Value::List(_) | Value::Map(_) | Value::Path(_) => format!("{value:?}"),
            _ => rel_display_value(value, language, DisplayContext::Tagged),
        },
        Language::Sparql => rel_display_value(value, language, DisplayContext::Tagged),
    }
}

fn rel_display_value(value: &Value, language: Language, context: DisplayContext) -> String {
    match language {
        Language::Cypher | Language::Gql => match context {
            DisplayContext::Scalar => cypher_plain_value(value),
            DisplayContext::Tagged => tagged_value(value),
        },
        Language::Gremlin | Language::Sparql => tagged_value(value),
    }
}

fn literal_collection_context(language: Language) -> DisplayContext {
    match language {
        Language::Cypher | Language::Gql => DisplayContext::Scalar,
        Language::Gremlin | Language::Sparql => DisplayContext::Tagged,
    }
}

fn constant_result_expr(value: &Value, language: Language, context: DisplayContext) -> Expr {
    match value {
        Value::Null => lit(ScalarValue::Utf8(None)),
        _ => lit(rel_display_value(value, language, context)),
    }
}

fn tagged_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Byte(value) => format!("d[{value}].b"),
        Value::UInt8(value) => format!("d[{value}].u8"),
        Value::Short(value) => format!("d[{value}].s"),
        Value::UInt16(value) => format!("d[{value}].u16"),
        Value::Int(value) => format!("d[{value}].i"),
        Value::UInt32(value) => format!("d[{value}].u32"),
        Value::Long(value) => format!("d[{value}].l"),
        Value::UInt64(value) => format!("d[{value}].u64"),
        Value::Float32(value) => format!("d[{value}].f"),
        Value::Float(value) => format!("d[{value}].d"),
        Value::BigInt(value) => format!("d[{value}].n"),
        Value::UInt128(value) => format!("d[{value}].u128"),
        Value::BigDecimal(value) => format!("d[{value}].m"),
        Value::DateTime(value) => format!("dt[{value}]"),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::String(value) => value.clone(),
        Value::Node { label, id } => format!("v[{label}#{id}]"),
        Value::Edge { rel_type, id, .. } => format!("e[{rel_type}#{id}]"),
        Value::List(items) | Value::Path(items) => {
            let prefix = if matches!(value, Value::Path(_)) {
                "p"
            } else {
                "l"
            };
            let parts = items.iter().map(tagged_value).collect::<Vec<_>>();
            format!("{prefix}[{}]", parts.join(","))
        }
        Value::Map(map) => {
            let parts = map
                .iter()
                .filter(|(key, _)| {
                    !key.starts_with("__")
                        && key.as_str() != STRUCT_ORDER_KEY
                        && key.as_str() != STRUCT_TYPES_KEY
                })
                .map(|(key, value)| {
                    format!(
                        "\"{}\":\"{}\"",
                        escape_debug_string(key),
                        tagged_value(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("m[{{{}}}]", parts.join(","))
        }
    }
}

fn cypher_plain_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Byte(value) => value.to_string(),
        Value::UInt8(value) => value.to_string(),
        Value::Short(value) => value.to_string(),
        Value::UInt16(value) => value.to_string(),
        Value::Int(value) | Value::Long(value) => value.to_string(),
        Value::UInt32(value) => value.to_string(),
        Value::UInt64(value) => value.to_string(),
        Value::Float32(value) => cypher_float_text(*value as f64),
        Value::Float(value) => cypher_float_text(*value),
        Value::BigInt(value) | Value::UInt128(value) => value.to_string(),
        Value::BigDecimal(value) => value.to_string(),
        Value::DateTime(value) | Value::String(value) => value.clone(),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::Node { label, id } => format!("{label}#{id}"),
        Value::Edge { rel_type, id, .. } => format!("{rel_type}#{id}"),
        Value::List(items) => {
            let body = items.iter().map(cypher_plain_value).collect::<Vec<_>>();
            format!("[{}]", body.join(","))
        }
        Value::Path(items) => {
            let body = items.iter().map(cypher_plain_value).collect::<Vec<_>>();
            format!("[{}]", body.join(","))
        }
        Value::Map(map) => {
            if let Some(Value::List(entries)) = map.get("\u{0}kuzu_map_entries") {
                let body = entries
                    .iter()
                    .filter_map(|entry| {
                        let Value::List(pair) = entry else {
                            return None;
                        };
                        let [key, value] = pair.as_slice() else {
                            return None;
                        };
                        Some(format!(
                            "{}={}",
                            cypher_plain_value(key),
                            cypher_plain_value(value)
                        ))
                    })
                    .collect::<Vec<_>>();
                return format!("{{{}}}", body.join(", "));
            }
            // A union is carried as a tagged map but prints as its payload
            // alone: `CAST(127 AS UNION(a STRING, b INT64))` is `127`, not
            // `{b: 127}`. Mirrors `output::union_display_value`.
            if let (Some(Value::String(_)), Some(payload)) = (map.get("__tag"), map.get("__value"))
            {
                return cypher_plain_value(payload);
            }
            // Structs print in declaration order. The map is sorted, so
            // without the recorded order the fields come out alphabetized.
            let ordered_keys: Vec<String> = match map.get(STRUCT_ORDER_KEY) {
                Some(Value::List(items)) => items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(key) if map.contains_key(key) => Some(key.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => map
                    .keys()
                    .filter(|key| {
                        !key.starts_with("__")
                            && key.as_str() != STRUCT_ORDER_KEY
                            && key.as_str() != STRUCT_TYPES_KEY
                    })
                    .cloned()
                    .collect(),
            };
            let body = ordered_keys
                .iter()
                .filter_map(|key| {
                    map.get(key)
                        .map(|value| format!("{key}: {}", cypher_plain_value(value)))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", body.join(", "))
        }
    }
}

fn cypher_float_text(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.6}")
    } else {
        value.to_string()
    }
}

fn escape_debug_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn visible_map_keys(map: &BTreeMap<String, Value>) -> Vec<String> {
    map.keys()
        .filter(|key| {
            key.as_str() != STRUCT_ORDER_KEY
                && key.as_str() != STRUCT_TYPES_KEY
                && !key.starts_with("__")
        })
        .cloned()
        .collect()
}

fn display_for_list_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Byte(value) => value.to_string(),
        Value::UInt8(value) => value.to_string(),
        Value::Short(value) => value.to_string(),
        Value::UInt16(value) => value.to_string(),
        Value::Int(value) | Value::Long(value) => value.to_string(),
        Value::UInt32(value) => value.to_string(),
        Value::UInt64(value) => value.to_string(),
        Value::Float32(value) => (*value as f64).to_string(),
        Value::Float(value) => value.to_string(),
        Value::BigInt(value) | Value::UInt128(value) => value.to_string(),
        Value::BigDecimal(value) => value.to_string(),
        Value::DateTime(value) | Value::String(value) => value.clone(),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::Node { label, id } => format!("{label}#{id}"),
        Value::Edge { rel_type, id, .. } => format!("{rel_type}#{id}"),
        Value::List(items) | Value::Path(items) => {
            let parts = items
                .iter()
                .map(display_for_list_to_string)
                .collect::<Vec<_>>();
            format!("[{}]", parts.join(","))
        }
        Value::Map(map) => {
            let parts = visible_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key)
                        .map(|value| format!("{key}: {}", display_for_list_to_string(value)))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn is_label_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("label") || name.eq_ignore_ascii_case("cypher_label")
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

fn normalize_function_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

fn normalize_temporal_unit(unit: &str) -> String {
    let normalized = unit.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "years" => "year",
        "months" => "month",
        "weeks" => "week",
        "days" => "day",
        "hours" => "hour",
        "minutes" => "minute",
        "seconds" => "second",
        "milliseconds" => "millisecond",
        "microseconds" => "microsecond",
        "nanoseconds" => "nanosecond",
        "quarters" => "quarter",
        "decades" => "decade",
        "centuries" => "century",
        "millennia" | "millenniums" => "millennium",
        _ => normalized.as_str(),
    }
    .to_string()
}

fn expression_has_wide_numeric_cast(expr: &IrExpr) -> bool {
    let IrExpr::Call { name, args } = expr else {
        return false;
    };
    cast_target_text(name, args).is_some_and(|target| {
        matches!(
            target
                .trim()
                .trim_matches('"')
                .to_ascii_uppercase()
                .as_str(),
            "INT128" | "UINT128"
        )
    })
}

fn is_unary_math_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "acos"
            | "acosh"
            | "asin"
            | "asinh"
            | "atan"
            | "atanh"
            | "cbrt"
            | "ceil"
            | "ceiling"
            | "cos"
            | "cosh"
            | "cot"
            | "degrees"
            | "exp"
            | "factorial"
            | "floor"
            | "ln"
            | "log"
            | "log2"
            | "log10"
            | "radians"
            | "round"
            | "sign"
            | "signum"
            | "sin"
            | "sinh"
            | "sqrt"
            | "tan"
            | "tanh"
            | "trunc"
            | "truncate"
    )
}

fn is_binary_math_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "atan2" | "gcd" | "lcm" | "log" | "nanvl" | "round" | "trunc" | "truncate"
    )
}

fn is_date_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("date") || name.eq_ignore_ascii_case("to_date")
}

fn is_date_constructor(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "date" | "to_date" | "timestamp" | "interval" | "duration"
    )
}

fn is_constant_collection_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "array_slice"
            | "array_append"
            | "array_cat"
            | "array_concat"
            | "array_contains"
            | "array_has"
            | "array_indexof"
            | "array_position"
            | "array_prepend"
            | "array_push_back"
            | "array_push_front"
            | "element_at"
            | "list_append"
            | "list_any_value"
            | "list_cat"
            | "list_concat"
            | "list_contains"
            | "list_distinct"
            | "list_element"
            | "list_extract"
            | "list_has_all"
            | "list_has"
            | "list_indexof"
            | "list_join"
            | "list_prepend"
            | "list_position"
            | "list_product"
            | "list_reverse"
            | "list_reverse_sort"
            | "list_slice"
            | "list_sort"
            | "list_sum"
            | "list_to_string"
            | "list_unique"
            | "map_keys"
    )
}

fn is_string_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "char_length"
            | "character_length"
            | "concat"
            | "concat_ws"
            | "contains"
            | "ends_with"
            | "endswith"
            | "gremlin_lcase"
            | "gremlin_substring"
            | "gremlin_ucase"
            | "lcase"
            | "left"
            | "length"
            | "local_length"
            | "local_lcase"
            | "local_ltrim"
            | "local_reverse_strings"
            | "local_rtrim"
            | "local_trim"
            | "local_ucase"
            | "lower"
            | "lpad"
            | "ltrim"
            | "prefix"
            | "regexp_full_match"
            | "regexp_like"
            | "regexp_matches"
            | "regexp_replace"
            | "replace"
            | "reverse"
            | "right"
            | "rpad"
            | "rtrim"
            | "size"
            | "starts_with"
            | "startswith"
            | "strcontains"
            | "substr"
            | "substring"
            | "suffix"
            | "tolower"
            | "toupper"
            | "trim"
            | "ucase"
            | "upper"
    )
}

fn is_core_variadic_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "coalesce" | "constant_or_null" | "greatest" | "ifnull" | "least" | "nullif"
    )
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

fn cast_target_text<'a>(name: &'a str, args: &'a [IrExpr]) -> Option<&'a str> {
    let normalized = name.to_ascii_lowercase();
    if normalized == "cast" {
        let IrExpr::Lit(Lit::String(target)) = args.get(1)? else {
            return None;
        };
        Some(target)
    } else {
        cast_target_from_function_name(&normalized).ok()
    }
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
        "to_int32" | "cast_int" | "gremlin_cast_int" => Ok("INT32"),
        "to_int64" | "to_serial" | "cast_long" => Ok("INT64"),
        "to_int128" => Ok("INT128"),
        "to_uint8" => Ok("UINT8"),
        "to_uint16" => Ok("UINT16"),
        "to_uint32" => Ok("UINT32"),
        "to_uint64" => Ok("UINT64"),
        "to_uint128" => Ok("UINT128"),
        "to_float" | "cast_float" => Ok("FLOAT"),
        "to_double" | "cast_double" => Ok("DOUBLE"),
        "cast_bool" | "cast_boolean" => Ok("BOOL"),
        "cast_bigint" => Ok("DECIMAL(38,0)"),
        "cast_bigdecimal" => Ok("DECIMAL(38,6)"),
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
    if let Some(decimal) = normalized
        .strip_prefix("DECIMAL(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let mut parts = decimal.split(',');
        let precision = parts
            .next()
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or_else(|| {
                RelError::Unsupported(format!("invalid decimal target `{type_name}`"))
            })?;
        let scale = parts
            .next()
            .and_then(|value| value.parse::<i8>().ok())
            .ok_or_else(|| {
                RelError::Unsupported(format!("invalid decimal target `{type_name}`"))
            })?;
        if parts.next().is_some() {
            return Err(RelError::Unsupported(format!(
                "invalid decimal target `{type_name}`"
            )));
        }
        return Ok(DataType::Decimal128(precision, scale));
    }
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
        "DECIMAL" => Ok(DataType::Decimal128(18, 3)),
        "STRING" | "VARCHAR" | "UUID" => Ok(DataType::Utf8),
        "DATE" => Ok(DataType::Date32),
        "TIMESTAMP" | "TIMESTAMP_US" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            None,
        )),
        "TIMESTAMP_NS" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Nanosecond,
            None,
        )),
        "TIMESTAMP_MS" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Millisecond,
            None,
        )),
        "TIMESTAMP_SEC" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Second,
            None,
        )),
        "TIMESTAMP_TZ" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            Some("UTC".into()),
        )),
        // 128-bit integers have no native Arrow representation; a
        // zero-scale decimal covers the numeric range these cases use and
        // prints identically. Values beyond 38 digits fail the cast, which
        // surfaces as an execution error rather than a wrong result.
        "INT128" | "UINT128" => Ok(DataType::Decimal128(38, 0)),
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

fn union_tag_col(binding: &str, property: &str) -> String {
    format!("{}__w_union_tag", prop_col(binding, property))
}

fn struct_field_col(binding: &str, property: &str, field: &str) -> String {
    format!("{}__w_struct__{field}", prop_col(binding, property))
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

fn resolve_column_name(plan: &LogicalPlan, name: &str) -> Option<String> {
    if has_exact_col(plan, name) {
        return Some(name.to_string());
    }
    let mut matches = plan
        .schema()
        .fields()
        .iter()
        .filter(|field| field.name().eq_ignore_ascii_case(name))
        .map(|field| field.name().to_string());
    let column = matches.next()?;
    matches.next().is_none().then_some(column)
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

fn path_len_col(binding: &str) -> String {
    format!("{binding}{PATH_LEN_SUFFIX}")
}

fn is_binding_column(name: &str, binding: &str) -> bool {
    name == binding
        || name == id_col(binding)
        || name == label_col(binding)
        || name == src_id_col(binding)
        || name == src_label_col(binding)
        || name == dst_id_col(binding)
        || name == dst_label_col(binding)
        || name == path_len_col(binding)
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

/// Bindings consumed by the first `GraphCorrelate` leaf under `node`.
fn first_correlate_bindings(node: &Node) -> Option<Vec<String>> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if let Node::GraphCorrelate { bindings, .. } = node {
            return Some(bindings.clone());
        }
        stack.extend(node_children(node));
    }
    None
}

fn node_children(node: &Node) -> Vec<&Node> {
    use Node::*;
    match node {
        GraphMerge {
            input,
            match_arm,
            create_arm,
            ..
        } => vec![input, match_arm, create_arm],
        GraphReturn { input, .. }
        | GraphConstructTriples { input, .. }
        | GraphDescribe { input, .. }
        | GraphAsk { input, .. }
        | GraphBind { input, .. }
        | GraphPathPattern { input, .. }
        | GraphPathFilter { input, .. }
        | GraphCreate { input, .. }
        | GraphSetProperty { input, .. }
        | GraphDelete { input, .. }
        | GraphFilter { input, .. }
        | GraphCurrentProject { input, .. }
        | GraphAggregate { input, .. }
        | GraphGroupMap { input, .. }
        | GraphGroupCountSideEffect { input, .. }
        | GraphCap { input, .. }
        | GraphShortestPath { input, .. }
        | GraphDistinct { input, .. }
        | GraphSort { input, .. }
        | GraphSlice { input, .. }
        | GraphSliceExpr { input, .. }
        | GraphBarrier { input, .. }
        | GraphUnwind { input, .. }
        | GraphQuantifier { input, .. }
        | GraphCollect { input, .. }
        | GraphListComprehension { input, .. }
        | GraphSelect { input, .. }
        | GraphExpand { input, .. }
        | GraphProject { input, .. }
        | GraphService { input, .. } => vec![input],
        GraphJoin { left, right, .. }
        | GraphApply { left, right, .. }
        | GraphUnion { left, right, .. }
        | GraphSparqlMinus { left, right, .. } => vec![left, right],
        GraphRepeat {
            seed,
            body,
            until_traversal,
            prefix_traversal,
            ..
        } => {
            let mut out = vec![seed.as_ref(), body.as_ref()];
            if let Some(node) = until_traversal {
                out.push(node);
            }
            if let Some(node) = prefix_traversal {
                out.push(node);
            }
            out
        }
        GraphCoalesce { input, arms, .. } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter());
            out
        }
        GraphChoose {
            input,
            arms,
            default,
            ..
        } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter().map(|arm| &arm.body));
            if let Some(default) = default {
                out.push(default);
            }
            out
        }
        GraphProcedureCall { input, .. } => input.iter().map(|node| node.as_ref()).collect(),
        GraphExtension { inputs, .. } => inputs.iter().collect(),
        GraphNodeScan { .. }
        | GraphRelScan { .. }
        | GraphValues { .. }
        | GraphOneRow
        | GraphEmpty
        | GraphCorrelate { .. }
        | GraphSparqlTriplePattern { .. }
        | GraphRdfPropertyPath { .. } => Vec::new(),
    }
}

fn unsupported_node_name(node: &Node) -> &'static str {
    match node {
        Node::GraphCorrelate { .. } => "GraphCorrelate",
        Node::GraphSparqlTriplePattern { .. } => "GraphSparqlTriplePattern",
        Node::GraphPathPattern { .. } => "GraphPathPattern",
        Node::GraphRdfPropertyPath { .. } => "GraphRdfPropertyPath",
        Node::GraphRepeat { .. } => "GraphRepeat",
        Node::GraphPathFilter { .. } => "GraphPathFilter",
        Node::GraphCreate { .. } => "GraphCreate",
        Node::GraphMerge { .. } => "GraphMerge",
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
