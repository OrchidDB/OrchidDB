//! One executable relational DAG. DuckDB regions are DataFusion physical sources;
//! all residual relational operators execute through DataFusion. No Graph IR plan
//! is retained by the executor and no graph interpreter fallback is available.
use arrow::array::RecordBatch;
use std::sync::{Arc, Mutex};
#[cfg(feature = "duckdb")]
use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
};

use super::{LoweredPlan, RelResult, sql};
use crate::ir::runtime::ReturnedBatches;
#[cfg(feature = "duckdb")]
use async_trait::async_trait;
use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
use datafusion::common::DataFusionError;
#[cfg(feature = "duckdb")]
use datafusion::common::{DFSchemaRef, Result};
use datafusion::execution::TaskContext;
#[cfg(feature = "duckdb")]
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::LogicalPlan;
#[cfg(feature = "duckdb")]
use datafusion::logical_expr::{Expr, Extension, UserDefinedLogicalNode, UserDefinedLogicalNodeCore};
#[cfg(feature = "duckdb")]
use datafusion::physical_expr::EquivalenceProperties;
#[cfg(feature = "duckdb")]
use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType};
#[cfg(feature = "duckdb")]
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::ExecutionPlan;
#[cfg(feature = "duckdb")]
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, Partitioning, PlanProperties, SendableRecordBatchStream,
};
use datafusion::physical_planner::{DefaultPhysicalPlanner, ExtensionPlanner, PhysicalPlanner};
use datafusion::prelude::{SessionConfig, SessionContext};
#[cfg(feature = "duckdb")]
use futures::stream;

#[derive(Debug, Clone, Default)]
pub struct DagStats {
    pub cost: crate::ir::QueryCost,
    pub duckdb_regions: usize,
    pub postgres_regions: usize,
    pub other_sql_regions: usize,
    pub datafusion_operators: usize,
    pub physical_plan: String,
    /// Optimized logical plan with physical table selections applied.
    pub logical_plan: String,
    pub constraint_proofs: Vec<super::constraints::RewriteProof>,
    pub layout_selections: Vec<super::layout::LayoutDecision>,
    pub representation_selections: Vec<super::representation::RepresentationDecision>,
    pub plan_estimates: Vec<super::statistics::PlanEstimate>,
    pub optimizer_decisions: Vec<super::statistics::OptimizerDecision>,
    pub sql_queries: Vec<String>,
    pub native_source_queries: Vec<String>,
    pub native_source_rows: usize,
}

/// Execution resources owned by one graph engine, never shared between engines.
/// SQL source tables are content-addressed by DuckDbExecutor; logical plans and
/// graph snapshots are rebuilt for every query.
pub(crate) struct DagSession {
    session: SessionContext,
    #[cfg(feature = "duckdb")]
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
    #[cfg(feature = "duckdb")]
    external: std::collections::BTreeSet<String>,
    optimize: bool,
    pub(crate) region_session: Option<sql::region::SharedRegionSession>,
}
impl DagSession {
    pub(crate) fn new(_timeout: Option<std::time::Duration>) -> Self {
        let session = super::optimizer::session(
            SessionConfig::new().with_target_partitions(1),
        );
        // Removing the last constant grouping key changes empty-input
        // semantics: a grouped aggregate has zero rows, a global one has one.
        let rules = session.state().optimizers().iter()
            .filter(|rule| rule.name() != "eliminate_group_by_constant")
            .cloned().collect();
        let state = datafusion::execution::session_state::SessionStateBuilder::new_from_existing(session.state())
            .with_optimizer_rules(rules).build();
        Self {
            #[cfg(feature = "duckdb")]
            external: Default::default(),
            optimize: true,
            region_session: None,
            session: SessionContext::new_with_state(state),
            #[cfg(feature = "duckdb")]
            executor: Arc::new(Mutex::new(
                _timeout
                    .map(sql::DuckDbExecutor::with_timeout)
                    .unwrap_or_default(),
            )),
        }
    }

    #[cfg(feature = "duckdb")]
    pub(crate) fn with_shared(executor: Arc<Mutex<sql::DuckDbExecutor>>, external: std::collections::BTreeSet<String>) -> Self {
        Self { session: SessionContext::new_with_config(SessionConfig::new().with_target_partitions(1)), executor, external, optimize: false, region_session: None }
    }

    #[cfg(feature = "duckdb")]
    pub(crate) fn shared_executor(&self) -> Arc<Mutex<sql::DuckDbExecutor>> { self.executor.clone() }

    #[cfg(feature = "duckdb")]
    pub(crate) fn executor(&self) -> std::result::Result<std::sync::MutexGuard<'_,sql::DuckDbExecutor>,String> {
        self.executor.lock().map_err(|_| "DuckDB executor poisoned".into())
    }


}

/// A planned DuckDB region has no DataFusion inputs: its native relational scans
/// and their Arrow sources are captured by PreparedSql. Its schema remains the
/// original relational schema, including column qualifiers used by consumers.
#[cfg(feature = "duckdb")]
#[derive(Clone)]
struct DuckDbRegion {
    id: usize,
    schema: DFSchemaRef,
    prepared: Arc<sql::PreparedSql>,
}
#[cfg(feature = "duckdb")]
impl fmt::Debug for DuckDbRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DuckDbRegion")
            .field("id", &self.id)
            .finish()
    }
}
#[cfg(feature = "duckdb")]
impl PartialEq for DuckDbRegion {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.prepared.query == other.prepared.query
            && self.schema == other.schema
    }
}
#[cfg(feature = "duckdb")]
impl Eq for DuckDbRegion {}
#[cfg(feature = "duckdb")]
impl Hash for DuckDbRegion {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.prepared.query.hash(state);
    }
}
#[cfg(feature = "duckdb")]
impl PartialOrd for DuckDbRegion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
#[cfg(feature = "duckdb")]
impl Ord for DuckDbRegion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.id, &self.prepared.query).cmp(&(other.id, &other.prepared.query))
    }
}
#[cfg(feature = "duckdb")]
impl UserDefinedLogicalNodeCore for DuckDbRegion {
    fn name(&self) -> &str {
        match self.prepared.dialect { sql::SqlDialect::DuckDb => "DuckDbRegion", sql::SqlDialect::Postgres => "PostgresRegion", sql::SqlDialect::Custom(_) => "SqlRegion" }
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        vec![]
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}Region(id={})", self.prepared.dialect.name(), self.id)
    }
    fn with_exprs_and_inputs(
        &self,
        expressions: Vec<Expr>,
        inputs: Vec<LogicalPlan>,
    ) -> Result<Self> {
        if !expressions.is_empty() || !inputs.is_empty() {
            return Err(DataFusionError::Plan("DuckDB region dependency mismatch".into()));
        }
        Ok(self.clone())
    }
}

/// Unknown or volatile UDFs and extension nodes execute only in DataFusion.
/// DuckDB eligibility is a planning decision; execution errors are never retried
/// through another engine, avoiding repeated effects or changed error semantics.
/// Analyze each logical node once. A rejected ancestor must not repeatedly
/// rescan its descendants while partitioning into smaller regions.
#[cfg(feature = "duckdb")]
#[derive(Default)]
struct SqlEligibility {
    reasons: std::collections::HashMap<usize, Option<&'static str>>,
    dialect: Option<sql::SqlDialect>,
}
#[cfg(feature = "duckdb")]
impl SqlEligibility {
    fn visit(&mut self, plan: &LogicalPlan) -> Option<&'static str> {
        let key = plan as *const LogicalPlan as usize;
        if let Some(reason) = self.reasons.get(&key) { return *reason; }
        // A complete SQL lowering consumes the node's expressions and inputs.
        // Requiring their scalar mappings again would reject index-provided
        // scores or engine operations that have no standalone scalar form.
        if matches!(plan, LogicalPlan::Extension(_)) && matches!(
            sql::lowering::lower_relation(plan, &sql::lowering::LoweringContext {
                dialect: self.dialect.unwrap_or(sql::SqlDialect::DuckDb),
                mode: sql::lowering::LoweringMode::InIsland,
            }), Ok(Some(sql::lowering::RelationLowering::Sql(_)))
        ) {
            self.reasons.insert(key, None);
            return None;
        }
        let mut reason = match plan {
            LogicalPlan::Extension(_) if matches!(sql::lowering::lower_relation(plan, &sql::lowering::LoweringContext {
                dialect: self.dialect.unwrap_or(sql::SqlDialect::DuckDb), mode: sql::lowering::LoweringMode::InIsland,
            }), Ok(Some(sql::lowering::RelationLowering::Sql(_) | sql::lowering::RelationLowering::Rewrite(_)))) => None,
            LogicalPlan::Extension(_) => Some("residual extension"),
            LogicalPlan::EmptyRelation(empty) if empty.produce_one_row => None,
            LogicalPlan::EmptyRelation(_) => Some("empty relation"),
            _ if plan.schema().fields().is_empty() => Some("zero-column relation"),
            _ => None,
        };
        if reason.is_some() {
            self.reasons.insert(key, reason);
            return reason;
        }
        let mut portable_schema = datafusion::common::DFSchema::empty();
        for input in plan.inputs() { portable_schema.merge(input.schema()); }
        portable_schema.merge(plan.schema());
        for expr in plan.expressions() {
            let _ = expr.apply(|expr| {
                if let Expr::Literal(value, _) = expr {
                    if let Some(why) = crate::ir::functions::portable::literal_issue(value, self.dialect.unwrap_or(sql::SqlDialect::DuckDb).name()) {
                        reason.get_or_insert(why);
                        return Ok(TreeNodeRecursion::Stop);
                    }
                }
                if let Expr::ScalarFunction(function) = expr {
                    if self.dialect == Some(sql::SqlDialect::Postgres) && matches!(function.func.name(), "__orchiddb_sparql_scalar" | "sha1" | "sha256" | "sha384" | "sha512") {
                        reason.get_or_insert("native RDF scalar kernel");
                        return Ok(TreeNodeRecursion::Stop);
                    }
                    if let Some(logical) = crate::ir::functions::logical::definition(&function.func) {
                        let dialect = self.dialect.unwrap_or(sql::SqlDialect::DuckDb);
                        let adapter_mapping = match dialect { sql::SqlDialect::Custom(adapter) => adapter.function_mapping(logical.logical_name()).is_some(), _ => false };
                        if !adapter_mapping && !logical.has_sql_mapping(dialect.name(), function.args.len()) {
                            reason.get_or_insert("logical function requires native execution");
                            return Ok(TreeNodeRecursion::Stop);
                        }
                        if !adapter_mapping && logical.portable_builtin {
                            if let Err(why) = crate::ir::functions::portable::validate_call(logical.logical_name(), &function.args, &portable_schema, dialect.name()) {
                                reason.get_or_insert(why);
                                return Ok(TreeNodeRecursion::Stop);
                            }
                        }
                        return Ok(TreeNodeRecursion::Continue);
                    }
                    let native = self.dialect.unwrap_or(sql::SqlDialect::DuckDb).supports_scalar_function(function.func.name())
                        || super::sparql::is_duck_function(&function.func)
                        || function.func.name().starts_with(crate::ir::functions::ENGINE_FUNCTION_PREFIX)
                        || function.func.name() == crate::ir::functions::ENGINE_CAST_FUNCTION;
                    if !native && (function.func.signature().volatility == datafusion::logical_expr::Volatility::Volatile
                        || function.func.documentation().is_none()) {
                        reason.get_or_insert("function capability/effect boundary");
                        return Ok(TreeNodeRecursion::Stop);
                    }
                }
                Ok(TreeNodeRecursion::Continue)
            });
        }
        if reason.is_none() {
            for input in plan.inputs() {
                reason = self.visit(input);
                if reason.is_some() { break; }
            }
        }
        self.reasons.insert(key, reason);
        reason
    }
}

#[cfg(feature = "duckdb")]
fn partition<'a>(
    plan: &'a LogicalPlan,
    stats: &'a mut DagStats,
    eligibility: &'a mut SqlEligibility,
    external: &'a std::collections::BTreeSet<String>,
    dialect: sql::SqlDialect,
    bind_inputs: bool,
    source_executor: &'a Arc<Mutex<sql::DuckDbExecutor>>,
) -> futures::future::BoxFuture<'a, Result<LogicalPlan>> {
    Box::pin(async move {
        let explain = std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some();
        let reason = eligibility.visit(plan);
        if explain && let Some(reason) = reason {
            eprintln!("DuckDB boundary: {reason}");
        }
        // A one-row, zero-column input can be absorbed into a parent SQL
        // projection, but cannot be emitted as a standalone SELECT * island.
        // Native scalar projections (including calls that fail to fold) need
        // that input to remain executable when they form a boundary.
        if reason.is_none() && !plan.schema().fields().is_empty() {
            // The SQL unparser requires an explicit select list for roots such
            // as joins. Unique aliases also handle duplicate qualified names.
            let projected = datafusion::logical_expr::LogicalPlanBuilder::from(plan.clone())
                .project(
                    plan.schema()
                        .columns()
                        .into_iter()
                        .enumerate()
                        .map(|(i, column)| Expr::Column(column).alias(format!("__region_{i}")))
                        .collect::<Vec<_>>(),
                )?
                .build()?;
            let candidate = LoweredPlan {
                plan: projected,
                fields: (0..plan.schema().fields().len())
                    .map(|i| format!("__region_{i}"))
                    .collect(),
                result_form: crate::ir::policy::ResultForm::RowSet,
                islands: Default::default(),
            };
            let empty = Default::default();
            let prepared = sql::prepare_with_external(&candidate, dialect, if bind_inputs { &empty } else { external }).await.and_then(|mut prepared| {
                if bind_inputs {
                    for table in &mut prepared.tables {
                        if external.contains(&table.name) {
                            let name = datafusion::common::TableReference::parse_str(&table.name);
                            let table_sql = name.catalog().into_iter().chain(name.schema()).chain(std::iter::once(name.table()))
                                .map(|part| sql::SqlDialect::DuckDb.quote_ident(part)).collect::<Vec<_>>().join(".");
                            let columns = table.schema.fields().iter().map(|f| sql::SqlDialect::DuckDb.quote_ident(f.name())).collect::<Vec<_>>().join(", ");
                            let mut source = source_executor.lock().map_err(|_| sql::SqlError::Execution("source session poisoned".into()))?;
                            let mut statement = source.connection()?.prepare(&format!("SELECT {columns} FROM {table_sql}"))
                                .map_err(|e| sql::SqlError::Execution(e.to_string()))?;
                            table.batches = statement.query_arrow([]).map_err(|e| sql::SqlError::Execution(e.to_string()))?.collect();
                        }
                    }
                    sql::region::bind_inputs(&mut prepared)?;
                }
                Ok(prepared)
            });
            if explain && let Err(error) = &prepared {
                eprintln!("DuckDB boundary: SQL preparation: {error}");
            }
            if let Ok(prepared) = prepared {
                if std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some() {
                    eprintln!("DuckDB candidate: {}", prepared.query);
                }
                let id = stats.duckdb_regions + stats.postgres_regions + stats.other_sql_regions;
                match dialect { sql::SqlDialect::DuckDb => stats.duckdb_regions += 1, sql::SqlDialect::Postgres => stats.postgres_regions += 1, sql::SqlDialect::Custom(_) => stats.other_sql_regions += 1 };
                stats.sql_queries.push(prepared.query.clone());
                return Ok(LogicalPlan::Extension(Extension {
                    node: Arc::new(DuckDbRegion {
                        id,
                        schema: plan.schema().clone(),
                        prepared: Arc::new(prepared),
                    }),
                }));
            }
        }
        use sql::lowering::{lower_relation, LoweringContext, LoweringMode, RelationLowering};
        match lower_relation(plan, &LoweringContext {dialect, mode:LoweringMode::InIsland})
            .map_err(|e| DataFusionError::Plan(e.to_string()))? {
            Some(RelationLowering::Dependent {source, template, schema}) => {
                if schema.as_ref() != plan.schema().as_ref() || template.dialect != dialect.name()
                    || template.parameters != source.schema().fields().len() {
                    return Err(DataFusionError::Plan("dependent relational lowering changed its execution contract".into()));
                }
                let source = partition(&source, stats, eligibility, external, dialect, bind_inputs, source_executor).await?;
                match dialect {
                    sql::SqlDialect::DuckDb => stats.duckdb_regions += 1,
                    sql::SqlDialect::Postgres => stats.postgres_regions += 1,
                    sql::SqlDialect::Custom(_) => stats.other_sql_regions += 1,
                }
                stats.sql_queries.push(template.sql.clone());
                return Ok(super::dependent::DependentOperation {source:Arc::new(source), template, schema}.into_plan());
            }
            Some(RelationLowering::Rewrite(rewritten)) => {
                if &rewritten == plan { return Err(DataFusionError::Plan("relational rewrite made no progress".into())); }
                return partition(&rewritten, stats, &mut SqlEligibility {dialect:Some(dialect), ..Default::default()}, external, dialect, bind_inputs, source_executor).await;
            }
            Some(RelationLowering::Sql(_)) => return Err(DataFusionError::Plan("required relational operation could not form a SQL island".into())),
            None => {},
        }
        let mut inputs = Vec::new();
        for input in plan.inputs() {
            inputs.push(partition(input, stats, eligibility,external,dialect,bind_inputs,source_executor).await?);
        }
        stats.datafusion_operators += 1;
        // DataFusion 53 reports UNNEST execution columns as expressions, but
        // its with_new_exprs constructor expects an empty expression list.
        // Partitioning replaces only children, keeping their schemas intact.
        if let LogicalPlan::Unnest(unnest) = plan {
            let mut unnest = unnest.clone();
            unnest.input = Arc::new(inputs.remove(0));
            Ok(LogicalPlan::Unnest(unnest))
        } else {
            plan.with_new_exprs(plan.expressions(), inputs)
        }
    })
}

#[cfg(feature = "duckdb")]
#[derive(Debug)]
struct RegionPlanner {
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
    region_session: Option<sql::region::SharedRegionSession>,
    cost: Arc<Mutex<crate::ir::QueryCost>>,
}
#[cfg(feature = "duckdb")]
#[async_trait]
impl ExtensionPlanner for RegionPlanner {
    async fn plan_extension(
        &self,
        _planner: &dyn PhysicalPlanner,
        node: &dyn UserDefinedLogicalNode,
        _logical_inputs: &[&LogicalPlan],
        _physical_inputs: &[Arc<dyn ExecutionPlan>],
        _state: &SessionState,
    ) -> Result<Option<Arc<dyn ExecutionPlan>>> {
        if let Some(operation) = node.as_any().downcast_ref::<super::dependent::DependentOperation>() {
            let session = self.region_session.clone().unwrap_or_else(|| Arc::new(Mutex::new(
                Box::new(EmbeddedRegionSession(self.executor.clone())) as Box<dyn sql::region::RegionSession>
            )));
            return Ok(Some(Arc::new(super::dependent::exec::DependentOperationExec::new(
                operation, _physical_inputs[0].clone(), session, self.cost.clone(),
            ))));
        }
        let Some(region) = node.as_any().downcast_ref::<DuckDbRegion>() else {
            return Ok(None);
        };
        let schema = Arc::new(region.schema.as_arrow().clone());
        let properties = Arc::new(PlanProperties::new(
            EquivalenceProperties::new(schema),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Final,
            Boundedness::Bounded,
        ));
        Ok(Some(Arc::new(DuckDbExec {
            prepared: region.prepared.clone(),
            executor: self.executor.clone(),
            region_session: self.region_session.clone(),
            cost: self.cost.clone(),
            properties,
        })))
    }
}

#[cfg(feature = "duckdb")]
#[derive(Debug)]
struct DuckDbExec {
    region_session: Option<sql::region::SharedRegionSession>,
    cost: Arc<Mutex<crate::ir::QueryCost>>,
    prepared: Arc<sql::PreparedSql>,
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
    properties: Arc<PlanProperties>,
}
#[cfg(feature = "duckdb")]
impl DisplayAs for DuckDbExec {
    fn fmt_as(&self, _format: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}Exec", self.prepared.dialect.name())
    }
}
#[cfg(feature = "duckdb")]
impl ExecutionPlan for DuckDbExec {
    fn name(&self) -> &str {
        match self.prepared.dialect { sql::SqlDialect::DuckDb => "DuckDbExec", sql::SqlDialect::Postgres => "PostgresExec", sql::SqlDialect::Custom(_) => "SqlExec" }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![]
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if !children.is_empty() {
            return Err(DataFusionError::Plan("DuckDbExec dependency mismatch".into()));
        }
        Ok(self)
    }
    fn execute(
        &self,
        partition: usize,
        _context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(
                "DuckDB region has one partition".into(),
            ));
        }
        let executor = self.executor.clone();
        let region_session = self.region_session.clone();
        let prepared = self.prepared.clone();
        let cost = self.cost.clone();
        let schema = self.schema();
        let expected = schema.clone();
        let work = async move {
            tokio::task::spawn_blocking(move || {
                let returned = if let Some(session) = region_session {
                    session.lock().map_err(|_| DataFusionError::Execution("SQL region session poisoned".into()))?
                        .query(&prepared.query, prepared.output_schema())
                        .map_err(|e| DataFusionError::Execution(e.to_string()))?
                } else {
                let mut executor = executor
                    .lock()
                    .map_err(|_| DataFusionError::Execution("DuckDB executor poisoned".into()))?;
                let returned = stacker::maybe_grow(8 * 1024 * 1024,64 * 1024 * 1024,
                    || executor.execute_prepared_arrow(&prepared))
                    .map_err(|e| DataFusionError::Execution(e.to_string()))?;
                returned
                };
                {
                    let mut cost = cost.lock().map_err(|_| DataFusionError::Execution("Query cost poisoned".into()))?;
                    cost.sql_executions = cost.sql_executions.saturating_add(1);
                    cost.sql_output_rows = cost.sql_output_rows.saturating_add(returned.num_rows() as u64);
                    cost.sql_output_bytes = cost.sql_output_bytes.saturating_add(returned.get_array_memory_size() as u64);
                }
                let arrays = returned.columns().iter().zip(expected.fields())
                    .map(|(array, field)| coerce_sql_array(array, field.data_type()))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                RecordBatch::try_new(expected, arrays).map_err(DataFusionError::from)
            })
            .await
            .map_err(|e| DataFusionError::Execution(e.to_string()))?
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            schema,
            stream::once(work),
        )))
    }
}

/// Execute the lowered relational DAG. DataFusion is the scheduler and executor;
/// eligible DuckDB regions are explicit physical nodes within that same plan.
pub async fn execute(lowered: LoweredPlan) -> RelResult<(ReturnedBatches, DagStats)> {
    execute_with_extensions(lowered, vec![], None, None).await
}

/// Prepare nested and top-level regions with the same island placement rules.
pub(crate) async fn prepare_with_extensions(
    logical: &LogicalPlan,
    extensions: Vec<Arc<dyn ExtensionPlanner + Send + Sync>>,
    resources: &DagSession,
    _cost: Arc<Mutex<crate::ir::QueryCost>>,
) -> RelResult<(Arc<dyn ExecutionPlan>, Arc<TaskContext>, DagStats)> {
    let session = &resources.session;
    // Optimize while relational scans and expressions remain visible, before
    // DuckDB placement turns each region into an opaque physical source.
    // One state snapshot per query keeps execution time and function metadata
    // consistent across logical and physical planning without repeated clones.
    let query_state = session.state();
    let (initial, mut proofs) = super::constraints::optimize(logical.clone())?;
    let optimized = if resources.optimize { query_state.optimize(&initial)? } else {
        // Mapped SQL placement skips optional optimizer rewrites, but residual
        // DataFusion operators still require type coercion and function analysis.
        query_state.analyzer().execute_and_check(initial, query_state.config_options(), |_, _| {})?
    };
    let optimized = if resources.region_session.is_some() && !resources.optimize {
        datafusion::optimizer::Optimizer::with_rules(vec![
            Arc::new(datafusion::optimizer::simplify_expressions::SimplifyExpressions::new()),
            Arc::new(datafusion::optimizer::replace_distinct_aggregate::ReplaceDistinctWithAggregate::new()),
        ])
            .optimize(optimized, &datafusion::optimizer::OptimizerContext::new(), |_, _| {})?
    } else { optimized };
    let mut has_search=false;
    optimized.apply_with_subqueries(|p| {has_search |= super::search::node(p).is_some(); Ok(TreeNodeRecursion::Continue)})?;
    let optimized=if has_search && !resources.optimize {
        datafusion::optimizer::Optimizer::with_rules(vec![
            Arc::new(datafusion::optimizer::simplify_expressions::SimplifyExpressions::new()),
            Arc::new(datafusion::optimizer::push_down_filter::PushDownFilter::new()),
            Arc::new(datafusion::optimizer::optimize_projections::OptimizeProjections::new()),
        ]).optimize(optimized,&datafusion::optimizer::OptimizerContext::new(),|_,_|{})?
    } else {optimized};
    let optimized=super::search::bind_seeds(optimized)?;
    let (optimized, more) = super::constraints::optimize(optimized)?;
    proofs.extend(more);
    let selected = super::representation::select(optimized)?;
    let (optimized, mut optimizer_decisions) = super::statistics::optimize(selected.plan)?;
    optimizer_decisions.extend(selected.access_decisions);
    let optimized=super::search::push_source_filters(optimized)?;
    let mut stats = DagStats { optimizer_decisions, plan_estimates: super::statistics::explain(&optimized), constraint_proofs: proofs, layout_selections: selected.layout_selections, representation_selections: selected.representation_selections, logical_plan: optimized.display_indent().to_string(), ..Default::default() };
    #[cfg(feature = "duckdb")]
    let plan = {
        let mut eligibility = SqlEligibility::default();
        let dialect = match &resources.region_session {
            Some(session) => session.lock().map_err(|_| DataFusionError::Execution("SQL region session poisoned".into()))?.dialect(),
            None => sql::SqlDialect::DuckDb,
        };
        eligibility.dialect = Some(dialect);
        partition(&optimized, &mut stats, &mut eligibility,&resources.external,dialect,resources.region_session.is_some(),&resources.executor).await?
    };
    #[cfg(not(feature = "duckdb"))]
    let plan = {
        let _ = optimized.apply(|_| {
            stats.datafusion_operators += 1;
            Ok(TreeNodeRecursion::Continue)
        });
        super::dependent::native(super::search::native(optimized.clone())?)?
    };

    #[cfg(feature = "duckdb")]
    let extensions = {
        let mut extensions = extensions;
        extensions.push(Arc::new(RegionPlanner {
            executor: resources.executor.clone(),
            region_session: resources.region_session.clone(),
            cost: _cost,
        }));
        extensions
    };
    let planner = DefaultPhysicalPlanner::with_extension_planners(extensions);
    let physical = planner
        .create_physical_plan(&plan, &query_state)
        .await?;
    stats.physical_plan = datafusion::physical_plan::displayable(physical.as_ref())
        .indent(true)
        .to_string();
    Ok((physical, Arc::new(TaskContext::from(&query_state)), stats))
}

pub(crate) async fn execute_with_extensions(
    lowered: LoweredPlan,
    extensions: Vec<Arc<dyn ExtensionPlanner + Send + Sync>>,
    timeout: Option<std::time::Duration>,
    resources: Option<&DagSession>,
) -> RelResult<(ReturnedBatches, DagStats)> {
    let started = std::time::Instant::now();
    let owned;
    let resources = match resources {
        Some(resources) => resources,
        None => {
            owned = DagSession::new(timeout);
            &owned
        }
    };
    let cost = Arc::new(Mutex::new(crate::ir::QueryCost::default()));
    let (physical, task, mut stats) = prepare_with_extensions(&lowered.plan, extensions, resources, cost.clone()).await?;
    let prepared_at = std::time::Instant::now();
    let planned_at = std::time::Instant::now();
    let schema = physical.schema();
    let mut batches = datafusion::physical_plan::collect(physical, task).await?;
    let batch = if batches.len() == 1 {
        // SQL regions and fused residual pipelines normally return one batch.
        // Preserve its buffers while applying the physical output schema.
        let batch = batches.pop().unwrap();
        if batch.schema() == schema {
            batch
        } else {
            RecordBatch::try_new_with_options(schema, batch.columns().to_vec(),
                &arrow::array::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())))?
        }
    } else {
        arrow_select::concat::concat_batches(&schema, batches.iter())?
    };
    stats.cost = cost.lock().map_err(|_| DataFusionError::Execution("Query cost poisoned".into()))?.clone();
    stats.cost.result_rows = batch.num_rows() as u64;
    stats.cost.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    if std::env::var_os("ORCHIDDB_PROFILE_DAG").is_some() {
        eprintln!(
            "dag-profile {}",
            serde_json::json!({
                "prepare_ms": (prepared_at-started).as_secs_f64()*1000.0,
                "physical_plan_ms": (planned_at-prepared_at).as_secs_f64()*1000.0,
                "execute_ms": planned_at.elapsed().as_secs_f64()*1000.0,
                "regions":stats.duckdb_regions + stats.postgres_regions + stats.other_sql_regions,"duckdb_regions":stats.duckdb_regions,"postgres_regions":stats.postgres_regions,"residuals":stats.datafusion_operators,
            })
        );
    }
    Ok((
        ReturnedBatches {
            fields: lowered.fields,
            result_form: lowered.result_form,
            batch,
        },
        stats,
    ))
}

/// DuckDB assigns a physical integer type to untyped NULLs, including inside
/// lists. Restore the logical Arrow type recursively without losing validity
/// masks or offsets, and reject a non-null value where Null was expected.
pub(crate) fn coerce_sql_array(array: &arrow::array::ArrayRef, target: &arrow::datatypes::DataType) -> std::result::Result<arrow::array::ArrayRef, arrow::error::ArrowError> {
    use arrow::array::{Array, make_array, new_null_array};
    use arrow::datatypes::DataType;
    if array.data_type() == target { return Ok(array.clone()); }
    if crate::ir::functions::domain::descriptor(target).is_some() {
        return crate::ir::functions::domain::restore(array, target)
            .map_err(|e| arrow::error::ArrowError::CastError(e.to_string()));
    }
    if target == &DataType::Null && array.null_count() == array.len() {
        return Ok(new_null_array(target, array.len()));
    }
    let children = match (array.data_type(), target) {
        (DataType::List(_), DataType::List(field)) |
        (DataType::LargeList(_), DataType::LargeList(field)) => Some(vec![field.data_type()]),
        (DataType::FixedSizeList(_, a), DataType::FixedSizeList(field, b)) if a == b => Some(vec![field.data_type()]),
        (DataType::Struct(a), DataType::Struct(b)) if a.len() == b.len() => Some(b.iter().map(|f| f.data_type()).collect()),
        _ => None,
    };
    if let Some(children) = children {
        let data = array.to_data();
        let converted = data.child_data().iter().zip(children)
            .map(|(child, target)| coerce_sql_array(&make_array(child.clone()), target).map(|a| a.to_data()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return Ok(make_array(data.into_builder().data_type(target.clone()).child_data(converted).build()?));
    }
    arrow::compute::cast_with_options(array, target, &arrow::compute::CastOptions { safe: false, ..Default::default() })
}

impl DagStats {
    pub(crate) fn merge_execution(&mut self, nested: &Self) {
        self.cost.add_work(&nested.cost);
        self.constraint_proofs.extend(nested.constraint_proofs.iter().cloned());
        self.layout_selections.extend(nested.layout_selections.iter().cloned());
        self.representation_selections.extend(nested.representation_selections.iter().cloned());
        self.plan_estimates.extend(nested.plan_estimates.iter().cloned());
        self.optimizer_decisions.extend(nested.optimizer_decisions.iter().cloned());
        if !nested.logical_plan.is_empty() {
            self.logical_plan.push_str("\nExecuted subplan:\n");
            self.logical_plan.push_str(&nested.logical_plan);
        }
        self.duckdb_regions += nested.duckdb_regions;
        self.postgres_regions += nested.postgres_regions;
        self.other_sql_regions += nested.other_sql_regions;
        self.datafusion_operators += nested.datafusion_operators;
        self.sql_queries.extend(nested.sql_queries.iter().cloned());
        if !nested.physical_plan.is_empty() && !self.physical_plan.contains(&nested.physical_plan) {
            self.physical_plan.push_str("\nExecuted subplan:\n");
            self.physical_plan.push_str(&nested.physical_plan);
        }
    }
}

#[cfg(feature = "duckdb")]
#[derive(Debug)]
struct EmbeddedRegionSession(Arc<Mutex<sql::DuckDbExecutor>>);
#[cfg(feature = "duckdb")]
impl sql::region::RegionSession for EmbeddedRegionSession {
    fn dialect(&self) -> sql::SqlDialect {sql::SqlDialect::DuckDb}
    fn query(&mut self, query:&str, schema:arrow::datatypes::SchemaRef) -> sql::SqlResult<RecordBatch> {
        self.0.lock().map_err(|_| sql::SqlError::Execution("embedded SQL session poisoned".into()))?
            .execute_prepared_arrow(&sql::PreparedSql::query_only(query.into(), schema))
    }
}
