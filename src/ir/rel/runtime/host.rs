//! Host-facing execution contracts for the existing compiled graph kernels.
//! This module never plans or executes a DataFusion physical operator.
use super::*;
pub use datafusion::logical_expr::LogicalPlan;
use std::collections::BTreeSet;

/// One statement's mutable state, shared with every correlated invocation.
/// A compiled plan owns no live host pointer or execution context.
#[derive(Debug, Default)]
pub struct KernelState {
    pub graph: PropertyGraph,
    pub frontier: Vec<Row>,
    pub batch_key: Option<String>,
    pub(crate) context: ExecutionContext,
}
impl KernelState {
    pub fn for_host(graph: PropertyGraph, runner: Arc<dyn SubplanRunner>) -> Self {
        let mut state = Self {
            graph,
            frontier: vec![Row::new()],
            ..Default::default()
        };
        state.set_subplan_runner(runner);
        state
    }
    pub fn start_clocks(
        &self,
        statement_micros: i64,
        transaction_micros: i64,
    ) -> std::result::Result<(), QueryExecutionError> {
        let statement = chrono::DateTime::from_timestamp_micros(statement_micros)
            .ok_or("Invalid host statement timestamp")?;
        let transaction = chrono::DateTime::from_timestamp_micros(transaction_micros)
            .ok_or("Invalid host transaction timestamp")?;
        self.graph.begin_statement();
        self.graph.set_host_clocks(statement, transaction);
        Ok(())
    }
    pub fn prepare_results(&self, rows: &[Row]) -> std::result::Result<(), QueryExecutionError> {
        self.graph.prefetch_source(rows);
        self.graph.check_source().map_err(Into::into)
    }
    pub fn query_scope(&self) -> KernelQueryScope {
        KernelQueryScope(self.context.jvm.scope())
    }
    pub fn set_subplan_runner(&mut self, runner: Arc<dyn SubplanRunner>) {
        self.context.subplan_runner = Some(runner);
    }
    pub fn cancellation_token(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.context.jvm.cancelled.clone()
    }
    pub fn set_deadline(&mut self, deadline: Option<std::time::Instant>) {
        self.context.jvm.deadline = deadline;
    }
    pub fn check(&self) -> IrResult<()> {
        self.context.jvm.check()
    }
    pub fn query_cost(&self) -> &crate::ir::QueryCost {
        &self.context.query_cost
    }
}

/// Cleans up JVM algorithm/application callback resources on every exit path.
pub struct KernelQueryScope(crate::ir::jvm::JvmQueryScope);
impl KernelQueryScope {
    pub fn complete(&mut self) {
        self.0.complete();
    }
}

/// Execute an already compiled correlated body. Implementations schedule its
/// relational and kernel inputs; they must not interpret GraphIR or replace
/// statement state. The frontier may be empty, which differs from one empty row.
pub trait SubplanRunner: fmt::Debug + Send + Sync {
    fn run(&self, plan: &LogicalPlan, state: &mut KernelState) -> IrResult<Vec<Row>>;
}

#[derive(Debug, Clone, Copy)]
pub struct CompileOptions {
    pub sql_islands: bool,
    pub language_functions: bool,
    pub native_values: bool,
    pub prune_return: bool,
}
impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            sql_islands: true,
            language_functions: false,
            native_values: true,
            prune_return: true,
        }
    }
}

/// Compile using the same worklist, language kernels and effect fences as the
/// legacy backend. The returned graph contains logical SQL islands and callable
/// kernels only. Compilation performs no physical planning or storage effects.
pub fn compile_for_host(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    options: CompileOptions,
) -> std::result::Result<LogicalPlan, QueryExecutionError> {
    stacker::maybe_grow(128 * 1024, 64 * 1024 * 1024, || {
        let compiler = Compiler {
            graph,
            policy: plan.policy.clone(),
            sql: options.sql_islands
                && !has_mutating_branches(&plan.root)
                && !(graph.mapping.is_some() && graph.has_mutations()),
            language_functions: options.language_functions,
            native_values: options.native_values,
            islands: super::super::island_planner::IslandMemo::new(&plan.root),
            lowered: Default::default(),
        };
        let needed = if options.prune_return {
            match plan.root.as_ref() {
                Node::GraphReturn { fields, .. } => Some(fields.iter().cloned().collect()),
                _ => None,
            }
        } else {
            None
        };
        optimize::optimize(
            compiler
                .lower(&plan.root)
                .map_err(QueryExecutionError::from_error)?,
            needed.as_ref(),
        )
        .map_err(QueryExecutionError::from_error)
    })
}

impl RowKernel {
    pub fn from_plan(plan: &LogicalPlan) -> Option<&Self> {
        let LogicalPlan::Extension(extension) = plan else {
            return None;
        };
        extension.node.as_any().downcast_ref()
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn inputs(&self) -> &[LogicalPlan] {
        &self.inputs
    }
    pub fn streaming(&self) -> bool {
        self.streaming
    }
    pub fn is_source(&self) -> bool {
        self.source.is_some()
    }
    pub fn relational_fields(&self) -> Option<&[String]> {
        self.relational_input.as_deref()
    }
    /// Unknown means the operator may inspect implicit traverser state.
    pub fn required_bindings(&self) -> Option<&BTreeSet<String>> {
        self.contract
            .as_ref()
            .and_then(|contract| contract.reads.as_ref())
    }
    pub fn written_bindings(&self) -> Option<&BTreeSet<String>> {
        self.contract.as_ref().map(|contract| &contract.writes)
    }
    pub fn slice(&self) -> Option<&crate::ir::plan::Slice> {
        self.slice.as_ref()
    }
    /// Decode the boundary using the same binding/bulk/value contract as the
    /// original physical adapter, including relational island output columns.
    pub fn decode_input(
        &self,
        batch: RecordBatch,
    ) -> std::result::Result<Vec<Row>, QueryExecutionError> {
        execution::input_rows(batch, self).map_err(QueryExecutionError::from_error)
    }
    /// Invoke only this compiled operation. The host owns input scheduling.
    pub fn invoke(
        &self,
        mut inputs: Vec<Vec<Row>>,
        state: &mut KernelState,
    ) -> std::result::Result<Vec<Row>, QueryExecutionError> {
        // Hosts schedule a non-source slice as one ordered barrier. Reuse the
        // existing bulk-aware range operation once, never restart per batch.
        if let Some(slice) = &self.slice {
            if self.source.is_none() {
                if inputs.len() != 1 {
                    return Err("Slice requires one ordered input".into());
                }
                inputs[0] =
                    execution::take_range(std::mem::take(&mut inputs[0]), &mut slice.clone());
            }
        }
        execution::run(self, inputs, state).map_err(QueryExecutionError::from_error)
    }
    pub fn source_cursor(&self) -> Option<KernelSourceCursor> {
        self.source.clone().map(|source| KernelSourceCursor {
            source: execution::SourceCursor::new(source),
            kernel: self.clone(),
            slice: self.slice.clone(),
        })
    }
}

/// Streaming cursor for the compiler's existing values/node/edge sources.
/// A fresh cursor is required for each execution, never stored in bind data.
pub struct KernelSourceCursor {
    source: execution::SourceCursor,
    kernel: RowKernel,
    slice: Option<crate::ir::plan::Slice>,
}
impl KernelSourceCursor {
    pub fn next(
        &mut self,
        size: usize,
        state: &mut KernelState,
    ) -> std::result::Result<Option<Vec<Row>>, QueryExecutionError> {
        if size == 0 {
            return Err("Kernel source batch size must be positive".into());
        }
        if self
            .slice
            .as_ref()
            .is_some_and(|slice| slice.fetch == Some(0))
        {
            return Ok(None);
        }
        self.source
            .next(size, state)
            .and_then(|rows| {
                rows.map(|rows| {
                    let rows = execution::run(&self.kernel, vec![rows], state)?;
                    Ok(if let Some(slice) = &mut self.slice {
                        execution::take_range(rows, slice)
                    } else {
                        rows
                    })
                })
                .transpose()
            })
            .map_err(QueryExecutionError::from_error)
    }
}

pub fn encode_rows(rows: Vec<Row>) -> std::result::Result<RecordBatch, QueryExecutionError> {
    super::transport::encode_host_rows(rows).map_err(QueryExecutionError::from_error)
}
pub fn decode_rows(batch: &RecordBatch) -> std::result::Result<Vec<Row>, QueryExecutionError> {
    super::transport::decode_rows(batch).map_err(QueryExecutionError::from_error)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A minimal test physical adapter. It schedules compiled descriptors only;
    // it intentionally has no Tokio runtime or DataFusion physical planner.
    #[derive(Debug, Default)]
    pub(crate) struct HostRunner {
        calls: AtomicUsize,
    }
    impl SubplanRunner for HostRunner {
        fn run(&self, plan: &LogicalPlan, state: &mut KernelState) -> IrResult<Vec<Row>> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let kernel = RowKernel::from_plan(plan).expect("native compiled descriptor");
            if let Some(mut cursor) = kernel.source_cursor() {
                let mut output = Vec::new();
                while let Some(rows) = cursor
                    .next(2, state)
                    .map_err(|e| RuntimeError::Runtime(e.to_string()))?
                {
                    output.extend(rows);
                }
                return Ok(output);
            }
            let inputs = kernel
                .inputs()
                .iter()
                .map(|input| self.run(input, state))
                .collect::<IrResult<Vec<_>>>()?;
            kernel
                .invoke(inputs, state)
                .map_err(|e| RuntimeError::Runtime(e.to_string()))
        }
    }
    fn compile(query: &str, graph: &PropertyGraph) -> LogicalPlan {
        let traversal = crate::language::gremlin::parse_traversal(query).unwrap();
        let plan = crate::language::gremlin::GremlinPlanner::new()
            .plan(&traversal)
            .unwrap();
        compile_for_host(
            &plan,
            graph,
            CompileOptions {
                sql_islands: false,
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn injected_host_runs_nested_control_and_group_finalizers_without_datafusion() {
        let graph = PropertyGraph::new();
        let runner = Arc::new(HostRunner::default());
        for (query, expected) in [
            (
                "g.inject(1,2,3).coalesce(__.filter(__.is(2)),__.constant(9))",
                vec![9, 2, 9],
            ),
            (
                "g.inject(1).repeat(__.map(__.constant(2))).times(2)",
                vec![2],
            ),
        ] {
            let compiled = compile(query, &graph);
            let mut state = KernelState::for_host(graph.clone(), runner.clone());
            let rows = runner.run(&compiled, &mut state).unwrap();
            let values = rows
                .iter()
                .map(|row| match row.get("current") {
                    Value::Int(v) => v as i64,
                    Value::Long(v) => v,
                    other => panic!("unexpected output: {other:?}"),
                })
                .collect::<Vec<_>>();
            assert_eq!(values, expected, "{query}");
        }
        let compiled = compile(
            "g.inject('a','b','a').group().by(__.identity()).by(__.count())",
            &graph,
        );
        let mut state = KernelState::for_host(graph, runner.clone());
        let rows = runner.run(&compiled, &mut state).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rows[0].get("current"),
            Value::TypedMap(_) | Value::Map(_)
        ));
        assert!(runner.calls.load(Ordering::Relaxed) > 10);
    }
    #[test]
    fn nullable_optional_path_uses_existing_host_kernels() {
        for function in ["nodes", "relationships"] {
            let request = serde_json::from_value(serde_json::json!({
                "version": 1, "dialect": "duckdb", "language": "cypher",
                "query": format!("WITH null AS a OPTIONAL MATCH p = (a)-[r]->() RETURN {function}(p) AS path, {function}(null) AS literal"),
                "tables": [], "native_values": true
            })).unwrap();
            let prepared = crate::compiler::prepare_graph(&request).unwrap();
            let compiled = compile_for_host(
                &prepared.plan,
                &prepared.graph,
                CompileOptions {
                    sql_islands: false,
                    ..Default::default()
                },
            )
            .unwrap();
            let runner = Arc::new(HostRunner::default());
            let mut state = KernelState::for_host(prepared.graph, runner.clone());
            let rows = runner.run(&compiled, &mut state).unwrap();
            assert_eq!(rows.len(), 1, "{function}");
            assert!(matches!(rows[0].bindings.get("path"), Some(Value::Null)));
            assert!(matches!(rows[0].bindings.get("literal"), Some(Value::Null)));
        }
    }
    #[test]
    fn prepared_kernel_graph_gets_fresh_side_effects_and_bulk_per_execution() {
        let graph = PropertyGraph::new();
        let runner = Arc::new(HostRunner::default());
        let compiled = compile("g.inject(1,2).aggregate('seen').cap('seen')", &graph);
        let mut prior = None;
        for _ in 0..2 {
            let mut state = KernelState::for_host(graph.clone(), runner.clone());
            let rows = runner.run(&compiled, &mut state).unwrap();
            assert_eq!(rows.len(), 1);
            let encoded =
                crate::ir::catalog::snapshot::binary::encode_value_bytes(&rows[0].get("current"));
            if let Some(prior) = &prior {
                assert_eq!(prior, &encoded);
            }
            prior = Some(encoded);
        }
    }
}
