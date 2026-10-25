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
    pub fn for_host(
        graph: PropertyGraph,
        runner: Arc<dyn SubplanRunner>,
        rule_context: &crate::ir::rules::RuleContext,
    ) -> Self {
        let mut state = Self {
            graph,
            frontier: vec![Row::new()],
            ..Default::default()
        };
        state.context.rule_context = rule_context.clone();
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

#[derive(Debug, Clone)]
pub struct CompileOptions {
    #[doc(hidden)]
    pub rule_context: crate::ir::rules::RuleContext,
    pub sql_islands: bool,
    pub language_functions: bool,
    pub native_values: bool,
    pub prune_return: bool,
}
impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            rule_context: Default::default(),
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
    let rule_context = &options.rule_context;
    let normalized = super::super::pipeline::normalize_graph(plan, rule_context)
        .map_err(QueryExecutionError::from_error)?;
    let plan = &normalized;
    stacker::maybe_grow(1024 * 1024, 64 * 1024 * 1024, || {
        let compiler = Compiler {
            rule_context: rule_context.clone(),
            graph,
            policy: plan.policy.clone(),
            sql: options.sql_islands
                && (!has_mutating_branches(&plan.root)
                    || rule_context.lowering_policy == policy::LoweringPolicy::SqlRequired)
                && !(graph.mapping.is_some() && graph.has_mutations()),
            language_functions: options.language_functions,
            native_values: options.native_values,
            live_graph_reads: crate::ir::analysis::contains_source_mutation(&plan.root) || graph.has_mutations(),
            snapshot_reads: if !graph.has_mutations() && plan.policy.language == crate::ir::policy::Language::Cypher {
                initial_snapshot_reads(&plan.root)
            } else { Default::default() },
            stateful_distinct: false,
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
    pub fn boundary_kind(&self) -> Option<policy::BoundaryKind> {
        self.boundary
    }
    /// Invoke the built-in executable, bypassing any override for this boundary.
    pub fn invoke_default(
        &self,
        inputs: Vec<Vec<Row>>,
        state: &mut KernelState,
    ) -> std::result::Result<Vec<Row>, QueryExecutionError> {
        execution::run_default(self, inputs, state).map_err(QueryExecutionError::from_error)
    }
    /// Encode a compiled boundary output for its next SQL island or kernel.
    pub fn encode_output(
        &self,
        rows: Vec<Row>,
        state: &KernelState,
    ) -> std::result::Result<RecordBatch, QueryExecutionError> {
        if let Some(output) = &self.relational_output {
            output.encode(&rows, &state.graph)
        } else {
            transport::encode_rows(rows).map_err(QueryExecutionError::from_error)
        }
    }
    pub fn encode_host_output(
        &self,
        rows: Vec<Row>,
        state: &KernelState,
    ) -> std::result::Result<RecordBatch, QueryExecutionError> {
        if self.relational_output.is_some() {
            self.encode_output(rows, state)
        } else {
            transport::encode_host_rows(rows).map_err(QueryExecutionError::from_error)
        }
    }
    /// Encode lazily while the host pulls, bounding expanded traverser bulk as
    /// well as physical rows. The reader owns all metadata and borrows no state.
    pub fn host_output_reader(
        &self,
        rows: Vec<Row>,
        state: &KernelState,
        size: usize,
    ) -> Box<dyn arrow::array::RecordBatchReader + Send> {
        let output = self.relational_output.clone();
        let schema = output.as_ref().map(|output| output.schema()).unwrap_or_else(transport::host_schema);
        let expands_bulk = output.as_ref().is_some_and(|output|
            !output.preserve_bulk && output.policy.language == crate::ir::policy::Language::Gremlin);
        let graph = state.graph.clone();
        let control = state.context.jvm.clone();
        let mut rows = rows.into_iter();
        let mut terminal = false;
        let batches = std::iter::from_fn(move || {
            if terminal || rows.len() == 0 { return None; }
            if let Err(error) = control.check() {
                terminal = true;
                return Some(Err(arrow::error::ArrowError::ExternalError(Box::new(
                    QueryExecutionError::from_error(error)))));
            }
            let batch = execution::take_output_rows(&mut rows, size.max(1), expands_bulk);
            let result = if let Some(output) = &output {
                output.encode(&batch, &graph)
                    .map_err(|error| arrow::error::ArrowError::ExternalError(Box::new(error)))
            } else {
                transport::encode_host_rows(batch)
                    .map_err(|error| arrow::error::ArrowError::ExternalError(Box::new(error)))
            };
            terminal = result.is_err();
            Some(result)
        });
        Box::new(arrow::array::RecordBatchIterator::new(batches, schema))
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
    /// Restore the declared SQL types before decoding host Arrow values.
    pub fn decode_relational_input(
        &self,
        batch: &RecordBatch,
        graph: &PropertyGraph,
    ) -> std::result::Result<Vec<Row>, QueryExecutionError> {
        let fields = self
            .relational_fields()
            .ok_or("Kernel has no relational input")?;
        let schema = Arc::new(self.inputs[0].schema().as_arrow().clone());
        // SQL must emit a cardinality column for a zero-column relation.
        // Its value is transport scaffolding; only the row count is logical.
        if schema.fields().is_empty() {
            let empty = RecordBatch::try_new_with_options(
                schema,
                vec![],
                &arrow::array::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
            )
            .map_err(|e| e.to_string())?;
            return super::super::native_values::decode_rows(&empty, fields, graph);
        }
        if schema.fields().len() != batch.num_columns() {
            return Err("SQL boundary column count changed".into());
        }
        let columns = batch
            .columns()
            .iter()
            .zip(schema.fields())
            .map(|(array, field)| super::super::dag::coerce_sql_array(array, field.data_type()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let batch = RecordBatch::try_new_with_options(
            schema,
            columns,
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
        )
        .map_err(|e| e.to_string())?;
        let mut rows = super::super::native_values::decode_rows(&batch, fields, graph)?;
        if self.relational_input_state {
            super::super::native_values::restore_traverser_state(&mut rows)?;
        }
        Ok(rows)
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
    pub fn host_output_reader(
        &self,
        rows: Vec<Row>,
        state: &KernelState,
        size: usize,
    ) -> Box<dyn arrow::array::RecordBatchReader + Send> {
        self.kernel.host_output_reader(rows, state, size)
    }
    /// Preserve the source's declared relational boundary schema on host pulls.
    pub fn encode_host_output(
        &self,
        rows: Vec<Row>,
        state: &KernelState,
    ) -> std::result::Result<RecordBatch, QueryExecutionError> {
        self.kernel.encode_host_output(rows, state)
    }

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

#[cfg(all(test, feature = "duckdb"))]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn host_arrow_output_splits_bulk_lazily_and_checks_cancellation() {
        let output = super::super::reentry::SqlOutput {
            fields: vec!["current".into()], nodes: vec![], edges: vec![],
            policy: crate::ir::policy::GraphPlanPolicy::gremlin(),
            preserve_bulk: false, transport_state: false,
            identity_type: arrow::datatypes::DataType::Int64,
            implicit_fields: vec![],
        };
        let LogicalPlan::Extension(extension) = kernel("BulkHostOutput", vec![], |_, _| Ok(vec![])) else { unreachable!() };
        let mut kernel = extension.node.as_any().downcast_ref::<RowKernel>().unwrap().clone();
        kernel.relational_output = Some(output);
        let state = KernelState::default();
        let row = Row { bindings: [("current".into(), Value::String("payload".into()))].into(), bulk: 33 };
        let reader = kernel.host_output_reader(vec![row.clone()], &state, 4);
        assert_eq!(reader.map(|batch| batch.unwrap().num_rows()).collect::<Vec<_>>(), vec![4,4,4,4,4,4,4,4,1]);
        let mut huge = row;
        huge.bulk = u64::MAX;
        let mut reader = kernel.host_output_reader(vec![huge], &state, 4);
        assert_eq!(reader.next().unwrap().unwrap().num_rows(), 4);
        state.context.jvm.cancelled.store(true, Ordering::Release);
        assert!(reader.next().unwrap().unwrap_err().to_string().contains("execution cancelled"));
        assert!(reader.next().is_none());
    }

    // Test-only Hybrid reference harness for kernel semantics and transaction
    // behavior. Production scheduling is SQL-required and tested separately.
    pub(crate) fn reference_context() -> crate::ir::rules::RuleContext {
        let mut context = crate::ir::rules::RuleContext::default();
        context.lowering_policy = policy::LoweringPolicy::Hybrid;
        context
    }
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
                rule_context: reference_context(),
                language_functions: true,
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
            let mut state =
                KernelState::for_host(graph.clone(), runner.clone(), &reference_context());
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
        let mut state = KernelState::for_host(graph, runner.clone(), &reference_context());
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
                    rule_context: reference_context(),
                    language_functions: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let runner = Arc::new(HostRunner::default());
            let mut state =
                KernelState::for_host(prepared.graph, runner.clone(), &reference_context());
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
            let mut state =
                KernelState::for_host(graph.clone(), runner.clone(), &reference_context());
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

#[cfg(all(test, feature = "duckdb"))]
#[tokio::test]
async fn repeat_dedup_retains_seen_state_while_ordinary_distinct_remains_sql() {
    let mut engine = crate::engine::GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("g.inject(1,2,2).dedup().count()", "2"),
        ("g.inject(1,2,2).repeat(__.dedup()).times(2).count()", "0"),
        ("g.inject(1,2,2).repeat(__.dedup()).times(1).count()", "2"),
    ] {
        let result = engine.gremlin(query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 1, "{query}");
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), expected, "{query}");
    }
}

#[cfg(all(test, feature = "duckdb"))]
#[tokio::test]
async fn both_endpoints_keep_properties_and_self_loop_multiplicity() {
    let mut engine = crate::engine::GraphEngine::in_memory().unwrap();
    engine.gremlin("g.addV('person').property('name','a').as('a').addV('person').property('name','b').as('b').addE('R').from('a').to('b')").await.unwrap();
    let result = engine.gremlin("g.withStrategies(new SubgraphStrategy(vertices: __.has('name',P.within('a','b')))).E().bothV().values('name').order()").await.unwrap();
    let names = (0..result.returned.batch.num_rows()).map(|row|
        arrow::util::display::array_value_to_string(result.returned.batch.column(0), row).unwrap()).collect::<Vec<_>>();
    assert_eq!(names, vec!["a", "b"]);
    engine.gremlin("g.V().has('name','a').as('a').addE('Loop').to('a')").await.unwrap();
    let result = engine.gremlin("g.E().hasLabel('Loop').bothV().count()").await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), "2");
}

#[cfg(all(test, feature = "duckdb"))]
#[tokio::test]
async fn mixed_injected_values_preserve_sql_union_carriers() {
    let mut engine = crate::engine::GraphEngine::in_memory().unwrap();
    let result = engine.gremlin("g.inject('a').concat(__.inject(['b','c']))").await.unwrap();
    assert_eq!(result.returned.batch.num_rows(), 1);
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), "aa");
}
