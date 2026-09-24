//! Executable relational lowering for Graph IR.
//!
//! This module decomposes supported Graph IR regions into ordinary
//! DataFusion logical plans. It intentionally sits beside `ir::df`: that
//! module preserves graph operators as DataFusion extension nodes for rules
//! and round-tripping, while this module lowers graph-shaped operators into
//! base relational scans, joins, projections, filters, and aggregates that
//! DataFusion can execute directly.

mod scans;
use scans::*;

mod expression;
use expression::*;

mod constants;
use constants::*;

mod collection_expr;
use collection_expr::*;

mod joins;
use joins::*;

mod columns;
pub(crate) use columns::*;

mod aggregates;
mod operators;
use aggregates::*;
mod branches;
mod collection_operators;
mod expansion;
mod plan_walk;
mod projection;
use plan_walk::*;

mod apply;
mod casts;
mod collections;
mod gremlin;
mod gremlin_state;
pub mod mapping;
pub mod rdf;
mod repeat;
mod sparql;
pub mod sql;
mod varlen;

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Array, Int64Builder, ListBuilder, RecordBatch,
    StringArray, StringBuilder,
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
use num_traits::{FromPrimitive, ToPrimitive};

use crate::ir::analysis::{ReadCapabilities, ReadValidationError, validate_read_capabilities};
use crate::ir::catalog::{CatalogError, EdgeTable, NodeTable, PropertyGraph};
use crate::ir::expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit, StringOp};
use crate::ir::interpreter::{
    ReturnedBatches, Row as InterpreterRow, compare_values, eval as interpreter_eval,
};
use crate::ir::plan::{
    ApplyKind, BindKind, ChooseArm, ChooseSelector, ChooseUnmatched, CoalesceSuccess, Direction,
    GraphPlan, JoinKind, LabelExpr, Node, NullsOrder, ProjectMode, ProjectionItem, QuantifierKind,
    Slice, SortDir, SortKey, TargetMode, UnionAlign,
};
use crate::ir::policy::{GraphPlanPolicy, Language, ResultForm};
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
const PATH_INNER_SUFFIX: &str = "__pathinner";
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
    /// Read-only RDF quad sources keyed by SPARQL dataset name.
    pub rdf_datasets: Option<Arc<rdf::RdfDatasetMapping>>,
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
            rdf_datasets: None,
            varlen_recursive_ceiling: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RelError {
    #[error("read plan validation: {0}")]
    ReadValidation(#[from] ReadValidationError),
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
    policy: GraphPlanPolicy,
    language: Language,
    scan_counter: usize,
    correlate_plan: Option<LogicalPlan>,
    rdf_typed_terms_used: bool,
    /// Gremlin step-label bind multiplicity (see `gremlin::label_bind_counts`).
    gremlin_label_binds: BTreeMap<String, usize>,
}

impl RelBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: RelBackendOptions) -> Self {
        Self { options }
    }

    pub fn lower(&self, plan: &GraphPlan, graph: &PropertyGraph) -> RelResult<LoweredPlan> {
        validate_read_capabilities(plan, ReadCapabilities::LOCAL_DUCKDB)?;
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
            policy: plan.policy.clone(),
            language: plan.policy.language,
            scan_counter: 0,
            correlate_plan: None,
            rdf_typed_terms_used: false,
            gremlin_label_binds: if plan.policy.language == Language::Gremlin {
                gremlin::label_bind_counts(&plan.root)
            } else {
                BTreeMap::new()
            },
        };
        if sparql::handles(plan) {
            return sparql::lower_plan(&mut ctx, plan);
        }
        let lowered = ctx.lower_node(&plan.root)?;
        let fields = lowered
            .fields
            .clone()
            .unwrap_or_else(|| output_fields(&lowered.plan));
        Ok(LoweredPlan {
            plan: lowered.plan,
            fields,
            result_form: lowered.result_form.unwrap_or(ctx.policy.result_form),
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

/// Names with language-owned lowering must not be silently shadowed by UDF
/// aliases. Keep classification aligned with the expression dispatch.
pub(crate) fn is_language_function(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    crate::ir::interpreter::is_known_function(&name)
        || expression::is_label_function(&name)
        || expression::is_id_function(&name)
        || expression::is_mod_function(&name)
        || expression::is_abs_function(&name)
        || expression::is_pow_function(&name)
        || expression::is_unary_math_function(&name)
        || expression::is_binary_math_function(&name)
        || expression::is_date_function(&name)
        || expression::is_date_constructor(&name)
        || expression::is_constant_collection_function(&name)
        || expression::is_string_function(&name)
        || expression::is_core_variadic_function(&name)
        || expression::is_exists_function(&name)
        || expression::is_in_function(&name)
        || expression::cast_target_from_function_name(&name).is_ok()
        || name.starts_with("cypher_")
        || name.starts_with("gremlin_")
        || name.starts_with("sparql_")
        || name.starts_with("__")
        || matches!(
            name.as_str(),
            "cast" | "date_part" | "date_trunc" | "map" | "make_map" | "list_slice" | "range"
        )
}
