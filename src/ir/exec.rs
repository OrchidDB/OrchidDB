//! Hybrid executable plans: SQL islands spliced into a Graph IR plan.
//!
//! The relational backend ([`crate::ir::rel`]) is all-or-nothing — it lowers
//! a whole [`GraphPlan`] to one relational plan, or fails. That makes a
//! single unsupported operator very expensive: `list_append` sitting in a
//! projection discards the scan, filter and joins beneath it, and the entire
//! query falls back to the tree interpreter.
//!
//! This module partitions instead. It walks the plan top-down looking for
//! *maximal* subtrees the relational backend accepts, executes each one as a
//! SQL island, and rewrites that subtree in place as [`Node::GraphValues`]
//! holding the island's result rows. Whatever could not be lowered runs
//! afterwards over those materialized rows.
//!
//! Splicing the results back in as `GraphValues` is what keeps this cheap:
//! the residual is still an ordinary Graph IR plan, so it needs no new
//! operator, no new executor, and no changes to any existing consumer of
//! `Node`.
//!
//! Two things are deliberately never islanded:
//!
//! * **Mutations.** `GraphCreate` has a relational lowering, but executing it
//!   through DataFusion would compute a result set without ever writing to
//!   the catalog. Writes must reach the graph store, so any subtree
//!   containing one stays with the interpreter.
//! * **Correlated subtrees.** A subtree under an `Apply`'s right side refers
//!   to bindings produced outside it. Lowering it standalone fails on the
//!   unresolved binding, so it declines to island on its own.

use std::future::Future;
use std::pin::Pin;

use arrow::array::Array;
use arrow::datatypes::{DataType, Field};

use crate::ir::catalog::{PropertyGraph, array_value};
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::plan::{GraphPlan, Node};
use crate::ir::rel::sql;
use crate::ir::rel::{LoweredPlan, RelBackend, execute_lowered};
use crate::ir::value::Value;

/// A future returned by [`IslandTarget::execute`]. Boxed because the trait is
/// object-safe by design: the target is chosen at runtime (config, env var,
/// or per-call), not at compile time.
pub type IslandFuture<'a> = Pin<Box<dyn Future<Output = Result<ReturnedBatches, String>> + 'a>>;

/// Where a SQL island actually runs.
///
/// The partitioner decides *what* to push down; this decides *who executes
/// it*. Keeping them apart is what makes the engine retargetable: DuckDB is
/// the default, but nothing above this trait knows that.
pub trait IslandTarget {
    /// Short name for diagnostics and harness reporting (`duckdb`, ...).
    fn name(&self) -> &str;
    fn execute<'a>(&'a self, lowered: LoweredPlan) -> IslandFuture<'a>;
}

/// In-process DataFusion. No external engine, no SQL text — useful as a
/// reference target and for environments without a database.
#[derive(Debug, Clone, Copy, Default)]
pub struct DataFusionTarget;

impl IslandTarget for DataFusionTarget {
    fn name(&self) -> &str {
        "datafusion"
    }

    fn execute<'a>(&'a self, lowered: LoweredPlan) -> IslandFuture<'a> {
        Box::pin(async move {
            execute_lowered(lowered)
                .await
                .map_err(|err| format!("{err}"))
        })
    }
}

/// A real SQL engine: the lowered plan is unparsed to dialect-specific SQL,
/// the referenced tables are materialized, and the query runs on the engine.
///
/// Generic over the executor so any [`SqlExecutor`](crate::ir::rel::sql::SqlExecutor)
/// implementation — DuckDB, Postgres, or one added later — can back an island
/// without touching the partitioner.
pub struct SqlTarget<F> {
    dialect: sql::SqlDialect,
    /// Executors take `&mut self` and some (Postgres) own a connection, so
    /// the target builds a fresh one per island rather than holding shared
    /// mutable state. That keeps island futures `Send` and keeps targets
    /// usable from any runtime.
    make_executor: F,
}

impl<F, E> SqlTarget<F>
where
    F: Fn() -> sql::SqlResult<E>,
    E: sql::SqlExecutor,
{
    pub fn new(dialect: sql::SqlDialect, make_executor: F) -> Self {
        Self {
            dialect,
            make_executor,
        }
    }
}

impl SqlTarget<fn() -> sql::SqlResult<sql::DuckDbExecutor>> {
    /// The default target: an in-process DuckDB database per island.
    #[cfg(feature = "duckdb")]
    pub fn duckdb() -> Self {
        Self::new(sql::SqlDialect::DuckDb, || Ok(sql::DuckDbExecutor::new()))
    }
}

#[cfg(feature = "postgres")]
impl SqlTarget<fn() -> sql::SqlResult<sql::PostgresExecutor>> {
    /// Postgres, connecting per island through
    /// [`PostgresExecutor::ENV_URL`](sql::PostgresExecutor::ENV_URL).
    pub fn postgres() -> Self {
        Self::new(sql::SqlDialect::Postgres, || {
            let url = std::env::var(sql::PostgresExecutor::ENV_URL).map_err(|_| {
                sql::SqlError::Setup(format!(
                    "{} is not set",
                    sql::PostgresExecutor::ENV_URL
                ))
            })?;
            sql::PostgresExecutor::connect(&url)
        })
    }
}

impl<F, E> IslandTarget for SqlTarget<F>
where
    F: Fn() -> sql::SqlResult<E>,
    E: sql::SqlExecutor,
{
    fn name(&self) -> &str {
        self.dialect.name()
    }

    fn execute<'a>(&'a self, lowered: LoweredPlan) -> IslandFuture<'a> {
        Box::pin(async move {
            // SQL engines run synchronously and cannot be cancelled once
            // started, so oversized plans are refused up front rather than
            // allowed to pin a core. DataFusion applies the same bound in
            // `execute_lowered`.
            let nodes = logical_plan_nodes(&lowered.plan);
            if nodes > MAX_ISLAND_PLAN_NODES {
                return Err(format!(
                    "island plan too large for uncancellable execution ({nodes} nodes)"
                ));
            }
            let prepared = sql::prepare(&lowered, self.dialect)
                .await
                .map_err(|err| format!("{err}"))?;
            let mut executor = (self.make_executor)().map_err(|err| format!("{err}"))?;
            sql::execute_prepared(&mut executor, &prepared).map_err(|err| format!("{err}"))
        })
    }
}

/// Upper bound on the relational plan size handed to a synchronous engine.
const MAX_ISLAND_PLAN_NODES: usize = 200;

fn logical_plan_nodes(plan: &datafusion::logical_expr::LogicalPlan) -> usize {
    let mut count = 0usize;
    let mut stack = vec![plan];
    while let Some(node) = stack.pop() {
        count += 1;
        stack.extend(node.inputs());
    }
    count
}

/// The default island target: DuckDB when compiled in, DataFusion otherwise.
///
/// `GRAPH_ISLAND_TARGET` overrides it (`duckdb`, `postgres`, `datafusion`) so
/// the same binary can be pointed at a different engine.
pub fn default_target() -> Box<dyn IslandTarget> {
    let requested = std::env::var("GRAPH_ISLAND_TARGET").unwrap_or_default();
    match requested.as_str() {
        "datafusion" => Box::new(DataFusionTarget),
        #[cfg(feature = "postgres")]
        "postgres" => Box::new(SqlTarget::postgres()),
        #[cfg(feature = "duckdb")]
        _ => Box::new(SqlTarget::duckdb()),
        #[cfg(not(feature = "duckdb"))]
        _ => Box::new(DataFusionTarget),
    }
}

/// What the partitioner did, for tests and for the harness to report.
#[derive(Debug, Clone, Default)]
pub struct ExecStats {
    /// Subtrees executed relationally.
    pub islands: usize,
    /// Rows those islands returned in total.
    pub island_rows: usize,
    /// Every operator left in the rewritten plan, islands included.
    pub residual_ops: usize,
    /// Operators the interpreter must still evaluate — everything in the
    /// residual except the spliced-in island results and the `GraphReturn`
    /// result-shaping boundary, neither of which does query work.
    pub interpreted_ops: usize,
    /// Why subtrees declined to island, outermost attempt first. This is the
    /// work list for closing lowering gaps — every entry is a query shape
    /// that still needs the interpreter.
    pub declined: Vec<String>,
}

impl ExecStats {
    /// Did the whole plan push down, leaving the interpreter nothing to do
    /// but shape already-computed rows?
    ///
    /// This is the metric that gates retiring the interpreter: it can be
    /// deleted once every case in the corpus reports `true`.
    pub fn fully_pushed_down(&self) -> bool {
        self.islands >= 1 && self.interpreted_ops == 0
    }
}

/// Column-name suffixes the relational lowering uses to encode a graph
/// binding across several flat columns. Kept in sync with `ir::rel`.
const ID_SUFFIX: &str = "__id";
const LABEL_SUFFIX: &str = "__label";
const PROP_MARKER: &str = "__prop__";
const SRC_ID_SUFFIX: &str = "__src_id";
const SRC_LABEL_SUFFIX: &str = "__src_label";
const DST_ID_SUFFIX: &str = "__dst_id";
const DST_LABEL_SUFFIX: &str = "__dst_label";
/// Separator between a `x.*` projection alias and each expanded property.
const STAR_SEP: &str = "__star__";

/// Rewrite `plan` so every maximal relationally-lowerable subtree is replaced
/// by its already-computed rows.
///
/// The returned plan is an ordinary Graph IR plan and can be handed straight
/// to the interpreter. On any island failure the subtree is left untouched,
/// so the worst case is exactly today's behaviour.
pub async fn plan_with_islands(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    backend: &RelBackend,
    target: &dyn IslandTarget,
) -> (GraphPlan, ExecStats) {
    let mut root = (*plan.root).clone();
    let mut stats = ExecStats::default();
    islandize(&mut root, &plan.policy, graph, backend, target, &mut stats).await;
    stats.residual_ops = count_ops(&root);
    stats.interpreted_ops = count_interpreted_ops(&root);
    (
        GraphPlan {
            policy: plan.policy.clone(),
            root: Box::new(root),
        },
        stats,
    )
}

/// Try to island `node`; if it declines, recurse into its children.
///
/// Boxed because the recursion is async — `islandize` awaits itself.
fn islandize<'a>(
    node: &'a mut Node,
    policy: &'a crate::ir::policy::GraphPlanPolicy,
    graph: &'a PropertyGraph,
    backend: &'a RelBackend,
    target: &'a dyn IslandTarget,
    stats: &'a mut ExecStats,
) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    Box::pin(async move {
        match try_island(node, policy, graph, backend, target).await {
            Ok(values) => {
                if let Node::GraphValues { rows, .. } = &values {
                    stats.islands += 1;
                    stats.island_rows += rows.len();
                }
                *node = values;
                return;
            }
            Err(Decline::Ineligible) => {}
            Err(Decline::Reason(reason)) => stats.declined.push(reason),
        }
        for child in children_mut(node) {
            islandize(child, policy, graph, backend, target, stats).await;
        }
    })
}

/// Why a subtree did not become an island.
enum Decline {
    /// Structurally not a candidate (leaf, mutation, result boundary). Not a
    /// gap in the lowering, so it is not worth reporting.
    Ineligible,
    /// The relational backend or the target engine rejected it. These are the
    /// gaps worth closing.
    Reason(String),
}

/// Execute `node` on the target engine and materialize the result.
async fn try_island(
    node: &Node,
    policy: &crate::ir::policy::GraphPlanPolicy,
    graph: &PropertyGraph,
    backend: &RelBackend,
    target: &dyn IslandTarget,
) -> Result<Node, Decline> {
    // A bare source is already as cheap as it gets; islanding one buys
    // nothing and only costs a materialization.
    if children_count(node) == 0 {
        return Err(Decline::Ineligible);
    }
    if contains_mutation(node) {
        return Err(Decline::Ineligible);
    }
    // `GraphReturn` is result shaping, not query work: it names the output
    // columns and applies the result form. Islanding it would hand the
    // engine's column order to the caller instead of the declared field
    // order, so it stays put and its input islands instead.
    if matches!(node, Node::GraphReturn { .. }) {
        return Err(Decline::Ineligible);
    }
    let candidate = GraphPlan {
        policy: policy.clone(),
        root: Box::new(node.clone()),
    };
    let lowered = backend
        .lower(&candidate, graph)
        .map_err(|err| Decline::Reason(format!("{err}")))?;
    let returned = target
        .execute(lowered)
        .await
        .map_err(|err| Decline::Reason(format!("{}: {err}", target.name())))?;
    batch_to_values(&returned).ok_or_else(|| {
        Decline::Reason("island result did not match the graph column encoding".to_string())
    })
}

/// Rebuild a relational result batch into `GraphValues` bindings.
///
/// The relational encoding spreads one graph binding over several columns
/// (`p__id`, `p__label`, `p__prop__age`). Node and edge bindings are
/// reassembled from their id/label columns; property columns are dropped
/// because the catalog remains the source of truth for property reads, so
/// carrying them would only risk disagreeing with it.
fn batch_to_values(returned: &ReturnedBatches) -> Option<Node> {
    let batch = &returned.batch;
    let schema = batch.schema();
    let names: Vec<String> = schema
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect();



    enum Source {
        /// `(id column, label column)`
        NodeCols(usize, usize),
        /// `(src id, src label, dst id, dst label, edge id, edge label)`
        EdgeCols(usize, usize, usize, usize, Option<usize>, Option<usize>),
        /// A `x.*` projection: `(property name, column)` in projection order.
        StarCols(Vec<(String, usize)>),
        Scalar(usize),
    }

    let find = |suffix: &str, binding: &str| -> Option<usize> {
        names.iter().position(|n| n == &format!("{binding}{suffix}"))
    };

    let mut bindings: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    // A `x.*` projection fans one field out into one column per property,
    // but the residual still refers to the single field `x.*`. Direct
    // evaluation represents that field as one binding holding a map of
    // property to value, which `finalize_return` expands into columns — so
    // collapse the columns back into exactly that shape.
    let mut star_groups: Vec<(String, Vec<(String, usize)>)> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let Some((field, key)) = name.split_once(STAR_SEP) else {
            continue;
        };
        match star_groups.iter_mut().find(|(existing, _)| existing == field) {
            Some((_, columns)) => columns.push((key.to_string(), index)),
            None => star_groups.push((field.to_string(), vec![(key.to_string(), index)])),
        }
    }
    for (field, columns) in star_groups {
        bindings.push(field);
        sources.push(Source::StarCols(columns));
    }

    for (index, name) in names.iter().enumerate() {
        if name.contains(PROP_MARKER) || name.contains(STAR_SEP) {
            continue;
        }
        // Derive the binding name from whichever structural suffix matched.
        let binding = [
            SRC_ID_SUFFIX,
            SRC_LABEL_SUFFIX,
            DST_ID_SUFFIX,
            DST_LABEL_SUFFIX,
            ID_SUFFIX,
            LABEL_SUFFIX,
        ]
        .iter()
        .find_map(|suffix| name.strip_suffix(*suffix))
        .map(str::to_string);

        match binding {
            Some(binding) => {
                if seen.contains(&binding) {
                    continue;
                }
                seen.push(binding.clone());
                let src_id = find(SRC_ID_SUFFIX, &binding);
                let dst_id = find(DST_ID_SUFFIX, &binding);
                if let (Some(src_id), Some(dst_id)) = (src_id, dst_id) {
                    let src_label = find(SRC_LABEL_SUFFIX, &binding)?;
                    let dst_label = find(DST_LABEL_SUFFIX, &binding)?;
                    bindings.push(binding.clone());
                    sources.push(Source::EdgeCols(
                        src_id,
                        src_label,
                        dst_id,
                        dst_label,
                        find(ID_SUFFIX, &binding),
                        find(LABEL_SUFFIX, &binding),
                    ));
                } else {
                    let id = find(ID_SUFFIX, &binding)?;
                    let label = find(LABEL_SUFFIX, &binding)?;
                    bindings.push(binding.clone());
                    sources.push(Source::NodeCols(id, label));
                }
            }
            None => {
                bindings.push(name.clone());
                sources.push(Source::Scalar(index));
            }
        }
    }

    let column_value = |index: usize, row: usize| -> Option<Value> {
        decode_value(
            batch.column(index).as_ref(),
            row,
            Some(schema.field(index)),
        )
    };
    let as_i64 = |value: &Value| -> Option<i64> {
        match value {
            Value::Int(v) | Value::Long(v) => Some(*v),
            _ => None,
        }
    };
    let as_label = |value: &Value| -> Option<String> {
        match value {
            Value::String(v) => Some(v.clone()),
            _ => None,
        }
    };

    let mut rows = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let mut out = Vec::with_capacity(sources.len());
        for source in &sources {
            let value = match source {
                Source::Scalar(index) => column_value(*index, row)?,
                Source::StarCols(columns) => {
                    let mut map = std::collections::BTreeMap::new();
                    // The map is sorted, so the projection order has to be
                    // recorded separately or the columns come out
                    // alphabetized instead of as written.
                    let mut order = Vec::with_capacity(columns.len());
                    for (key, index) in columns {
                        map.insert(key.clone(), column_value(*index, row)?);
                        order.push(Value::String(key.clone()));
                    }
                    map.insert(
                        crate::ir::value::STRUCT_ORDER_KEY.to_string(),
                        Value::List(order),
                    );
                    Value::Map(map)
                }
                Source::NodeCols(id, label) => {
                    let id_value = column_value(*id, row)?;
                    let label_value = column_value(*label, row)?;
                    match (as_i64(&id_value), as_label(&label_value)) {
                        (Some(id), Some(label)) => Value::Node { label, id },
                        // A null id is an outer-join miss, not a broken
                        // encoding; anything else means the island did not
                        // produce the shape we expect, so decline the island
                        // rather than fabricate a binding.
                        _ if matches!(id_value, Value::Null) => Value::Null,
                        _ => return None,
                    }
                }
                Source::EdgeCols(src_id, src_label, dst_id, dst_label, id, label) => {
                    let src_id_value = column_value(*src_id, row)?;
                    if matches!(src_id_value, Value::Null) {
                        Value::Null
                    } else {
                        Value::Edge {
                            rel_type: label
                                .and_then(|index| column_value(index, row))
                                .as_ref()
                                .and_then(as_label)
                                .unwrap_or_default(),
                            id: id
                                .and_then(|index| column_value(index, row))
                                .as_ref()
                                .and_then(as_i64)
                                .unwrap_or_default(),
                            src_label: as_label(&column_value(*src_label, row)?)?,
                            src_id: as_i64(&src_id_value)?,
                            dst_label: as_label(&column_value(*dst_label, row)?)?,
                            dst_id: as_i64(&column_value(*dst_id, row)?)?,
                            projected_properties: None,
                        }
                    }
                }
            };
            out.push(value);
        }
        rows.push(out);
    }

    Some(Node::GraphValues {
        bindings,
        rows,
        bulk: None,
    })
}

/// Decode one Arrow cell into a [`Value`], or `None` if the type has no
/// faithful representation here.
///
/// `None` is load-bearing: it makes the whole island decline, so the subtree
/// falls back to direct evaluation. The alternative — substituting `Null` for
/// anything unrecognized, which is what [`array_value`] does by design for
/// property reads — turns an unsupported type into a silently wrong answer.
/// That is exactly how `collect()` results were being dropped.
fn decode_value(array: &dyn Array, row: usize, field: Option<&Field>) -> Option<Value> {
    if row >= array.len() || array.is_null(row) {
        return Some(Value::Null);
    }
    match array.data_type() {
        DataType::Null => Some(Value::Null),
        DataType::Boolean
        | DataType::Int32
        | DataType::Int64
        | DataType::Float64
        | DataType::Utf8 => Some(array_value(array, row, field)),
        // Narrower and unsigned widths carry the same values; widen rather
        // than decline, since the graph value model has one integer type.
        DataType::Int8 => widen_int::<arrow::array::Int8Array>(array, row),
        DataType::Int16 => widen_int::<arrow::array::Int16Array>(array, row),
        DataType::UInt8 => widen_int::<arrow::array::UInt8Array>(array, row),
        DataType::UInt16 => widen_int::<arrow::array::UInt16Array>(array, row),
        DataType::UInt32 => widen_int::<arrow::array::UInt32Array>(array, row),
        DataType::UInt64 => array
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .and_then(|typed| i64::try_from(typed.value(row)).ok())
            .map(Value::Long),
        DataType::Float32 => array
            .as_any()
            .downcast_ref::<arrow::array::Float32Array>()
            .map(|typed| Value::Float(f64::from(typed.value(row)))),
        DataType::LargeUtf8 => array
            .as_any()
            .downcast_ref::<arrow::array::LargeStringArray>()
            .map(|typed| Value::String(typed.value(row).to_string())),
        DataType::Utf8View => array
            .as_any()
            .downcast_ref::<arrow::array::StringViewArray>()
            .map(|typed| Value::String(typed.value(row).to_string())),
        // `collect()` and friends produce real Arrow lists; decode them
        // elementwise so nested lists work too.
        DataType::List(inner) => {
            let typed = array.as_any().downcast_ref::<arrow::array::ListArray>()?;
            decode_list(typed.value(row).as_ref(), inner)
        }
        DataType::LargeList(inner) => {
            let typed = array
                .as_any()
                .downcast_ref::<arrow::array::LargeListArray>()?;
            decode_list(typed.value(row).as_ref(), inner)
        }
        _ => None,
    }
}

fn widen_int<T>(array: &dyn Array, row: usize) -> Option<Value>
where
    T: arrow::array::Array + 'static,
    for<'a> &'a T: arrow::array::ArrayAccessor,
    for<'a> <&'a T as arrow::array::ArrayAccessor>::Item: Into<i64>,
{
    let typed = array.as_any().downcast_ref::<T>()?;
    Some(Value::Long(
        arrow::array::ArrayAccessor::value(&typed, row).into(),
    ))
}

fn decode_list(items: &dyn Array, inner: &Field) -> Option<Value> {
    let mut out = Vec::with_capacity(items.len());
    for index in 0..items.len() {
        out.push(decode_value(items, index, Some(inner))?);
    }
    Some(Value::List(out))
}

/// Does this subtree write to the graph?
fn contains_mutation(node: &Node) -> bool {
    if matches!(
        node,
        Node::GraphCreate { .. }
            | Node::GraphMerge { .. }
            | Node::GraphSetProperty { .. }
            | Node::GraphDelete { .. }
    ) {
        return true;
    }
    children(node).into_iter().any(contains_mutation)
}

fn count_ops(node: &Node) -> usize {
    1 + children(node).into_iter().map(count_ops).sum::<usize>()
}

/// Operators that represent real work left for the interpreter. Spliced
/// island results and the `GraphReturn` boundary are excluded: the first is
/// already computed, the second only names and shapes the output.
fn count_interpreted_ops(node: &Node) -> usize {
    let self_cost = usize::from(!matches!(
        node,
        Node::GraphValues { .. } | Node::GraphReturn { .. }
    ));
    self_cost
        + children(node)
            .into_iter()
            .map(count_interpreted_ops)
            .sum::<usize>()
}

fn children_count(node: &Node) -> usize {
    children(node).len()
}

fn children(node: &Node) -> Vec<&Node> {
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
            out.extend(until_traversal.iter().map(|node| node.as_ref()));
            out.extend(prefix_traversal.iter().map(|node| node.as_ref()));
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
            out.extend(default.iter().map(|node| node.as_ref()));
            out
        }
        GraphProcedureCall { input, .. } => input.iter().map(|node| node.as_ref()).collect(),
        GraphExtension { inputs, .. } => inputs.iter().collect(),
        _ => Vec::new(),
    }
}

fn children_mut(node: &mut Node) -> Vec<&mut Node> {
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
            let mut out = vec![seed.as_mut(), body.as_mut()];
            out.extend(until_traversal.iter_mut().map(|node| node.as_mut()));
            out.extend(prefix_traversal.iter_mut().map(|node| node.as_mut()));
            out
        }
        GraphCoalesce { input, arms, .. } => {
            let mut out = vec![input.as_mut()];
            out.extend(arms.iter_mut());
            out
        }
        GraphChoose {
            input,
            arms,
            default,
            ..
        } => {
            let mut out = vec![input.as_mut()];
            out.extend(arms.iter_mut().map(|arm| &mut arm.body));
            out.extend(default.iter_mut().map(|node| node.as_mut()));
            out
        }
        GraphProcedureCall { input, .. } => input.iter_mut().map(|node| node.as_mut()).collect(),
        GraphExtension { inputs, .. } => inputs.iter_mut().collect(),
        _ => Vec::new(),
    }
}
