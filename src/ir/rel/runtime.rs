//! Relational extension kernels executed by DataFusion. Graph IR is consumed
//! during compilation only; physical operators contain scalar metadata and
//! Arrow inputs, never an executable Graph IR subtree.
use crate::ir::diagnostics::QueryExecutionError;
use super::{LoweredPlan, RelBackend};
use crate::ir::{
    catalog::{
        PropertyGraph,
        snapshot::binary::{decode_value_bytes, encode_value},
    },
    runtime::{RuntimeError, IrResult, ReturnedBatches, Row, eval, context::ExecutionContext},
    jvm::JvmExecution,
    plan::{GraphPlan, Node},
    value::Value,
};
use arrow::{
    array::{Array, BinaryArray, BinaryBuilder, RecordBatch, UInt64Array},
    datatypes::{DataType, Field, Schema, SchemaRef},
};
use async_trait::async_trait;
use datafusion::{
    common::{DFSchemaRef, DataFusionError, Result},
    execution::{TaskContext, context::SessionState},
    logical_expr::{
        Expr, Extension, LogicalPlan, UserDefinedLogicalNode, UserDefinedLogicalNodeCore,
    },
    physical_expr::EquivalenceProperties,
    physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
        SendableRecordBatchStream,
        execution_plan::{Boundedness, EmissionType},
        stream::RecordBatchStreamAdapter,
    },
    physical_planner::{ExtensionPlanner, PhysicalPlanner},
};
use futures::stream;
use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Debug, Default)]
pub(crate) struct State {
    pub graph: PropertyGraph,
    pub context: ExecutionContext,
    pub frontier: Vec<Row>,
    pub batch_key: Option<String>,
}
mod optimize;

type Kernel = dyn Fn(Vec<Vec<Row>>, &mut State) -> IrResult<Vec<Row>> + Send + Sync;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
fn failure(e: impl ToString) -> DataFusionError {
    DataFusionError::Execution(e.to_string())
}
fn schema() -> SchemaRef {
    static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
    SCHEMA.get_or_init(|| Arc::new(Schema::new(vec![
        Field::new("bindings", DataType::Binary, false),
        Field::new("bulk", DataType::UInt64, false),
    ]))).clone()
}
fn encode_rows(rows: Vec<Row>) -> Result<RecordBatch> {
    let mut bindings = BinaryBuilder::new();
    let mut bulk = Vec::with_capacity(rows.len());
    let mut encoded = Vec::new();
    for row in rows {
        encoded.clear();
        encode_value(&mut encoded, &Value::Map(row.bindings));
        bindings.append_value(&encoded);
        bulk.push(row.bulk);
    }
    Ok(RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(bindings.finish()),
            Arc::new(UInt64Array::from(bulk)),
        ],
    )?)
}
fn decode_rows(batch: &RecordBatch) -> Result<Vec<Row>> {
    if batch.schema().as_ref() != schema().as_ref() {
        return Err(failure("Invalid traverser batch schema"));
    }
    let bindings = batch
        .column(0)
        .as_any()
        .downcast_ref::<BinaryArray>()
        .ok_or_else(|| failure("Invalid bindings"))?;
    let bulk = batch
        .column(1)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .ok_or_else(|| failure("Invalid bulk"))?;
    (0..batch.num_rows())
        .map(|i| {
            if bindings.is_null(i) || bulk.is_null(i) {
                return Err(failure("Null traverser transport field"));
            }
            let Value::Map(bindings) = decode_value_bytes(bindings.value(i)).map_err(failure)?
            else {
                return Err(failure("Invalid traverser bindings"));
            };
            Ok(Row {
                bindings,
                bulk: bulk.value(i),
            })
        })
        .collect()
}
#[derive(Clone)]
pub(crate) struct RowKernel {
    id: u64,
    name: String,
    inputs: Vec<LogicalPlan>,
    schema: DFSchemaRef,
    kernel: Arc<Kernel>,
    relational_input: Option<Vec<String>>,
    /// Adjacent unary kernels may retain rows in memory without reordering evaluation.
    fuse_unary: bool,
    contract: Option<optimize::Contract>,
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
            contract: None,
            fuse_unary: matches!(name,
                "Bind" | "Filter" | "Project" | "CurrentProject" | "Return" | "RetainBindings"
                | "Expand" | "PathFilter" | "CorrelatedInput"
                | "NodeScan" | "RelScan" | "Values" | "OneRow" | "Empty"
            ),
        }),
    })
}
/// Fuse adjacent read kernels, including their zero-input sources. This does not move predicates,
/// change evaluation order, share branches, or cross SQL/JVM/write boundaries.
/// Every original operator keeps its cancellation and work-budget check.
fn fuse_unary_kernels(plan: LogicalPlan) -> Result<LogicalPlan> {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    Ok(plan.transform_up(|plan| {
        let LogicalPlan::Extension(extension) = &plan else { return Ok(Transformed::no(plan)); };
        let Some(parent) = extension.node.as_any().downcast_ref::<RowKernel>() else { return Ok(Transformed::no(plan)); };
        if !parent.fuse_unary || parent.inputs.len()!=1 { return Ok(Transformed::no(plan)); }
        let LogicalPlan::Extension(input) = &parent.inputs[0] else { return Ok(Transformed::no(plan)); };
        let Some(child) = input.node.as_any().downcast_ref::<RowKernel>() else { return Ok(Transformed::no(plan)); };
        if !child.fuse_unary || child.inputs.len()>1 || child.relational_input.is_some() || parent.relational_input.is_some() {
            return Ok(Transformed::no(plan));
        }
        let mut fused=parent.clone();
        fused.id=NEXT_ID.fetch_add(1,Ordering::Relaxed);
        fused.name=format!("Fused({} -> {})",child.name,parent.name);
        fused.inputs=child.inputs.clone();
        let before=child.kernel.clone();let after=parent.kernel.clone();
        fused.kernel=Arc::new(move |inputs,state| {
            let rows=before(inputs,state)?;
            state.context.jvm.check()?;
            state.context.charge(1)?;
            after(vec![rows],state)
        });
        Ok(Transformed::yes(LogicalPlan::Extension(Extension{node:Arc::new(fused)})))
    })?.data)
}

#[derive(Debug)]
pub(crate) struct KernelPlanner {
    pub state: Arc<Mutex<State>>,
}
#[async_trait]
impl ExtensionPlanner for KernelPlanner {
    async fn plan_extension(
        &self,
        _planner: &dyn PhysicalPlanner,
        node: &dyn UserDefinedLogicalNode,
        _logical_inputs: &[&LogicalPlan],
        physical_inputs: &[Arc<dyn ExecutionPlan>],
        _session: &SessionState,
    ) -> Result<Option<Arc<dyn ExecutionPlan>>> {
        let Some(kernel) = node.as_any().downcast_ref::<RowKernel>() else {
            return Ok(None);
        };
        let properties = Arc::new(PlanProperties::new(
            EquivalenceProperties::new(schema()),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Final,
            Boundedness::Bounded,
        ));
        Ok(Some(Arc::new(KernelExec {
            kernel: kernel.clone(),
            inputs: physical_inputs.to_vec(),
            state: self.state.clone(),
            properties,
        })))
    }
}
#[derive(Debug)]
struct KernelExec {
    kernel: RowKernel,
    inputs: Vec<Arc<dyn ExecutionPlan>>,
    state: Arc<Mutex<State>>,
    properties: Arc<PlanProperties>,
}
impl DisplayAs for KernelExec {
    fn fmt_as(&self, _format: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}Exec", self.kernel.name)
    }
}
impl ExecutionPlan for KernelExec {
    fn name(&self) -> &str {
        &self.kernel.name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        self.inputs.iter().collect()
    }
    fn with_new_children(
        self: Arc<Self>,
        inputs: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if inputs.len() != self.inputs.len() {
            return Err(failure("Invalid kernel physical rewrite"));
        }
        Ok(Arc::new(Self {
            kernel: self.kernel.clone(),
            inputs,
            state: self.state.clone(),
            properties: self.properties.clone(),
        }))
    }
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(failure("Stateful kernel has one partition"));
        }
        let inputs = self.inputs.clone();
        let state = self.state.clone();
        let kernel = self.kernel.clone();
        let work = async move {
            let mut input_rows = Vec::new();
            // Preserve input order for observable effects. No parallel branch replay.
            for input in inputs {
                let batches = datafusion::physical_plan::collect(input, context.clone()).await?;
                let mut rows = Vec::new();
                for batch in batches {
                    if let Some(fields) = &kernel.relational_input {
                        let returned = ReturnedBatches {
                            fields: fields.clone(),
                            result_form: crate::ir::policy::ResultForm::RowSet,
                            batch,
                        };
                        let (bindings, values) = crate::ir::exec::batch_to_bindings(&returned)
                            .ok_or_else(|| failure("Unsupported Arrow boundary value"))?;
                        rows.extend(values.into_iter().map(|values| Row {
                            bindings: bindings.iter().cloned().zip(values).collect(),
                            bulk: 1,
                        }));
                    } else {
                        rows.extend(decode_rows(&batch)?);
                    }
                }
                input_rows.push(rows);
            }
            tokio::task::spawn_blocking(move || {
                let mut state = state.lock().map_err(|_| failure("Query state poisoned"))?;
                state.context.jvm.check().map_err(failure)?;
                state.context.charge(1).map_err(failure)?;
                let rows = (kernel.kernel)(input_rows, &mut state)
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                encode_rows(rows)
            })
            .await
            .map_err(failure)?
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            schema(),
            stream::once(work),
        )))
    }
}

struct Compiler<'a> {
    graph: &'a PropertyGraph,
    policy: crate::ir::policy::GraphPlanPolicy,
    sql: bool,
    islands: super::island_planner::SharedMemo,
}
impl Compiler<'_> {
    fn lower(&self, node: &Node) -> Result<LogicalPlan> {
        if self.sql
            && self.islands.lock().unwrap().safe(node)
            && !matches!(
                node,
                Node::GraphReturn { .. }
                    | Node::GraphValues { .. }
                    | Node::GraphOneRow
                    | Node::GraphEmpty
            )
        {
            let lowered = RelBackend::new()
                .preserving_traverser_state()
                .lower_island(&self.policy, node, self.graph, self.islands.clone());
            if std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some()
                && let Err(error) = &lowered {
                eprintln!("SQL IR lowering boundary: {error}");
            }
            if let Ok(lowered) = lowered {
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
                return Ok(LogicalPlan::Extension(Extension {
                    node: Arc::new(adapter),
                }));
            }
        }
        if let Node::GraphSlice { input, slice } = node {
            if let Some(plan) = self.lower_bounded(input, slice)? {
                return Ok(plan);
            }
        }
        self.lower_kernel(node).map(|plan| optimize::annotate(plan, node))
    }
    fn lower_kernel(&self, node: &Node) -> Result<LogicalPlan> {
        use crate::ir::runtime::ops::*;
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
                let bindings = bindings.clone();
                let rows = rows.clone();
                let bulk = bulk.clone();
                Ok(kernel("Values", vec![], move |_, _| {
                    source::values_op(&bindings, &rows, bulk.as_deref())
                }))
            }
            Node::GraphOneRow => Ok(kernel("OneRow", vec![], |_, _| Ok(vec![Row::new()]))),
            Node::GraphEmpty => Ok(kernel("Empty", vec![], |_, _| Ok(vec![]))),
            // Scalar kernels retain language semantics; DataFusion schedules
            // their inputs and owns the physical pipeline.
            _ => self.lower_scalar_kernel(node),
        }
    }
}

pub async fn execute_rows_with_jvm(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    jvm: JvmExecution,
) -> std::result::Result<(Vec<Row>, super::dag::DagStats), String> {
    crate::ir::jvm::validate_computer_plan(&plan.root)?;
    let local=graph.clone();
    let result=execute_rows_inner(plan, &local, jvm, None, None, false).await.map_err(|e|e.to_string())?;
    if !crate::ir::jvm::contains_computer(&plan.root) {graph.restore_execution_overlay(&local);}
    Ok(result)
}
async fn execute_rows_inner(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    jvm: JvmExecution,
    timeout: Option<std::time::Duration>,
    resources: Option<&super::dag::DagSession>,
    prune_return: bool,
) -> std::result::Result<(Vec<Row>, super::dag::DagStats), QueryExecutionError> {
    graph.begin_statement();
    let compiler = Compiler {
        graph,
        policy: plan.policy.clone(),
        sql: !has_mutating_branches(&plan.root),
        islands: super::island_planner::IslandMemo::new(&plan.root),
    };
    let needed = if prune_return { match plan.root.as_ref() {
        Node::GraphReturn {fields,..} => Some(fields.iter().cloned().collect()), _=>None
    }} else {None};
    let logical = optimize::optimize(compiler.lower(&plan.root).map_err(QueryExecutionError::from_error)?, needed.as_ref()).map_err(QueryExecutionError::from_error)?;
    // Lowering memo entries may include abandoned candidate sources. The
    // executable DAG owns the sources it uses; release the compiler before I/O.
    drop(compiler);
    let mut context = ExecutionContext::default();
    context.jvm = jvm;
    context.sql_timeout = timeout;
    let state = Arc::new(Mutex::new(State {
        graph: graph.clone(),
        context,
        frontier: vec![Row::new()],
        batch_key: None,
    }));
    let lowered = LoweredPlan {
        plan: logical,
        fields: vec!["bindings".into(), "bulk".into()],
        result_form: crate::ir::policy::ResultForm::RowSet,
        islands: Default::default(),
    };
    let (returned, stats) = super::dag::execute_with_extensions(
        lowered,
        vec![Arc::new(KernelPlanner {
            state: state.clone(),
        })],
        timeout,
        resources,
    )
    .await
    .map_err(QueryExecutionError::from_error)?;
    let rows = decode_rows(&returned.batch).map_err(QueryExecutionError::from_error)?;
    let state = state.lock().map_err(|_| "Query state poisoned")?;
    state.context.jvm.check().map_err(QueryExecutionError::from_error)?;
    graph.restore_execution_overlay(&state.graph);
    Ok((rows, stats))
}

impl Compiler<'_> {
    #[allow(unused_variables, unused_mut)]
    fn lower_scalar_kernel(&self, node: &Node) -> Result<LogicalPlan> {
        use crate::ir::runtime::ops::*;
        use crate::ir::runtime::ops::{
            aggregate::aggregate_op,
            barrier::barrier_op,
            collect::collect_op,
            distinct::{distinct_op, row_signature},
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
            source::{node_scan, rel_scan},
            unwind::unwind_op,
        };
        use std::collections::BTreeSet;
        match node {
            Node::GraphNodeScan {
                binding, labels, ..
            } => {
                let binding = binding.clone();
                let labels = labels.clone();
                Ok(kernel("NodeScan", vec![], move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    let binding = &binding;
                    let labels = &labels;
                    node_scan(binding, labels, graph)
                }))
            }
            Node::GraphRelScan {
                binding,
                types,
                dir,
                ..
            } => {
                let binding = binding.clone();
                let types = types.clone();
                let dir = dir.clone();
                Ok(kernel("RelScan", vec![], move |mut inputs, state| {
                    let graph = &state.graph;
                    let ctx = &mut state.context;
                    let binding = &binding;
                    let types = &types;
                    let dir = &dir;
                    rel_scan(binding, types, *dir, graph)
                }))
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
            Node::GraphCreate {
                nodes,
                edges,
                input,
                ..
            } => {
                let nodes = nodes.clone();
                let edges = edges.clone();
                Ok(kernel(
                    "Create",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let graph = &state.graph;
                        let ctx = &mut state.context;
                        let nodes = &nodes;
                        let edges = &edges;
                        {
                            let rows = inputs.remove(0);
                            create_op(nodes, edges, rows, graph)
                        }
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
                            aggregate_op(group, aggs, rows, graph)
                        }
                    },
                ))
            }
            Node::GraphDistinct {
                keys, mode, input, ..
            } => {
                let keys = keys.clone();
                let mode = mode.clone();
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
                                distinct::distinct_op_with_seen(keys, *mode, rows, seen)
                            } else {
                                distinct_op(keys, *mode, rows)
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
                                    let key = row_signature(row);
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
                input,
                ..
            } => {
                let path = path.clone();
                let selector = selector.clone();
                let parts = parts.clone();
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
                            path_pattern_op(path, selector, parts, upstream, graph)
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
    execute_with_session(plan, graph, timeout, None).await.map_err(|e|e.to_string())
}

pub(crate) async fn execute_with_session(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    timeout: Option<std::time::Duration>,
    resources: Option<&super::dag::DagSession>,
) -> std::result::Result<(ReturnedBatches, super::dag::DagStats), QueryExecutionError> {
    crate::ir::jvm::validate_computer_plan(&plan.root)?;
    let local = graph.clone();
    let (rows, stats) = execute_rows_inner(plan, &local, JvmExecution::default(), timeout, resources, true).await?;
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
    let returned =
        crate::ir::runtime::output::finalize_return(&fields, form, rows, &local, &plan.policy)
            .map_err(QueryExecutionError::from_error)?;
    if !crate::ir::jvm::contains_computer(&plan.root) { graph.restore_execution_overlay(&local); }
    Ok((returned, stats))
}

fn has_mutating_branches(node: &Node) -> bool {
    let children = crate::ir::analysis::children(node);
    (children.len() > 1 && crate::ir::exec::contains_mutation(node))
        || children.into_iter().any(has_mutating_branches)
}
