//! Compiled language boundaries scheduled by Orchid between SQL engines. Graph IR is consumed
//! during compilation only; physical operators contain scalar metadata and
//! Arrow inputs, never an executable Graph IR subtree.
use crate::ir::diagnostics::QueryExecutionError;
use super::{LoweredPlan, RelBackend};
use crate::ir::{
    catalog::PropertyGraph,
    runtime::{RuntimeError, IrResult, ReturnedBatches, Row, eval, context::ExecutionContext},
    jvm::JvmExecution,
    plan::{GraphPlan, Node},
    value::Value,
};
use arrow::array::RecordBatch;
use datafusion::{
    common::{DFSchemaRef, DataFusionError, Result, tree_node::{TreeNode, Transformed}},
    logical_expr::{Expr, Extension, LogicalPlan, UserDefinedLogicalNodeCore},
};
use futures::stream;
use super::engine_exec::{ExecutionPlan, TaskContext, PlanProperties, SendableRecordBatchStream, RecordBatchStreamAdapter};
use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

pub mod host;
pub mod policy;
pub mod program;
pub use host::{CompileOptions, KernelState, SubplanRunner, compile_for_host};
pub type CompiledKernel = RowKernel;
type State = KernelState;
mod legacy;
mod optimize;

type Kernel = dyn Fn(Vec<Vec<Row>>, &mut State) -> IrResult<Vec<Row>> + Send + Sync;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
fn failure(e: impl ToString) -> DataFusionError {
    DataFusionError::Execution(e.to_string())
}
mod transport;
mod execution;
mod reentry;
mod references;
pub(crate) use reentry::live_graph_scan;
use execution::{Source, source_kernel};
use transport::{schema, decode_rows};
#[cfg(test)]
use transport::encode_rows;

#[derive(Clone)]
pub struct RowKernel {
    boundary: Option<policy::BoundaryKind>,
    id: u64,
    name: String,
    inputs: Vec<LogicalPlan>,
    schema: DFSchemaRef,
    kernel: Arc<Kernel>,
    relational_input: Option<Vec<String>>,
    relational_input_state: bool,
    relational_output: Option<reentry::SqlOutput>,
    /// Adjacent unary kernels may retain rows in memory without reordering evaluation.
    fuse_unary: bool,
    contract: Option<optimize::Contract>,
    /// The operator can consume input incrementally while preserving order.
    streaming: bool,
    source: Option<Source>,
    slice: Option<crate::ir::plan::Slice>,
    /// Flat fused stages; execution never recurses through closure wrappers.
    prefix: Vec<Arc<Kernel>>,
}
impl fmt::Debug for RowKernel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&self.name).field("id", &self.id).finish()
    }
}
impl PartialEq for RowKernel {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.inputs == o.inputs
    }
}
impl Eq for RowKernel {}
impl Hash for RowKernel {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.id.hash(s);
        self.inputs.hash(s);
    }
}
impl PartialOrd for RowKernel {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for RowKernel {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.id.cmp(&o.id)
    }
}
impl UserDefinedLogicalNodeCore for RowKernel {
    fn name(&self) -> &str {
        &self.name
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        self.inputs.iter().collect()
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
    fn with_exprs_and_inputs(&self, exprs: Vec<Expr>, inputs: Vec<LogicalPlan>) -> Result<Self> {
        if !exprs.is_empty() || inputs.len() != self.inputs.len() {
            return Err(failure("Invalid kernel rewrite"));
        }
        Ok(Self {
            inputs,
            ..self.clone()
        })
    }
    fn prevent_predicate_push_down_columns(&self) -> std::collections::HashSet<String> {
        self.schema
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect()
    }
}
fn kernel(
    name: &str,
    inputs: Vec<LogicalPlan>,
    operation: impl Fn(Vec<Vec<Row>>, &mut State) -> IrResult<Vec<Row>> + Send + Sync + 'static,
) -> LogicalPlan {
    LogicalPlan::Extension(Extension {
        node: Arc::new(RowKernel {
            boundary: if name == "DecodeTraversers" { Some(policy::BoundaryKind::ResultTransport) } else { None },
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            name: name.into(),
            inputs,
            schema: Arc::new(
                schema()
                    .as_ref()
                    .clone()
                    .try_into()
                    .expect("unique transport fields"),
            ),
            kernel: Arc::new(operation),
            relational_input: None,
            relational_input_state: false,
            relational_output: None,
            contract: None,
            streaming: name == "DecodeTraversers",
            source: None,
            slice: None,
            prefix: vec![],
            fuse_unary: matches!(name,
                "Bind" | "Filter" | "Project" | "CurrentProject" | "Return" | "RetainBindings"
                | "Expand" | "PathFilter" | "CorrelatedInput"
                | "Unwind" | "Quantifier" | "ListComprehension"
                | "NodeScan" | "RelScan" | "Values" | "OneRow" | "Empty"
            ),
        }),
    })
}
/// Fuse batch-local reads and chunked sources into a flat pipeline. Fusion
/// never crosses SQL, JVM, write or whole-input operators.
/// Every original operator keeps its cancellation and work-budget check.
fn fuse_unary_kernels(plan: LogicalPlan) -> Result<LogicalPlan> {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    Ok(plan.transform_up(|plan| {
        let LogicalPlan::Extension(extension) = &plan else { return Ok(Transformed::no(plan)); };
        let Some(parent) = extension.node.as_any().downcast_ref::<RowKernel>() else { return Ok(Transformed::no(plan)); };
        if parent.boundary.is_some() || !parent.fuse_unary || parent.inputs.len()!=1 { return Ok(Transformed::no(plan)); }
        let LogicalPlan::Extension(input) = &parent.inputs[0] else { return Ok(Transformed::no(plan)); };
        let Some(child) = input.node.as_any().downcast_ref::<RowKernel>() else { return Ok(Transformed::no(plan)); };
        if child.boundary.is_some() || !child.fuse_unary || (child.inputs.len()!=1 && child.source.is_none()) || child.relational_input.is_some() || parent.relational_input.is_some()
            || !child.streaming || !parent.streaming {
            return Ok(Transformed::no(plan));
        }
        let mut fused=parent.clone();
        fused.id=NEXT_ID.fetch_add(1,Ordering::Relaxed);
        fused.name=format!("Fused({} -> {})",child.name,parent.name);
        fused.inputs=child.inputs.clone();
        fused.source=child.source.clone();
        fused.prefix=child.prefix.clone();
        fused.prefix.push(child.kernel.clone());
        fused.prefix.extend(parent.prefix.iter().cloned());
        Ok(Transformed::yes(LogicalPlan::Extension(Extension{node:Arc::new(fused)})))
    })?.data)
}

#[derive(Debug)]
pub(crate) struct KernelPlanner { pub state: Arc<Mutex<State>> }
impl KernelPlanner {
    pub(crate) fn plan(&self, kernel: &RowKernel, inputs: Vec<Arc<dyn ExecutionPlan>>) -> Result<Arc<dyn ExecutionPlan>> {
        policy::validate_kernel(kernel, policy::LoweringPolicy::SqlRequired).map_err(|e| failure(e.to_string()))?;
        Ok(Arc::new(KernelExec {
            kernel: kernel.clone(), inputs, state: self.state.clone(),
            properties: Arc::new(PlanProperties::new(Arc::new(kernel.schema.as_arrow().clone()))),
        }))
    }
}
#[derive(Debug)]
struct KernelExec {
    kernel: RowKernel,
    inputs: Vec<Arc<dyn ExecutionPlan>>,
    state: Arc<Mutex<State>>,
    properties: Arc<PlanProperties>,
}
impl ExecutionPlan for KernelExec {
    fn name(&self) -> &str { &self.kernel.name }
    fn description(&self) -> String {
        if self.kernel.streaming && self.inputs.len() == 1 && execution::needs_source_spool(&self.kernel, &self.state, &self.inputs[0]).unwrap_or(false) {
            format!("{} [source reentry: bounded Arrow IPC spool]", self.kernel.name)
        } else { self.kernel.name.clone() }
    }
    fn as_any(&self) -> &dyn Any { self }
    fn properties(&self) -> &Arc<PlanProperties> { &self.properties }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> { self.inputs.iter().collect() }
    fn with_new_children(self: Arc<Self>, inputs: Vec<Arc<dyn ExecutionPlan>>) -> Result<Arc<dyn ExecutionPlan>> {
        if inputs.len() != self.inputs.len() { return Err(failure("Invalid boundary dependency rewrite")); }
        Ok(Arc::new(Self { kernel:self.kernel.clone(),inputs,state:self.state.clone(),properties:self.properties.clone() }))
    }
    fn execute(&self, partition: usize, context: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        if partition != 0 { return Err(failure("Stateful boundary has one partition")); }
        execution::execute(self, context)
    }
}

struct Compiler<'a> {
    rule_context: crate::ir::rules::RuleContext,
    graph: &'a PropertyGraph,
    policy: crate::ir::policy::GraphPlanPolicy,
    sql: bool,
    language_functions: bool,
    native_values: bool,
    live_graph_reads: bool,
    /// Gremlin repeat bodies share deduplication history across iterations.
    stateful_distinct: bool,
    islands: super::island_planner::SharedMemo,
    lowered: std::cell::RefCell<std::collections::HashMap<usize, LogicalPlan>>,
}
impl Compiler<'_> {
    fn lower(&self, node: &Node) -> Result<LogicalPlan> {
        let key = node as *const Node as usize;
        if let Some(plan) = self.lowered.borrow_mut().remove(&key) {
            return Ok(plan);
        }
        // Rewritten subplans can own temporary IR nodes. Keep their pointer
        // identities local to this lowering call, including on failure.
        let outer = self.lowered.take();
        let result = stacker::maybe_grow(1024 * 1024, 8 * 1024 * 1024, || self.lower_worklist(node));
        self.lowered.replace(outer);
        result
    }
    fn lower_worklist(&self, node: &Node) -> Result<LogicalPlan> {
        let key = node as *const Node as usize;
        // Consume child results from a heap worklist. Native input chains no
        // longer consume one compiler stack frame per operator.
        let mut pending = vec![(node, false)];
        while let Some((node, ready)) = pending.pop() {
            let key = node as *const Node as usize;
            if ready {
                let plan = self.lower_native(node)?;
                self.lowered.borrow_mut().insert(key, plan);
            } else if let Some(plan) = self.lower_sql(node)? {
                self.lowered.borrow_mut().insert(key, plan);
            } else {
                pending.push((node, true));
                // Consecutive CREATE clauses become one ordered mutation
                // program, rather than nested physical operators/futures.
                if matches!(node, Node::GraphCreate { .. }) {
                    let mut input = node;
                    while let Node::GraphCreate { input: next, .. } = input {
                        input = next;
                    }
                    pending.push((input, false));
                } else {
                    // Correlated bodies have their own compiler and binding
                    // scope. Precompiling them here duplicates nested work.
                    let inputs = match node {
                        Node::GraphApply { left, .. } => vec![left.as_ref()],
                        Node::GraphRepeat { seed, .. } => vec![seed.as_ref()],
                        Node::GraphCoalesce { input, .. }
                        | Node::GraphChoose { input, .. }
                        | Node::GraphMerge { input, .. }
                        | Node::GraphGroupMap { input, .. }
                        | Node::GraphGroupSideEffect { input, .. }
                        | Node::GraphSideEffect { input, .. } => vec![input.as_ref()],
                        _ => crate::ir::analysis::children(node),
                    };
                    pending.extend(inputs.into_iter().rev().map(|child| (child, false)));
                }
            }
        }
        self.lowered.borrow_mut().remove(&key).ok_or_else(|| failure("Missing compiled query root"))
    }
    fn lower_sql(&self, node: &Node) -> Result<Option<LogicalPlan>> {
        if self.stateful_distinct {
            let mut pending = vec![node];
            while let Some(part) = pending.pop() {
                if matches!(part, Node::GraphDistinct { .. }) { return Ok(None); }
                pending.extend(crate::ir::analysis::children(part));
            }
        }
        if self.live_graph_reads {
            let mut pending = vec![node];
            while let Some(part) = pending.pop() {
                if matches!(part, Node::GraphNodeScan { .. } | Node::GraphRelScan { .. }) {
                    return Ok(None);
                }
                pending.extend(crate::ir::analysis::children(part));
            }
        }
        let can_lower_sql = self.sql && {
            let islands = self.islands.lock().unwrap();
            islands.safe(node)
                && !(self.policy.language == crate::ir::policy::Language::Cypher
                    && !self.language_functions
                    && self.rule_context.profile.cypher_aggregates == crate::ir::rules::CypherAggregatePolicy::Conformant
                    && islands.requires_cypher_equality(node))
        };
        let terminal_gremlin = self.language_functions
            && self.policy.language == crate::ir::policy::Language::Gremlin
            && matches!(node, Node::GraphReturn { .. })
            && super::language_functions::terminal_gremlin(node);
        if can_lower_sql
            && (terminal_gremlin || !matches!(
                node,
                Node::GraphReturn { .. }
                    | Node::GraphValues { .. }
                    | Node::GraphOneRow
                    | Node::GraphEmpty
            ))
        {
            let backend = RelBackend::with_options(super::RelBackendOptions { rule_context: self.rule_context.clone(), mapping: self.graph.mapping.clone(), language_functions: self.language_functions, native_values: self.native_values, live_graph_reads: self.live_graph_reads, ..Default::default() });
            // A complete terminal traversal needs only its returned values.
            // Partial islands must retain state for downstream graph operators.
            let backend = if terminal_gremlin { backend } else { backend.preserving_traverser_state() };
            let memo = if terminal_gremlin { super::island_planner::IslandMemo::new(node) } else { self.islands.clone() };
            let lowered = backend.lower_island(&self.policy, node, self.graph, memo);
            if std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some()
                && let Err(error) = &lowered {
                eprintln!("SQL IR lowering boundary: {error}");
            }
            if let Err(super::RelError::RulePipeline(error)) = &lowered {
                return Err(failure(error.to_string()));
            }
            if let Ok(lowered) = lowered {
                // This mixed runtime may execute a region with DataFusion,
                // whose physical LIMIT accepts literal bounds only. Retain
                // dynamic bounds in the once-per-invocation GraphSlice kernel;
                // whole-query SQL export can still emit native OFFSET/LIMIT.
                let mut dynamic_limit = false;
                lowered.plan.apply(|plan| {
                    if let LogicalPlan::Limit(limit) = plan {
                        use datafusion::logical_expr::{SkipType, FetchType};
                        dynamic_limit |= !matches!(limit.get_skip_type()?, SkipType::Literal(_))
                            || !matches!(limit.get_fetch_type()?, FetchType::Literal(_));
                    }
                    Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
                })?;
                if dynamic_limit { return Ok(None); }
                let mut adapter =
                    match kernel("DecodeTraversers", vec![lowered.plan], |mut inputs, _| {
                        Ok(inputs.remove(0))
                    }) {
                        LogicalPlan::Extension(e) => {
                            e.node.as_any().downcast_ref::<RowKernel>().unwrap().clone()
                        }
                        _ => unreachable!(),
                    };
                adapter.relational_input = Some(lowered.fields);
                return Ok(Some(LogicalPlan::Extension(Extension {
                    node: Arc::new(adapter),
                })));
            }
        }
        Ok(None)
    }
    fn lower_native(&self, node: &Node) -> Result<LogicalPlan> {
        // A lazy side effect upstream of LIMIT is a traversal demand
        // boundary: SQL batch prefetch must not invoke unconsumed effects.
        if let Node::GraphSlice { input, slice } = node
            && let Some(plan) = self.lower_bounded(input, slice)? {
                let LogicalPlan::Extension(extension) = plan else { unreachable!() };
                let mut boundary = extension.node.as_any().downcast_ref::<RowKernel>().unwrap().clone();
                boundary.boundary = Some(policy::BoundaryKind::Barrier);
                return Ok(LogicalPlan::Extension(Extension { node: Arc::new(boundary) }));
        }
        if self.rule_context.lowering_policy == policy::LoweringPolicy::SqlRequired
            && !(self.stateful_distinct && matches!(node, Node::GraphDistinct { .. }))
            && let Some(plan) = self.lower_after_boundary(node)? { return Ok(plan); }
        if matches!(node, Node::GraphAggregate { .. }) && self.policy.language == crate::ir::policy::Language::Cypher
            && self.rule_context.profile.cypher_aggregates == crate::ir::rules::CypherAggregatePolicy::SqlNative {
            return Err(failure("SQL-native aggregate semantics require SQL lowering; native conformant aggregation is not an equivalent fallback"));
        }
        if let Node::GraphSlice { input, slice } = node && self.rule_context.lowering_policy != policy::LoweringPolicy::SqlRequired {
            if let Some(plan) = self.lower_bounded(input, slice)? {
                return Ok(plan);
            }
        }
        let plan = self.lower_kernel(node)?;
        let plan = if let LogicalPlan::Extension(e) = &plan {
            if let Some(kernel) = e.node.as_any().downcast_ref::<RowKernel>() {
                let mut kernel = kernel.clone();
                kernel.boundary = kernel.boundary.or_else(|| policy::boundary(node));
                if self.stateful_distinct && matches!(node, Node::GraphDistinct { .. }) {
                    kernel.boundary = Some(policy::BoundaryKind::TraversalState);
                }
                if self.live_graph_reads && matches!(node, Node::GraphNodeScan { .. } | Node::GraphRelScan { .. }) {
                    kernel.boundary = Some(policy::BoundaryKind::GraphSource);
                }
                policy::validate_kernel(&kernel, self.rule_context.lowering_policy).map_err(|e| failure(e.to_string()))?;
                LogicalPlan::Extension(Extension { node: Arc::new(kernel) })
            } else { plan }
        } else { plan };
        Ok(optimize::annotate(plan, node))
    }
    fn lower_kernel(&self, node: &Node) -> Result<LogicalPlan> {
        if self.rule_context.profile.string_to_float == crate::ir::rules::StringToFloatPolicy::DuckDbNative {
            let mut owned_conversion = false;
            let plan = GraphPlan { policy: self.policy.clone(), root: Box::new(node.clone()) };
            crate::ir::rules::graph::visit_expressions(&plan, |expr, scope| {
                if scope.node_path.is_empty() && super::conversion_rules::contains_float_conversion(expr) {
                    owned_conversion = true;
                }
            });
            if owned_conversion { return Err(failure("native string-to-float requires its owning DuckDB engine; compatible residual evaluation is forbidden")); }
        }
        use crate::ir::plan::*;
        match node {
            Node::GraphReturn { input, .. } => self.lower(input),
            Node::GraphJvm { operation, input } => {
                let operation = operation.clone();
                let input = self.lower(input)?;
                Ok(kernel("Jvm", vec![input], move |mut inputs, state| {
                    crate::ir::jvm::execute(
                        &operation,
                        inputs.remove(0),
                        &state.graph,
                        &mut state.context.jvm,
                    )
                }))
            }
            Node::GraphValues {
                bindings,
                rows,
                bulk,
            } => {
                Ok(source_kernel("Values", Source::Values {
                    bindings: bindings.clone(), rows: Arc::new(rows.clone()), bulk: bulk.clone(),
                }))
            }

            Node::GraphOneRow => Ok(kernel("OneRow", vec![], |_, _| Ok(vec![Row::new()]))),
            Node::GraphEmpty => Ok(kernel("Empty", vec![], |_, _| Ok(vec![]))),
            // Explicit language boundaries retain their semantics; Orchid
            // schedules their inputs and SQL engines own relational work.
            _ => self.lower_scalar_kernel(node),
        }
    }
}

pub async fn execute_rows_with_jvm(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    jvm: JvmExecution,
) -> std::result::Result<(Vec<Row>, super::dag::DagStats), String> {
    stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, || {
        crate::ir::jvm::validate_computer_plan(&plan.root)
    })?;
    let mut scope = jvm.scope();
    let local=graph.clone();
    let result=execute_rows_inner(plan, &local, jvm, None, None, false).await.map_err(|e|e.to_string())?;
    if !crate::ir::jvm::contains_computer(&plan.root) {graph.restore_execution_overlay(&local);}
    scope.complete();
    Ok(result)
}
/// Diagnostic preparation uses the execution compiler and physical placement,
/// but never opens a result stream or invokes a graph kernel.
#[cfg(feature = "duckdb")]
pub(crate) async fn inspect_with_session(
    plan: &GraphPlan, graph: &PropertyGraph, resources: &super::dag::DagSession,
) -> std::result::Result<super::dag::DagStats, String> {
    let rule_context = resources.rule_context().map_err(|e| e.to_string())?;
    let logical = host::compile_for_host(plan, graph, CompileOptions {
        rule_context: rule_context.clone(), sql_islands: true,
        language_functions: resources.language_functions_enabled(), native_values: rule_context.lowering_policy == policy::LoweringPolicy::SqlRequired,
        prune_return: true,
    }).map_err(|e| e.to_string())?;
    let mut context = ExecutionContext::default();
    context.rule_context = rule_context.clone();
    let state = Arc::new(Mutex::new(State {
        graph: graph.clone(), context, frontier: vec![Row::new()], batch_key: None,
    }));
    let (_, _, stats) = super::stack::on_query_stack(super::dag::prepare_with_extensions(
        &logical, vec![Arc::new(KernelPlanner { state })], resources,
        Arc::new(Mutex::new(crate::ir::QueryCost::default())), &rule_context,
    )).await.map_err(|e| e.to_string())?;
    Ok(stats)
}

async fn execute_rows_inner(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    jvm: JvmExecution,
    timeout: Option<std::time::Duration>,
    resources: Option<&super::dag::DagSession>,
    prune_return: bool,
) -> std::result::Result<(Vec<Row>, super::dag::DagStats), QueryExecutionError> {
    #[cfg(feature = "duckdb")]
    let mapped_resources = if resources.is_none() { graph.source.as_ref().zip(graph.mapping.as_ref()).and_then(|(source,mapping)| source.executor().map(|executor| super::dag::DagSession::with_shared(executor,mapping.physical_table_names()))) } else {None};
    #[cfg(feature = "duckdb")]
    let resources = resources.or(mapped_resources.as_ref());
    graph.begin_statement();
    let rule_context = resources.map(|r| r.rule_context()).transpose().map_err(QueryExecutionError::from_error)?.unwrap_or_default();
    let logical = host::compile_for_host(plan, graph, CompileOptions {
        rule_context: rule_context.clone(),
        sql_islands: true,
        language_functions: resources.is_some_and(|r| r.language_functions_enabled()),
        native_values: rule_context.lowering_policy == policy::LoweringPolicy::SqlRequired,
        prune_return,
    })?;
    let mut context = ExecutionContext::default();
    context.rule_context = rule_context.clone();
    context.region_session = resources.and_then(|r|r.region_session.clone());
    context.jvm = jvm;
    context.sql_timeout = timeout;
    let state = Arc::new(Mutex::new(State {
        graph: graph.clone(),
        context,
        frontier: vec![Row::new()],
        batch_key: None,
    }));
    let lowered = LoweredPlan {
            rule_context: rule_context.clone(),
        plan: logical,
        fields: vec!["bindings".into(), "bulk".into()],
        result_form: crate::ir::policy::ResultForm::RowSet,
        islands: Default::default(),
    };
    let (returned, mut stats) = super::stack::on_query_stack(super::dag::execute_with_extensions(
        lowered,
        vec![Arc::new(KernelPlanner {
            state: state.clone(),
        })],
        timeout,
        resources,
    ))
    .await
    .map_err(QueryExecutionError::from_error)?;
    let rows = decode_rows(&returned.batch).map_err(QueryExecutionError::from_error)?;
    let state = state.lock().map_err(|_| "Query state poisoned")?;
    state.context.jvm.check().map_err(QueryExecutionError::from_error)?;
    graph.restore_execution_overlay(&state.graph);
    stats.merge_execution(&state.context.nested_dag_stats);
    stats.rule_trace = rule_context.trace();
    stats.cost.add_work(&state.context.query_cost);
    Ok((rows, stats))
}

impl Compiler<'_> {
    #[allow(unused_variables, unused_mut)]
    fn lower_scalar_kernel(&self, node: &Node) -> Result<LogicalPlan> {
        use crate::ir::runtime::ops::*;
        use crate::ir::runtime::ops::{
            aggregate::{aggregate_op, aggregate_op_with_cypher_equivalence},
            barrier::barrier_op,
            collect::collect_op,
            distinct::{
                distinct_op, distinct_op_with_seen, row_signature,
                row_signature_with_cypher_equivalence,
                distinct_op_with_seen_and_cypher_equivalence,
            },
            expand::{bind_op, expand_op},
            join::join_op,
            list_comprehension::list_comprehension_op,
            mutation::{create_op, delete_op, set_property_op},
            path_pattern::path_pattern_op,
            project::{current_project_op, project_op},
            quantifier::quantifier_op,
            select::select_op,
            slice::{slice_expr_op, slice_op},
            sort::sort_op,
            unwind::unwind_op,
        };
        use std::collections::BTreeSet;
        match node {
            Node::GraphNodeScan {
                binding, labels, ..
            } => {
                Ok(source_kernel("NodeScan", Source::Nodes { binding: binding.clone(), labels: labels.clone() }))
            }
            Node::GraphRelScan { binding, types, .. } => {
                Ok(source_kernel("RelScan", Source::Edges { binding: binding.clone(), types: types.clone() }))
            }
            Node::GraphBind {
                bind,
                kind,
                expr,
                input,
            } => {
                let bind = bind.clone();
                let kind = kind.clone();
                let expr = expr.clone();
                Ok(kernel(
                    "Bind",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let bind = &bind;
                        let kind = &kind;
                        let expr = &expr;
                        {
                            let upstream = inputs.remove(0);
                            bind_op(bind, *kind, expr.as_ref(), upstream)
                        }
                    },
                ))
            }
            Node::GraphExpand {
                source,
                target,
                target_mode,
                target_labels,
                rel_binding,
                rel_types,
                dir,
                length,
                history,
                path,
                path_mode,
                match_mode,
                input,
                ..
            } => {
                let source = source.clone();
                let target = target.clone();
                let target_mode = target_mode.clone();
                let target_labels = target_labels.clone();
                let rel_binding = rel_binding.clone();
                let rel_types = rel_types.clone();
                let dir = dir.clone();
                let length = length.clone();
                let history = history.clone();
                let path = path.clone();
                let path_mode = path_mode.clone();
                let match_mode = match_mode.clone();
                Ok(kernel(
                    "Expand",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let source = &source;
                        let target = &target;
                        let target_mode = &target_mode;
                        let target_labels = &target_labels;
                        let rel_binding = &rel_binding;
                        let rel_types = &rel_types;
                        let dir = &dir;
                        let length = &length;
                        let history = &history;
                        let path = &path;
                        let path_mode = &path_mode;
                        let match_mode = &match_mode;
                        {
                            let upstream = inputs.remove(0);
                            expand_op(
                                source,
                                target,
                                *target_mode,
                                target_labels,
                                rel_binding.as_deref(),
                                rel_types,
                                *dir,
                                length,
                                history.as_deref(),
                                path.as_deref(),
                                *path_mode,
                                *match_mode,
                                upstream,
                                graph,
                                ctx,
                            )
                        }
                    },
                ))
            }
            Node::GraphPathFilter {
                condition, input, ..
            } => {
                let condition = condition.clone();
                Ok(kernel(
                    "PathFilter",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let condition = &condition;
                        {
                            let rows = inputs.remove(0);
                            Ok(rows
                                .into_iter()
                                .filter(|row| match eval(condition, row, graph) {
                                    Ok(Value::Bool(true)) => true,
                                    _ => false,
                                })
                                .collect())
                        }
                    },
                ))
            }
            Node::GraphCreate { .. } => {
                let mut stages = Vec::new();
                let mut input = node;
                while let Node::GraphCreate { nodes, edges, input: next, .. } = input {
                    stages.push((nodes.clone(), edges.clone()));
                    input = next;
                }
                stages.reverse();
                Ok(kernel(
                    "Create",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let mut rows = inputs.remove(0);
                        for (index, (nodes, edges)) in stages.iter().enumerate() {
                            if index > 0 {
                                state.context.charge(1)?;
                                state.context.query_cost.native_kernel_calls += 1;
                                state.context.query_cost.native_input_rows += rows.len() as u64;
                                state.graph.normalize_source_rows(&mut rows).map_err(RuntimeError::Runtime)?;
                                state.graph.prefetch_source(&rows);
                                state.graph.check_source().map_err(RuntimeError::Runtime)?;
                            }
                            rows = create_op(nodes, edges, rows, &state.graph)?;
                            if index + 1 < stages.len() {
                                state.context.query_cost.native_output_rows += rows.len() as u64;
                            }
                        }
                        Ok(rows)
                    },
                ))
            }
            Node::GraphSetProperty { items, input } => {
                let items = items.clone();
                Ok(kernel(
                    "SetProperty",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let items = &items;
                        {
                            let rows = inputs.remove(0);
                            set_property_op(items, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphDelete {
                targets,
                detach,
                input,
            } => {
                let targets = targets.clone();
                let detach = detach.clone();
                Ok(kernel(
                    "Delete",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let targets = &targets;
                        let detach = &detach;
                        {
                            let rows = inputs.remove(0);
                            delete_op(targets, *detach, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphFilter { condition, input } => {
                let condition = condition.clone();
                Ok(kernel(
                    "Filter",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let condition = &condition;
                        {
                            let rows = inputs.remove(0);
                            let mut out = Vec::new();
                            for row in rows {
                                match eval(condition, &row, graph)? {
                                    Value::Bool(true) => out.push(row),
                                    _ => {}
                                }
                            }
                            Ok(out)
                        }
                    },
                ))
            }
            Node::GraphProject {
                mode, items, input, ..
            } => {
                let mode = mode.clone();
                let items = items.clone();
                Ok(kernel(
                    "Project",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let mode = &mode;
                        let items = &items;
                        {
                            let rows = inputs.remove(0);
                            project_op(*mode, items, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphCurrentProject { expr, input, .. } => {
                let expr = expr.clone();
                Ok(kernel(
                    "CurrentProject",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let expr = &expr;
                        {
                            let rows = inputs.remove(0);
                            current_project_op(expr, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphAggregate {
                group, aggs, input, ..
            } => {
                let group = group.clone();
                let aggs = aggs.clone();
                let cypher_equivalence =
                    self.policy.language == crate::ir::policy::Language::Cypher;
                Ok(kernel(
                    "Aggregate",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let group = &group;
                        let aggs = &aggs;
                        {
                            let rows = inputs.remove(0);
                            if cypher_equivalence {
                                aggregate_op_with_cypher_equivalence(
                                    group, aggs, rows, graph, true,
                                )
                            } else {
                                aggregate_op(group, aggs, rows, graph)
                            }
                        }
                    },
                ))
            }
            Node::GraphDistinct {
                keys, mode, input, ..
            } => {
                let keys = keys.clone();
                let mode = mode.clone();
                let cypher_equivalence =
                    self.policy.language == crate::ir::policy::Language::Cypher;
                Ok(kernel(
                    "Distinct",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let keys = &keys;
                        let mode = &mode;
                        {
                            let rows = inputs.remove(0);
                            if let Some(seen) = ctx.next_distinct_seen() {
                                if cypher_equivalence {
                                    distinct_op_with_seen_and_cypher_equivalence(
                                        keys, *mode, rows, seen, true,
                                    )
                                } else {
                                    distinct_op_with_seen(keys, *mode, rows, seen)
                                }
                            } else {
                                if cypher_equivalence {
                                    let mut seen = BTreeSet::new();
                                    distinct_op_with_seen_and_cypher_equivalence(
                                        keys, *mode, rows, &mut seen, true,
                                    )
                                } else {
                                    distinct_op(keys, *mode, rows)
                                }
                            }
                        }
                    },
                ))
            }
            Node::GraphSort { keys, input } => {
                let keys = keys.clone();
                Ok(kernel(
                    "Sort",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let keys = &keys;
                        {
                            let rows = inputs.remove(0);
                            sort_op(keys, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphSlice { slice, input } => {
                let slice = slice.clone();
                Ok(kernel(
                    "Slice",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let slice = &slice;
                        {
                            let rows = inputs.remove(0);
                            slice_op(slice, rows)
                        }
                    },
                ))
            }
            Node::GraphSliceExpr {
                offset,
                fetch,
                input,
            } => {
                let offset = offset.clone();
                let fetch = fetch.clone();
                Ok(kernel(
                    "SliceExpr",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let offset = &offset;
                        let fetch = &fetch;
                        {
                            let rows = inputs.remove(0);
                            slice_expr_op(offset.as_ref(), fetch.as_ref(), rows, graph)
                        }
                    },
                ))
            }
            Node::GraphJoin {
                left,
                right,
                condition,
                ..
            } => {
                let condition = condition.clone();
                Ok(kernel(
                    "Join",
                    vec![self.lower(left)?, self.lower(right)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let condition = &condition;
                        {
                            let l = inputs.remove(0);
                            let r = inputs.remove(0);
                            join_op(l, r, condition.as_ref(), graph)
                        }
                    },
                ))
            }
            Node::GraphUnion {
                all, left, right, ..
            } => {
                let all = all.clone();
                let cypher_equivalence =
                    self.policy.language == crate::ir::policy::Language::Cypher;
                Ok(kernel(
                    "Union",
                    vec![self.lower(left)?, self.lower(right)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let all = &all;
                        {
                            let mut l = inputs.remove(0);
                            let r = inputs.remove(0);
                            l.extend(r);
                            if !all {
                                let mut seen = BTreeSet::new();
                                l.retain(|row| {
                                    let key = if cypher_equivalence {
                                        row_signature_with_cypher_equivalence(row)
                                    } else {
                                        row_signature(row)
                                    };
                                    seen.insert(key)
                                });
                            }
                            Ok(l)
                        }
                    },
                ))
            }
            Node::GraphUnwind {
                input_expr,
                bind,
                outer,
                input,
            } => {
                let input_expr = input_expr.clone();
                let bind = bind.clone();
                let outer = outer.clone();
                Ok(kernel(
                    "Unwind",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let input_expr = &input_expr;
                        let bind = &bind;
                        let outer = &outer;
                        {
                            let rows = inputs.remove(0);
                            unwind_op(input_expr, bind, *outer, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphSelect {
                labels,
                outputs,
                input,
            } => {
                let labels = labels.clone();
                let outputs = outputs.clone();
                Ok(kernel(
                    "Select",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let labels = &labels;
                        let outputs = &outputs;
                        {
                            let rows = inputs.remove(0);
                            select_op(labels, outputs, rows)
                        }
                    },
                ))
            }
            Node::GraphPathPattern {
                path,
                selector,
                parts,
                path_mode,
                match_mode,
                input,
                ..
            } => {
                let path = path.clone();
                let selector = selector.clone();
                let parts = parts.clone();
                let path_mode = *path_mode;
                let match_mode = *match_mode;
                Ok(kernel(
                    "PathPattern",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let path = &path;
                        let selector = &selector;
                        let parts = &parts;
                        {
                            let upstream = inputs.remove(0);
                            path_pattern_op(path, selector, parts, path_mode, match_mode, upstream, graph, ctx)
                        }
                    },
                ))
            }
            Node::GraphSample {
                kind,
                seed,
                step_id,
                weight,
                input,
            } => {
                let kind = kind.clone();
                let seed = seed.clone();
                let step_id = step_id.clone();
                let weight = weight.clone();
                Ok(kernel(
                    "Sample",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let kind = &kind;
                        let seed = &seed;
                        let step_id = &step_id;
                        let weight = &weight;
                        {
                            let rows = inputs.remove(0);
                            let rng = ctx
                                .random_steps
                                .entry(step_id.clone())
                                .or_insert_with(|| sample::JavaRandom::new(*seed));
                            sample::sample_op(*kind, weight.as_ref(), rows, graph, rng)
                        }
                    },
                ))
            }
            Node::GraphBarrier {
                partition,
                order,
                slice,
                materialize,
                bulk_policy,
                input,
            } => {
                let partition = partition.clone();
                let order = order.clone();
                let slice = slice.clone();
                let materialize = materialize.clone();
                let bulk_policy = bulk_policy.clone();
                Ok(kernel(
                    "Barrier",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let partition = &partition;
                        let order = &order;
                        let slice = &slice;
                        let materialize = &materialize;
                        let bulk_policy = &bulk_policy;
                        {
                            let rows = inputs.remove(0);
                            barrier_op(
                                partition,
                                order,
                                slice,
                                *materialize,
                                *bulk_policy,
                                rows,
                                graph,
                            )
                        }
                    },
                ))
            }
            Node::GraphGroupCountSideEffect { label, key, input } => {
                let label = label.clone();
                let key = key.clone();
                Ok(kernel(
                    "GroupCountSideEffect",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let label = &label;
                        let key = &key;
                        {
                            let rows = inputs.remove(0);
                            let counts = ctx.group_counts.entry(label.clone()).or_default();
                            for row in &rows {
                                let key_value = eval(key, row, graph)?;
                                if let Some((_, count)) =
                                    counts.iter_mut().find(|(key, _)| key == &key_value)
                                {
                                    *count += row.bulk;
                                } else {
                                    counts.push((key_value, row.bulk));
                                }
                            }
                            Ok(rows)
                        }
                    },
                ))
            }
            Node::GraphShortestPath {
                source,
                target,
                direction,
                rel_types,
                max_distance,
                include_edges,
                output,
                all_paths,
                input,
            } => {
                let source = source.clone();
                let target = target.clone();
                let direction = direction.clone();
                let rel_types = rel_types.clone();
                let max_distance = max_distance.clone();
                let include_edges = include_edges.clone();
                let output = output.clone();
                let all_paths = all_paths.clone();
                Ok(kernel(
                    "ShortestPath",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let source = &source;
                        let target = &target;
                        let direction = &direction;
                        let rel_types = &rel_types;
                        let max_distance = &max_distance;
                        let include_edges = &include_edges;
                        let output = &output;
                        let all_paths = &all_paths;
                        {
                            let rows = inputs.remove(0);
                            crate::ir::runtime::context::shortest_path_op(
                                source,
                                target.as_deref(),
                                *direction,
                                rel_types,
                                *max_distance,
                                *include_edges,
                                output,
                                *all_paths,
                                rows,
                                graph,
                            )
                        }
                    },
                ))
            }
            Node::GraphQuantifier {
                kind,
                item_binding,
                input_expr,
                predicate,
                output,
                input,
            } => {
                let kind = kind.clone();
                let item_binding = item_binding.clone();
                let input_expr = input_expr.clone();
                let predicate = predicate.clone();
                let output = output.clone();
                Ok(kernel(
                    "Quantifier",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let kind = &kind;
                        let item_binding = &item_binding;
                        let input_expr = &input_expr;
                        let predicate = &predicate;
                        let output = &output;
                        {
                            let rows = inputs.remove(0);
                            quantifier_op(
                                *kind,
                                item_binding,
                                input_expr,
                                predicate,
                                output,
                                rows,
                                graph,
                            )
                        }
                    },
                ))
            }
            Node::GraphCollect {
                value,
                distinct,
                order,
                alias,
                input,
            } => {
                let value = value.clone();
                let distinct = distinct.clone();
                let order = order.clone();
                let alias = alias.clone();
                Ok(kernel(
                    "Collect",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let value = &value;
                        let distinct = &distinct;
                        let order = &order;
                        let alias = &alias;
                        {
                            let rows = inputs.remove(0);
                            collect_op(value, *distinct, order, alias, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphExtension { name, .. } => {
                let name = name.clone();
                Ok(kernel("Extension", vec![], move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    let name = &name;
                    {
                        Err(RuntimeError::Unsupported(format!(
                            "GraphExtension({name}): extension nodes have no runtime"
                        )))
                    }
                }))
            }
            Node::GraphSparqlTriplePattern { .. } | Node::GraphSparqlGraphNames { .. } => Ok(kernel(
                "SparqlTriplePattern",
                vec![],
                move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    {
                        Err(RuntimeError::Unsupported(
                            "unresolved SPARQL triple patterns must pass through ontology mapping"
                                .into(),
                        ))
                    }
                },
            )),
            Node::GraphRdfPropertyPath { .. } => Ok(kernel(
                "RdfPropertyPath",
                vec![],
                move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    {
                        Err(RuntimeError::Unsupported(
                            "GraphRdfPropertyPath: SPARQL property paths require an RDF store"
                                .into(),
                        ))
                    }
                },
            )),
            Node::GraphSparqlMinus { .. } => {
                Ok(kernel("SparqlMinus", vec![], move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    {
                        Err(RuntimeError::Unsupported(
                            "GraphSparqlMinus: solution-mapping MINUS is not yet implemented"
                                .into(),
                        ))
                    }
                }))
            }
            Node::GraphConstructTriples { .. } => Ok(kernel(
                "ConstructTriples",
                vec![],
                move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    {
                        Err(RuntimeError::Unsupported(
                            "GraphConstructTriples: SPARQL CONSTRUCT output is not yet implemented"
                                .into(),
                        ))
                    }
                },
            )),
            Node::GraphDescribe { .. } => {
                Ok(kernel("Describe", vec![], move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    {
                        Err(RuntimeError::Unsupported(
                            "GraphDescribe: SPARQL DESCRIBE output is not yet implemented".into(),
                        ))
                    }
                }))
            }
            Node::GraphAsk { field, input } => {
                let field = field.clone();
                Ok(kernel(
                    "Ask",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let field = &field;
                        {
                            let answer = !inputs.remove(0).is_empty();
                            Ok(vec![Row::new().with(field, Value::Bool(answer))])
                        }
                    },
                ))
            }
            Node::GraphListComprehension {
                input_expr,
                item,
                filter,
                map_expr,
                alias,
                input,
            } => {
                let input_expr = input_expr.clone();
                let item = item.clone();
                let filter = filter.clone();
                let map_expr = map_expr.clone();
                let alias = alias.clone();
                Ok(kernel(
                    "ListComprehension",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let input_expr = &input_expr;
                        let item = &item;
                        let filter = &filter;
                        let map_expr = &map_expr;
                        let alias = &alias;
                        {
                            let rows = inputs.remove(0);
                            list_comprehension_op(
                                input_expr,
                                item,
                                filter.as_ref(),
                                map_expr.as_ref(),
                                alias,
                                rows,
                                graph,
                            )
                        }
                    },
                ))
            }
            other => self.lower_control(other),
        }
    }
}


pub(crate) mod control;

/// Execute one statement atomically, including language result conversion.
pub async fn execute(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    timeout: Option<std::time::Duration>,
) -> std::result::Result<(ReturnedBatches, super::dag::DagStats), String> {
    execute_with_session(plan, graph, timeout, None, Default::default()).await.map_err(|e|e.to_string())
}

pub(crate) async fn execute_with_session(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    timeout: Option<std::time::Duration>,
    resources: Option<&super::dag::DagSession>,
    jvm_workers: crate::ir::jvm::JvmWorkerPool,
) -> std::result::Result<(ReturnedBatches, super::dag::DagStats), QueryExecutionError> {
    stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, || {
        crate::ir::jvm::validate_computer_plan(&plan.root)
    })?;
    let local = graph.clone();
    let started = std::time::Instant::now();
    let jvm = JvmExecution::for_query(jvm_workers);
    let mut scope = jvm.scope();
    let (rows, mut stats) = execute_rows_inner(plan, &local, jvm, timeout, resources, true).await?;
    let (fields, form) = match plan.root.as_ref() {
        Node::GraphReturn {
            fields,
            result_form,
            ..
        } => (fields.clone(), *result_form),
        Node::GraphAsk { field, .. } => {
            (vec![field.clone()], crate::ir::policy::ResultForm::Boolean)
        }
        _ => (
            rows.first()
                .map(|r| r.bindings.keys().cloned().collect())
                .unwrap_or_default(),
            crate::ir::policy::ResultForm::RowSet,
        ),
    };
    local.prefetch_source(&rows);
    local.check_source()?;
    let returned =
        crate::ir::runtime::output::finalize_return(&fields, form, rows, &local, &plan.policy)
            .map_err(QueryExecutionError::from_error)?;
    local.check_source()?;
    if !crate::ir::jvm::contains_computer(&plan.root) { graph.restore_execution_overlay(&local); }
    stats.cost.result_rows = returned.batch.num_rows() as u64;
    stats.cost.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    scope.complete();
    Ok((returned, stats))
}

fn has_mutating_branches(node: &Node) -> bool {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        let children = crate::ir::analysis::children(node);
        if children.len() > 1 && crate::ir::exec::contains_mutation(node) {
            return true;
        }
        pending.extend(children);
    }
    false
}

pub(crate) fn is_kernel_exec(plan: &dyn ExecutionPlan) -> bool { plan.as_any().is::<KernelExec>() }
