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
use datafusion::functions_aggregate::count::count_all;
use datafusion::functions_aggregate::expr_fn::{
    avg as df_avg, count as df_count, max as df_max, min as df_min, sum as df_sum,
};
use datafusion::logical_expr::{
    BinaryExpr, Expr, JoinType, LogicalPlan, LogicalPlanBuilder, Operator,
};
use datafusion::prelude::{SessionContext, lit};

use crate::ir::catalog::{CatalogError, EdgeTable, NodeTable, PropertyGraph};
use crate::ir::expr::{AggKind, BinaryOp, IrExpr, Lit, StringOp};
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::plan::{
    ApplyKind, BindKind, Direction, GraphPlan, JoinKind, LabelExpr, Node, NullsOrder, ProjectMode,
    ProjectionItem, Slice, SortDir, TargetMode, UnionAlign,
};
use crate::ir::policy::ResultForm;
use crate::ir::value::Value;

const ID_SUFFIX: &str = "__id";
const LABEL_SUFFIX: &str = "__label";
const PROP_MARKER: &str = "__prop__";
const SRC_ID_SUFFIX: &str = "__src_id";
const SRC_LABEL_SUFFIX: &str = "__src_label";
const DST_ID_SUFFIX: &str = "__dst_id";
const DST_LABEL_SUFFIX: &str = "__dst_label";

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
        let mut ctx = LoweringContext {
            graph,
            options: self.options.clone(),
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
    let output_schema = Arc::new(lowered.plan.schema().as_arrow().clone());
    let ctx = SessionContext::new();
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
                let group_exprs = group
                    .iter()
                    .map(|item| {
                        self.lower_expr(&input.plan, &item.expr)
                            .map(|expr| expr.alias(item.alias.clone()))
                    })
                    .collect::<RelResult<Vec<_>>>()?;
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
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .limit(*offset as usize, fetch.map(|n| n as usize))?
                    .build()?;
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
                left,
                right,
                ..
            } => self.lower_apply(*kind, correlation, left, right)?,
            GraphUnion {
                all,
                align,
                left,
                right,
            } => self.lower_union(*all, *align, left, right)?,
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
            Direction::Both => {
                let out = self.lower_expand_direction(
                    input.clone(),
                    source,
                    target,
                    target_mode,
                    target_labels,
                    rel_binding,
                    rel_types,
                    Direction::Out,
                )?;
                let inn = self.lower_expand_direction(
                    input,
                    source,
                    target,
                    target_mode,
                    target_labels,
                    rel_binding,
                    rel_types,
                    Direction::In,
                )?;
                let plan = LogicalPlanBuilder::from(out.plan.clone())
                    .union_by_name(inn.plan.clone())?
                    .build()?;
                let mut islands = out.islands;
                islands.merge(inn.islands);
                Ok(LoweredNode {
                    plan,
                    islands,
                    fields: out.fields,
                    result_form: out.result_form,
                })
            }
        }
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
        let mut projections = self.project_item_exprs(&input.plan, alias, expr)?;
        if projections.is_empty() {
            return Err(RelError::Unsupported(
                "current projection produced no relational columns".into(),
            ));
        }
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
        _correlation: &[String],
        left: &Node,
        right: &Node,
    ) -> RelResult<LoweredNode> {
        if kind != ApplyKind::Inner {
            return Err(RelError::Unsupported(format!(
                "GraphApply kind `{kind:?}` is not relationally lowered yet"
            )));
        }

        let left = self.lower_node(left)?;
        let previous = self.correlate_plan.replace(left.plan.clone());
        let right = self.lower_node(right);
        self.correlate_plan = previous;
        let mut right = right?;
        right.islands.merge(left.islands);
        Ok(right)
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

    fn return_projection(&self, plan: &LogicalPlan, fields: &[String]) -> RelResult<Vec<Expr>> {
        let mut projections = Vec::new();
        for field in fields {
            if has_exact_col(plan, field) {
                projections.push(col_exact(field));
            } else if has_binding_shape(plan, field).is_some() {
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
                    Err(RelError::Unsupported(format!(
                        "element binding `{binding}` needs scalar context"
                    )))
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
            IrExpr::Call { name, args } if name == "path_or_self" => {
                let Some(fallback) = args.get(1) else {
                    return Err(RelError::Unsupported("path_or_self arity".into()));
                };
                self.lower_expr(plan, fallback)
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
