//! Orchid-owned execution DAG: SQL regions and explicit language boundaries.
//! DataFusion is used for logical analysis/optimization only.
use arrow::array::RecordBatch;
use std::sync::{Arc, Mutex};
use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
};

use super::engine_exec::{self, ExecutionPlan, SendableRecordBatchStream, TaskContext};
use super::engine_exec::{PlanProperties, RecordBatchStreamAdapter};
use super::{LoweredPlan, RelError, RelResult, sql};
use crate::ir::runtime::ReturnedBatches;
use datafusion::common::DataFusionError;
use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
use datafusion::common::{DFSchemaRef, Result};
use datafusion::logical_expr::LogicalPlan;
use datafusion::logical_expr::{Expr, Extension, UserDefinedLogicalNodeCore};
use futures::stream;

mod exchange;
#[cfg(all(test, feature = "duckdb"))]
mod scalar_inputs;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SqlBoundary {
    pub operator: String,
    pub engine: String,
    pub category: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct DagStats {
    pub rule_trace: Vec<crate::ir::rules::RuleTrace>,
    pub cost: crate::ir::QueryCost,
    pub duckdb_regions: usize,
    pub postgres_regions: usize,
    pub other_sql_regions: usize,
    /// Compatibility field: DataFusion never executes operators. Always zero.
    pub datafusion_operators: usize,
    pub boundary_operators: usize,
    pub output_decode_operators: usize,
    pub sql_boundaries: Vec<SqlBoundary>,
    pub runtime_inputs: Vec<sql::TableData>,
    next_runtime_input: usize,
    pub physical_plan: String,
    /// Optimized logical plan with physical table selections applied.
    pub logical_plan: String,
    pub constraint_proofs: Vec<super::constraints::RewriteProof>,
    pub layout_selections: Vec<super::layout::LayoutDecision>,
    pub representation_selections: Vec<super::representation::RepresentationDecision>,
    pub plan_estimates: Vec<super::statistics::PlanEstimate>,
    pub optimizer_decisions: Vec<super::statistics::OptimizerDecision>,
    pub sql_queries: Vec<String>,
    pub write_queries: Vec<super::host::WriteStatement>,
    pub native_source_queries: Vec<String>,
    pub native_source_rows: usize,
}

/// Execution resources owned by one graph engine, never shared between engines.
/// SQL source tables are content-addressed by DuckDbExecutor; logical plans and
/// graph snapshots are rebuilt for every query.
pub(crate) struct DagSession {
    rule_template: crate::ir::rules::RuleContext,
    session: super::optimizer::CompilerSession,
    #[cfg(feature = "duckdb")]
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
    external: std::collections::BTreeSet<String>,
    optimize: bool,
    pub(crate) region_session: Option<sql::region::SharedRegionSession>,
}
impl DagSession {
    pub(crate) fn set_cypher_aggregate_policy(
        &mut self,
        policy: crate::ir::rules::CypherAggregatePolicy,
    ) {
        self.rule_template.profile.cypher_aggregates = policy;
    }
    pub(crate) fn set_lowering_policy(&mut self, _policy: super::runtime::policy::LoweringPolicy) {
        self.rule_template.lowering_policy = super::runtime::policy::LoweringPolicy::SqlRequired;
    }
    pub(crate) fn set_boundary_executables(
        &mut self,
        executables: super::runtime::policy::ExecutableSet,
    ) {
        self.rule_template.executables = executables;
    }
    pub(crate) fn rule_context(&self) -> RelResult<crate::ir::rules::RuleContext> {
        let mut context = self.rule_template.new_query();
        #[cfg(feature = "duckdb")]
        if self.region_session.is_none() {
            let mut executor = self
                .executor
                .lock()
                .map_err(|_| DataFusionError::Execution("SQL session poisoned".into()))?;
            context.capabilities.language_functions_enabled = executor.language_functions_enabled();
            context.capabilities.float_helper = executor
                .prepare_float_helper_capability()
                .map_err(|e| RelError::Unsupported(e.to_string()))?;
        }
        Ok(context)
    }

    pub(crate) fn recreated(&self, timeout: Option<std::time::Duration>) -> Self {
        let mut next = Self::new(timeout);
        next.rule_template = self.rule_template.clone();
        next.region_session = self.region_session.clone();
        next
    }
    pub(crate) fn set_rule_context(&mut self, context: crate::ir::rules::RuleContext) {
        self.rule_template = context;
    }
    pub(crate) fn language_functions_enabled(&self) -> bool {
        #[cfg(feature = "duckdb")]
        {
            self.region_session.is_none()
                && self
                    .executor
                    .lock()
                    .is_ok_and(|e| e.language_functions_enabled())
        }
        #[cfg(not(feature = "duckdb"))]
        {
            false
        }
    }
    pub(crate) fn new(_timeout: Option<std::time::Duration>) -> Self {
        // A grouped empty input must not become a global aggregate when the
        // final constant grouping key is eliminated.
        let session = super::optimizer::session().without_rule("eliminate_group_by_constant");
        Self {
            rule_template: Default::default(),
            external: Default::default(),
            optimize: true,
            region_session: None,
            session,
            #[cfg(feature = "duckdb")]
            executor: Arc::new(Mutex::new(
                _timeout
                    .map(sql::DuckDbExecutor::with_timeout)
                    .unwrap_or_default(),
            )),
        }
    }

    #[cfg(feature = "duckdb")]
    pub(crate) fn with_shared(
        executor: Arc<Mutex<sql::DuckDbExecutor>>,
        external: std::collections::BTreeSet<String>,
    ) -> Self {
        Self {
            rule_template: Default::default(),
            session: super::optimizer::session(),
            executor,
            external,
            optimize: false,
            region_session: None,
        }
    }

    #[cfg(feature = "duckdb")]
    pub(crate) fn shared_executor(&self) -> Arc<Mutex<sql::DuckDbExecutor>> {
        self.executor.clone()
    }

    #[cfg(feature = "duckdb")]
    pub(crate) fn executor(
        &self,
    ) -> std::result::Result<std::sync::MutexGuard<'_, sql::DuckDbExecutor>, String> {
        self.executor
            .lock()
            .map_err(|_| "DuckDB executor poisoned".into())
    }
}

#[derive(Debug, Clone)]
struct SourceTable {
    name: String,
    schema: arrow::datatypes::SchemaRef,
    uses: usize,
}

/// A planned DuckDB region has no DataFusion inputs: its native relational scans
/// and their Arrow sources are captured by PreparedSql. Its schema remains the
/// original relational schema, including column qualifiers used by consumers.
#[derive(Clone)]
struct DuckDbRegion {
    id: usize,
    schema: DFSchemaRef,
    prepared: Arc<sql::PreparedSql>,
    inputs: Vec<LogicalPlan>,
    input_tables: Vec<sql::TableData>,
    source_tables: Vec<SourceTable>,
}
impl fmt::Debug for DuckDbRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DuckDbRegion")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for DuckDbRegion {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.prepared.query == other.prepared.query
            && self.schema == other.schema
            && self.inputs == other.inputs
    }
}
impl Eq for DuckDbRegion {}
impl Hash for DuckDbRegion {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.prepared.query.hash(state);
    }
}
impl PartialOrd for DuckDbRegion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DuckDbRegion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.id, &self.prepared.query).cmp(&(other.id, &other.prepared.query))
    }
}
impl UserDefinedLogicalNodeCore for DuckDbRegion {
    fn name(&self) -> &str {
        match self.prepared.dialect {
            sql::SqlDialect::DuckDb => "DuckDbRegion",
            sql::SqlDialect::Postgres => "PostgresRegion",
            sql::SqlDialect::Custom(_) => "SqlRegion",
        }
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
    fn fmt_for_explain(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}Region(id={})", self.prepared.dialect.name(), self.id)
    }
    fn with_exprs_and_inputs(
        &self,
        expressions: Vec<Expr>,
        inputs: Vec<LogicalPlan>,
    ) -> Result<Self> {
        if !expressions.is_empty() || inputs.len() != self.inputs.len() {
            return Err(DataFusionError::Plan(
                "DuckDB region dependency mismatch".into(),
            ));
        }
        let mut updated = self.clone();
        updated.inputs = inputs;
        Ok(updated)
    }
}

use super::sql::eligibility::SqlEligibility;

fn partition<'a>(
    plan: &'a LogicalPlan,
    stats: &'a mut DagStats,
    eligibility: &'a mut SqlEligibility,
    external: &'a std::collections::BTreeSet<String>,
    dialect: sql::SqlDialect,
    rule_context: &'a crate::ir::rules::RuleContext,
) -> futures::future::BoxFuture<'a, Result<LogicalPlan>> {
    Box::pin(async move {
        let explain = std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some();
        let reason = eligibility.visit(plan);
        let mut emission_error = None;
        if explain && let Some(reason) = reason {
            eprintln!("DuckDB boundary: {reason}");
        }
        let cut = if reason.is_some() {
            exchange::cut_inputs(plan, eligibility, &mut stats.next_runtime_input, external)?
        } else {
            None
        };
        let (sql_plan, bindings) = cut.unwrap_or_else(|| (plan.clone(), vec![]));
        let mut candidate_external = external.clone();
        candidate_external.extend(bindings.iter().map(|(_, table)| table.name.clone()));
        if reason.is_none() || !bindings.is_empty() {
            // The SQL unparser requires an explicit select list for roots such
            // as joins. Unique aliases also handle duplicate qualified names.
            let columns = if plan.schema().fields().is_empty() {
                vec![datafusion::logical_expr::lit(1_i64).alias("__region_0")]
            } else {
                plan.schema()
                        .columns()
                        .into_iter()
                        .enumerate()
                        .map(|(i, column)| Expr::Column(column).alias(format!("__region_{i}")))
                        .collect::<Vec<_>>()
            };
            let projected = datafusion::logical_expr::LogicalPlanBuilder::from(sql_plan)
                .project(columns)?
                .build()?;
            let candidate = LoweredPlan {
                rule_context: rule_context.clone(),
                plan: projected,
                fields: (0..plan.schema().fields().len().max(1))
                    .map(|i| format!("__region_{i}"))
                    .collect(),
                result_form: crate::ir::policy::ResultForm::RowSet,
                islands: Default::default(),
            };
            let prepared =
                sql::prepare_with_external(&candidate, dialect, &candidate_external).await;
            if let Err(error) = &prepared {
                emission_error = Some(error.to_string());
            }
            if explain && let Err(error) = &prepared {
                eprintln!("DuckDB boundary: SQL preparation: {error}");
            }
            if let Err(sql::SqlError::RulePipeline(error)) = &prepared {
                return Err(DataFusionError::Plan(error.to_string()));
            }
            if let Ok(prepared) = prepared {
                let mut dependencies = Vec::new();
                let mut input_tables = Vec::new();
                for (input, table) in bindings {
                    let mut child_eligibility = SqlEligibility {
                        dialect: Some(dialect),
                        language_functions: eligibility.language_functions,
                        ..Default::default()
                    };
                    dependencies.push(
                        partition(
                            &input,
                            stats,
                            &mut child_eligibility,
                            external,
                            dialect,
                            rule_context,
                        )
                        .await?,
                    );
                    stats.runtime_inputs.push(table.clone());
                    input_tables.push(table);
                }
                if std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some() {
                    eprintln!("DuckDB candidate: {}", prepared.query);
                }
                let id = stats.duckdb_regions + stats.postgres_regions + stats.other_sql_regions;
                match dialect {
                    sql::SqlDialect::DuckDb => stats.duckdb_regions += 1,
                    sql::SqlDialect::Postgres => stats.postgres_regions += 1,
                    sql::SqlDialect::Custom(_) => stats.other_sql_regions += 1,
                };
                let mut source_tables = std::collections::BTreeMap::new();
                candidate.plan.apply_with_subqueries(|node| {
                    if let LogicalPlan::TableScan(scan) = node {
                        let name = scan.table_name.to_string();
                        if external.contains(&name) || external.contains(scan.table_name.table()) {
                            source_tables
                                .entry(name.clone())
                                .and_modify(|t: &mut SourceTable| t.uses += 1)
                                .or_insert_with(|| SourceTable {
                                    name,
                                    schema: scan.source.schema(),
                                    uses: 1,
                                });
                        }
                    }
                    Ok(TreeNodeRecursion::Continue)
                })?;
                stats.sql_queries.push(prepared.query.clone());
                return Ok(LogicalPlan::Extension(Extension {
                    node: Arc::new(DuckDbRegion {
                        id,
                        schema: plan.schema().clone(),
                        prepared: Arc::new(prepared),
                        inputs: dependencies,
                        input_tables,
                        source_tables: source_tables.into_values().collect(),
                    }),
                }));
            }
        }
        use sql::lowering::{LoweringContext, LoweringMode, RelationLowering, lower_relation};
        match lower_relation(
            plan,
            &LoweringContext {
                dialect,
                mode: LoweringMode::InIsland,
            },
        )
        .map_err(|e| DataFusionError::Plan(e.to_string()))?
        {
            Some(RelationLowering::Dependent {
                source,
                template,
                schema,
            }) => {
                if schema.as_ref() != plan.schema().as_ref()
                    || template.dialect != dialect.name()
                    || template.parameters != source.schema().fields().len()
                {
                    return Err(DataFusionError::Plan(
                        "dependent relational lowering changed its execution contract".into(),
                    ));
                }
                let source =
                    partition(&source, stats, eligibility, external, dialect, rule_context).await?;
                match dialect {
                    sql::SqlDialect::DuckDb => stats.duckdb_regions += 1,
                    sql::SqlDialect::Postgres => stats.postgres_regions += 1,
                    sql::SqlDialect::Custom(_) => stats.other_sql_regions += 1,
                }
                stats.sql_queries.push(template.sql.clone());
                return Ok(super::dependent::DependentOperation {
                    source: Arc::new(source),
                    template,
                    schema,
                }
                .into_plan());
            }
            Some(RelationLowering::Rewrite(rewritten)) => {
                if &rewritten == plan {
                    return Err(DataFusionError::Plan(
                        "relational rewrite made no progress".into(),
                    ));
                }
                return partition(
                    &rewritten,
                    stats,
                    &mut SqlEligibility {
                        dialect: Some(dialect),
                        language_functions: eligibility.language_functions,
                        ..Default::default()
                    },
                    external,
                    dialect,
                    rule_context,
                )
                .await;
            }
            Some(RelationLowering::Sql(_)) => {
                return Err(DataFusionError::Plan(
                    "required relational operation could not form a SQL island".into(),
                ));
            }
            None => {}
        }
        {
            let boundary = if let Some(kernel) = super::runtime::CompiledKernel::from_plan(plan) {
                super::runtime::policy::validate_kernel(
                    kernel,
                    super::runtime::policy::LoweringPolicy::SqlRequired,
                )
                .map_err(|e| DataFusionError::Plan(e.to_string()))?;
                true
            } else {
                matches!(
                    plan,
                    LogicalPlan::SubqueryAlias(_) | LogicalPlan::EmptyRelation(_)
                )
            };
            if !boundary {
                return Err(DataFusionError::Plan(format!(
                    "SQL-required lowering rejected residual {}: {}",
                    plan.display(),
                    emission_error
                        .as_deref()
                        .unwrap_or(reason.unwrap_or("no SQL implementation"))
                )));
            }
        }
        let mut inputs = Vec::new();
        for input in plan.inputs() {
            inputs
                .push(partition(input, stats, eligibility, external, dialect, rule_context).await?);
        }
        stats.boundary_operators += 1;
        let output_decode = matches!(plan, LogicalPlan::Extension(e) if e.node.name() == "Pruned(DecodeTraversers)");
        stats.output_decode_operators += usize::from(output_decode);
        let category = if output_decode {
            "output_decode"
        } else if emission_error.is_some() {
            "emission_failure"
        } else if matches!(reason, Some("residual extension" | "empty relation")) {
            "missing_transformation"
        } else {
            "capability_boundary"
        };
        stats.sql_boundaries.push(SqlBoundary {
            operator: plan.display().to_string(),
            engine: dialect.name().into(),
            category: category.into(),
            reason: if output_decode {
                "decode SQL values into the language result representation".into()
            } else {
                emission_error
                    .unwrap_or_else(|| reason.unwrap_or("no standalone SQL representation").into())
            },
        });
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

fn compile_execution(
    plan: &LogicalPlan,
    kernels: &[Arc<super::runtime::KernelPlanner>],
    resources: &DagSession,
    cost: &Arc<Mutex<crate::ir::QueryCost>>,
) -> RelResult<Arc<dyn ExecutionPlan>> {
    if let LogicalPlan::SubqueryAlias(alias) = plan {
        return compile_execution(&alias.input, kernels, resources, cost);
    }
    if let LogicalPlan::EmptyRelation(empty) = plan {
        let schema = Arc::new(empty.schema.as_arrow().clone());
        let rows = usize::from(empty.produce_one_row);
        let arrays = schema
            .fields()
            .iter()
            .map(|field| arrow::array::new_null_array(field.data_type(), rows))
            .collect();
        let batch = RecordBatch::try_new_with_options(
            schema.clone(),
            arrays,
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(rows)),
        )?;
        return Ok(Arc::new(engine_exec::InputBatches::new(
            schema,
            vec![batch],
        )));
    }
    let inputs = plan
        .inputs()
        .into_iter()
        .map(|input| compile_execution(input, kernels, resources, cost))
        .collect::<RelResult<Vec<_>>>()?;
    if let Some(kernel) = super::runtime::CompiledKernel::from_plan(plan) {
        return kernels
            .first()
            .ok_or_else(|| {
                RelError::Unsupported("language boundary has no execution state".into())
            })?
            .plan(kernel, inputs)
            .map_err(Into::into);
    }
    if let LogicalPlan::Extension(extension) = plan {
        if let Some(operation) = extension
            .node
            .as_any()
            .downcast_ref::<super::dependent::DependentOperation>()
        {
            let session = match resources.region_session.clone() {
                Some(session) => session,
                None => {
                    #[cfg(feature = "duckdb")]
                    {
                        Arc::new(Mutex::new(Box::new(sql::DuckDbSession {
                            executor: resources.executor.clone(),
                        })
                            as Box<dyn sql::region::RegionSession>))
                    }
                    #[cfg(not(feature = "duckdb"))]
                    {
                        return Err(RelError::Unsupported(
                            "Register a DuckDB or Trino executable session".into(),
                        ));
                    }
                }
            };
            return Ok(Arc::new(
                super::dependent::exec::DependentOperationExec::new(
                    operation,
                    inputs[0].clone(),
                    session,
                    cost.clone(),
                ),
            ));
        }
        if let Some(region) = extension.node.as_any().downcast_ref::<DuckDbRegion>() {
            return Ok(Arc::new(DuckDbExec {
                prepared: region.prepared.clone(),
                inputs,
                input_tables: region.input_tables.clone(),
                source_tables: region.source_tables.clone(),
                #[cfg(feature = "duckdb")]
                executor: resources.executor.clone(),
                region_session: resources.region_session.clone(),
                cost: cost.clone(),
                properties: Arc::new(PlanProperties::new(Arc::new(
                    region.schema.as_arrow().clone(),
                ))),
            }));
        }
    }
    Err(RelError::Unsupported(format!(
        "Executable engine requires SQL lowering for {}",
        plan.display()
    )))
}

#[derive(Debug)]
struct DuckDbExec {
    inputs: Vec<Arc<dyn ExecutionPlan>>,
    input_tables: Vec<sql::TableData>,
    source_tables: Vec<SourceTable>,
    region_session: Option<sql::region::SharedRegionSession>,
    cost: Arc<Mutex<crate::ir::QueryCost>>,
    prepared: Arc<sql::PreparedSql>,
    #[cfg(feature = "duckdb")]
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
    properties: Arc<PlanProperties>,
}
impl ExecutionPlan for DuckDbExec {
    fn name(&self) -> &str {
        match self.prepared.dialect.name() {
            "duckdb" => "DuckDbExec",
            "trino" => "TrinoExec",
            _ => "UnsupportedSqlEngine",
        }
    }
    fn description(&self) -> String {
        if self.inputs.is_empty() {
            return self.name().into();
        }
        let concurrent = self
            .region_session
            .as_ref()
            .is_some_and(|s| s.lock().is_ok_and(|s| s.supports_concurrent_statements()));
        if !concurrent
            && self
                .inputs
                .iter()
                .any(|p| !p.as_any().is::<engine_exec::InputBatches>())
        {
            format!(
                "{} [shared-session inputs: Arrow IPC replay spool, bounded memory]",
                self.name()
            )
        } else {
            format!("{} [inputs: live Arrow streams]", self.name())
        }
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
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.len() != self.inputs.len() {
            return Err(DataFusionError::Plan(
                "DuckDbExec dependency mismatch".into(),
            ));
        }
        Ok(Arc::new(Self {
            inputs: children,
            input_tables: self.input_tables.clone(),
            source_tables: self.source_tables.clone(),
            region_session: self.region_session.clone(),
            cost: self.cost.clone(),
            #[cfg(feature = "duckdb")]
            executor: self.executor.clone(),
            prepared: self.prepared.clone(),
            properties: self.properties.clone(),
        }))
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
        #[cfg(feature = "duckdb")]
        let executor = self.executor.clone();
        let region_session = self.region_session.clone();
        let prepared = self.prepared.clone();
        let cost = self.cost.clone();
        let schema = self.schema();
        let expected = schema.clone();
        let inputs = self.inputs.clone();
        let tables = self.input_tables.clone();
        #[cfg(feature = "duckdb")]
        let source_tables = self.source_tables.clone();
        let context = _context;
        let work = async move {
            use crate::execution::stream::{BatchSource, BatchStream, InputRelation};
            use futures::StreamExt;
            let mut prepared = prepared.as_ref().clone();
            let concurrent = match &region_session {
                Some(session) => session
                    .lock()
                    .map_err(|_| DataFusionError::Execution("SQL session poisoned".into()))?
                    .supports_concurrent_statements(),
                None => false,
            };
            let mut relations = Vec::new();
            #[cfg(feature = "duckdb")]
            if region_session.is_some() {
                let source_count = source_tables.len();
                for (index, table) in source_tables.into_iter().enumerate() {
                    let name = datafusion::common::TableReference::parse_str(&table.name);
                    let table_sql = name
                        .catalog()
                        .into_iter()
                        .chain(name.schema())
                        .chain(std::iter::once(name.table()))
                        .map(|p| sql::SqlDialect::DuckDb.quote_ident(p))
                        .collect::<Vec<_>>()
                        .join(".");
                    let columns = table
                        .schema
                        .fields()
                        .iter()
                        .map(|f| sql::SqlDialect::DuckDb.quote_ident(f.name()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let query = sql::PreparedSql::query_only(
                        format!("SELECT {columns} FROM {table_sql}"),
                        table.schema.clone(),
                    );
                    let stream = sql::DuckDbSession {
                        executor: executor.clone(),
                    }
                    .prepared(query, vec![], Some(table.schema))
                    .await
                    .map_err(DataFusionError::Execution)?;
                    // A source connection permits only one unfinished statement.
                    // Spool only when more mapped tables must share that source
                    // connection, avoiding a source-source deadlock in joins.
                    let source = if index + 1 < source_count || !inputs.is_empty() || table.uses > 1
                    {
                        BatchSource::spool(stream, &std::env::temp_dir())
                            .await
                            .map_err(DataFusionError::Execution)?
                    } else {
                        BatchSource::once(stream)
                    };
                    relations.push(InputRelation {
                        name: table.name,
                        source,
                    });
                }
            }

            if region_session.is_some() {
                for table in std::mem::take(&mut prepared.tables) {
                    let schema = table.schema.clone();
                    let batches = table.batches;
                    let source = BatchSource::replayable(schema.clone(), move || {
                        Ok(BatchStream::new(
                            schema.clone(),
                            stream::iter(batches.clone().into_iter().map(Ok)),
                        ))
                    });
                    relations.push(InputRelation {
                        name: table.name,
                        source,
                    });
                }
                if !prepared.setup.is_empty() {
                    return Err(DataFusionError::Execution(
                        "Caller SQL sessions require typed input relations, not setup statements"
                            .into(),
                    ));
                }
            }

            for (input, table) in inputs.into_iter().zip(tables) {
                let needs_replay = !concurrent && !input.as_any().is::<engine_exec::InputBatches>();
                let source = engine_exec::execute_stream(input, context.clone())?;
                let empty_input = source.schema().fields().is_empty();
                let transport_schema = table.schema.clone();
                let stream = BatchStream::new(transport_schema.clone(), source.map(move |batch| {
                    let batch = batch.map_err(|e| e.to_string())?;
                    if !empty_input { return Ok(batch); }
                    RecordBatch::try_new(transport_schema.clone(), vec![Arc::new(
                        arrow::array::Int64Array::from(vec![1_i64; batch.num_rows()]))])
                        .map_err(|e| e.to_string())
                }));
                // All embedded islands share a session. A reentrant scan cannot
                // execute its upstream statement while the downstream owns it.
                // Explicit IPC replay is bounded in memory and releases that session.
                let source = if needs_replay {
                    BatchSource::spool(stream, &std::env::temp_dir())
                        .await
                        .map_err(DataFusionError::Execution)?
                } else {
                    BatchSource::once(stream)
                };
                relations.push(InputRelation {
                    name: table.name,
                    source,
                });
            }
            let returned = if let Some(session) = region_session {
                tokio::task::spawn_blocking(move || {
                    let mut session = session
                        .lock()
                        .map_err(|_| DataFusionError::Execution("SQL session poisoned".into()))?;
                    if let Some(required) = &prepared.float_helper {
                        session
                            .validate_float_helper(required)
                            .map_err(|e| DataFusionError::Execution(e.to_string()))?;
                    }
                    session
                        .query_stream(&prepared.query, prepared.output_schema(), relations)
                        .map_err(|e| DataFusionError::Execution(e.to_string()))
                })
                .await
                .map_err(|e| DataFusionError::Execution(e.to_string()))??
            } else {
                #[cfg(feature = "duckdb")]
                {
                    let physical_schema = if expected.fields().is_empty() {
                        prepared.output_schema()
                    } else { expected.clone() };
                    sql::DuckDbSession { executor }
                        .prepared(prepared, relations, Some(physical_schema))
                        .await
                        .map_err(DataFusionError::Execution)?
                }
                #[cfg(not(feature = "duckdb"))]
                {
                    return Err(DataFusionError::Execution(
                        "Register a DuckDB or Trino executable session".into(),
                    ));
                }
            };
            cost.lock()
                .map_err(|_| DataFusionError::Execution("Query cost poisoned".into()))?
                .sql_executions += 1;
            Ok::<_, DataFusionError>(returned.record_cost(cost.clone()).map(move |batch| {
                let returned = batch.map_err(DataFusionError::Execution)?;
                let mut cost = cost
                    .lock()
                    .map_err(|_| DataFusionError::Execution("Query cost poisoned".into()))?;
                cost.sql_output_rows = cost
                    .sql_output_rows
                    .saturating_add(returned.num_rows() as u64);
                cost.sql_output_bytes = cost
                    .sql_output_bytes
                    .saturating_add(returned.get_array_memory_size() as u64);
                let arrays = returned
                    .columns()
                    .iter()
                    .zip(expected.fields())
                    .map(|(a, f)| coerce_sql_array(a, f.data_type()))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                RecordBatch::try_new_with_options(
                    expected.clone(),
                    arrays,
                    &arrow::record_batch::RecordBatchOptions::new()
                        .with_row_count(Some(returned.num_rows())),
                )
                .map_err(DataFusionError::from)
            }))
        };
        use futures::TryStreamExt;
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            schema,
            stream::once(work).try_flatten(),
        )))
    }
}

/// Execute SQL regions and explicit boundaries using the Orchid stream scheduler.
pub async fn execute(lowered: LoweredPlan) -> RelResult<(ReturnedBatches, DagStats)> {
    execute_with_extensions(lowered, vec![], None, None).await
}

/// Prepare nested and top-level regions with the same island placement rules.
pub(crate) async fn prepare_with_extensions(
    logical: &LogicalPlan,
    extensions: Vec<Arc<super::runtime::KernelPlanner>>,
    resources: &DagSession,
    _cost: Arc<Mutex<crate::ir::QueryCost>>,
    rule_context: &crate::ir::rules::RuleContext,
) -> RelResult<(Arc<dyn ExecutionPlan>, Arc<TaskContext>, DagStats)> {
    let session = &resources.session;
    // Optimize while relational scans and expressions remain visible, before
    // DuckDB placement turns each region into an opaque physical source.
    // One state snapshot per query keeps execution time and function metadata
    // consistent across compiler passes without any executable environment.
    let query_state = session.state();
    let (initial, mut proofs) = super::pipeline::relational(logical.clone(), rule_context, false)?;
    let optimized = if resources.optimize {
        query_state.optimize(&initial)?
    } else {
        // Mapped SQL placement skips optional optimizer rewrites, but residual
        // logical operators still require type coercion and function analysis.
        query_state.analyzer().execute_and_check(
            initial,
            query_state.config_options(),
            |_, _| {},
        )?
    };
    // Input-only native scalar calls must become typed literals before SQL
    // eligibility and search lowering, even when optional rewrites are disabled.
    // SimplifyExpressions respects function volatility and leaves row-dependent
    // calls (and engine-only calls) in place. Reuse the query's execution context
    // so stable functions see the same clock as the rest of the query.
    let optimized = if !resources.optimize
        && crate::ir::diagnostics::constant_folding_enabled()
        && super::conversion_rules::can_fold_plan(&optimized)
    {
        datafusion::optimizer::Optimizer::with_rules(vec![Arc::new(
            datafusion::optimizer::simplify_expressions::SimplifyExpressions::new(),
        )])
        .optimize(optimized, &query_state, |_, _| {})?
    } else {
        optimized
    };
    let mut has_search = false;
    optimized.apply_with_subqueries(|p| {
        has_search |= super::search::node(p).is_some();
        Ok(TreeNodeRecursion::Continue)
    })?;
    let optimized = if has_search && !resources.optimize {
        datafusion::optimizer::Optimizer::with_rules(vec![
            Arc::new(datafusion::optimizer::push_down_filter::PushDownFilter::new()),
            Arc::new(datafusion::optimizer::optimize_projections::OptimizeProjections::new()),
        ])
        .optimize(optimized, &query_state, |_, _| {})?
    } else {
        optimized
    };
    crate::ir::rules::RuleSession::new(rule_context.clone()).invalidate();
    let optimized = super::search::bind_seeds(optimized)?;
    crate::ir::rules::RuleSession::new(rule_context.clone()).invalidate();
    let (optimized, more) = super::pipeline::relational(optimized, rule_context, true)?;
    proofs.extend(more);
    let selected = super::pipeline::placement(optimized, rule_context)?;
    let (optimized, mut optimizer_decisions) = super::statistics::optimize(selected.plan)?;
    optimizer_decisions.extend(selected.access_decisions);
    let optimized = super::search::push_source_filters(optimized)?;
    crate::ir::rules::RuleSession::new(rule_context.clone()).invalidate();
    let mut stats = DagStats {
        optimizer_decisions,
        plan_estimates: super::statistics::explain(&optimized),
        constraint_proofs: proofs,
        layout_selections: selected.layout_selections,
        representation_selections: selected.representation_selections,
        logical_plan: optimized.display_indent().to_string(),
        ..Default::default()
    };
    let plan = {
        let mut eligibility = SqlEligibility::default();
        let dialect = match &resources.region_session {
            Some(session) => session
                .lock()
                .map_err(|_| DataFusionError::Execution("SQL region session poisoned".into()))?
                .dialect(),
            None => {
                #[cfg(feature = "duckdb")]
                {
                    sql::SqlDialect::DuckDb
                }
                #[cfg(not(feature = "duckdb"))]
                {
                    return Err(RelError::Unsupported(
                        "Register a DuckDB or Trino executable session".into(),
                    ));
                }
            }
        };
        let optimized =
            super::lowering_stage::normalize(optimized, &query_state, Some(dialect), rule_context)?;
        let optimized = if matches!(dialect, sql::SqlDialect::Custom(_)) {
            super::optimizer::simplify_bound_expressions(optimized, &query_state)?
        } else {
            optimized
        };
        stats.logical_plan = optimized.display_indent().to_string();
        if dialect != sql::SqlDialect::DuckDb && dialect != sql::SqlDialect::Custom(&sql::trino::TRINO) {
            return Err(RelError::Unsupported(format!(
                "{} is a source dialect, not an executable engine",
                dialect.name()
            )));
        }
        eligibility.dialect = Some(dialect);
        #[cfg(feature = "duckdb")]
        {
            eligibility.language_functions = resources.region_session.is_none()
                && resources
                    .executor
                    .lock()
                    .map_err(|_| DataFusionError::Execution("SQL session poisoned".into()))?
                    .language_functions_enabled();
        }
        partition(
            &optimized,
            &mut stats,
            &mut eligibility,
            &resources.external,
            dialect,
            rule_context,
        )
        .await?
    };

    let plan = super::lowering_stage::normalize_residual(plan, &query_state, rule_context)?;
    super::lowering_stage::validate_residual(&plan)?;
    super::pipeline::validate_host_contract(&plan)?;
    let physical = compile_execution(&plan, &extensions, resources, &_cost)?;
    validate_sql_required_physical(&physical)?;
    stats.rule_trace = rule_context.trace();
    stats.physical_plan = engine_exec::explain(&physical);
    Ok((
        physical,
        Arc::new(
            TaskContext::default()
                .with_batch_size(query_state.config_options().execution.batch_size),
        ),
        stats,
    ))
}

pub(crate) async fn execute_with_extensions(
    lowered: LoweredPlan,
    extensions: Vec<Arc<super::runtime::KernelPlanner>>,
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
    let (physical, task, mut stats) = prepare_with_extensions(
        &lowered.plan,
        extensions,
        resources,
        cost.clone(),
        &lowered.rule_context,
    )
    .await?;
    let prepared_at = std::time::Instant::now();
    let planned_at = std::time::Instant::now();
    let schema = physical.schema();
    let mut batches = engine_exec::collect(physical, task).await?;
    let batch = if batches.len() == 1 {
        // SQL regions and fused residual pipelines normally return one batch.
        // Preserve its buffers while applying the physical output schema.
        let batch = batches.pop().unwrap();
        if batch.schema() == schema {
            batch
        } else {
            RecordBatch::try_new_with_options(
                schema,
                batch.columns().to_vec(),
                &arrow::array::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
            )?
        }
    } else {
        arrow_select::concat::concat_batches(&schema, batches.iter())?
    };
    stats.cost = cost
        .lock()
        .map_err(|_| DataFusionError::Execution("Query cost poisoned".into()))?
        .clone();
    stats.cost.result_rows = batch.num_rows() as u64;
    stats.cost.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    if std::env::var_os("ORCHIDDB_PROFILE_DAG").is_some() {
        eprintln!(
            "dag-profile {}",
            serde_json::json!({
                "prepare_ms": (prepared_at-started).as_secs_f64()*1000.0,
                "physical_plan_ms": (planned_at-prepared_at).as_secs_f64()*1000.0,
                "execute_ms": planned_at.elapsed().as_secs_f64()*1000.0,
                "regions":stats.duckdb_regions + stats.postgres_regions + stats.other_sql_regions,"duckdb_regions":stats.duckdb_regions,"postgres_regions":stats.postgres_regions,"boundaries":stats.boundary_operators,
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
pub(crate) fn coerce_sql_array(
    array: &arrow::array::ArrayRef,
    target: &arrow::datatypes::DataType,
) -> std::result::Result<arrow::array::ArrayRef, arrow::error::ArrowError> {
    use arrow::array::{Array, make_array, new_null_array};
    use arrow::datatypes::DataType;
    if array.data_type() == target {
        return Ok(array.clone());
    }
    if crate::ir::functions::domain::descriptor(target).is_some() {
        return crate::ir::functions::domain::restore(array, target)
            .map_err(|e| arrow::error::ArrowError::CastError(e.to_string()));
    }
    if target == &DataType::Null && array.null_count() == array.len() {
        return Ok(new_null_array(target, array.len()));
    }
    let children = match (array.data_type(), target) {
        (DataType::List(_), DataType::List(field))
        | (DataType::LargeList(_), DataType::LargeList(field)) => Some(vec![field.data_type()]),
        (DataType::FixedSizeList(_, a), DataType::FixedSizeList(field, b)) if a == b => {
            Some(vec![field.data_type()])
        }
        (DataType::Struct(a), DataType::Struct(b)) if a.len() == b.len() => {
            Some(b.iter().map(|f| f.data_type()).collect())
        }
        _ => None,
    };
    if let Some(children) = children {
        let data = array.to_data();
        let converted = data
            .child_data()
            .iter()
            .zip(children)
            .map(|(child, target)| {
                coerce_sql_array(&make_array(child.clone()), target).map(|a| a.to_data())
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return Ok(make_array(
            data.into_builder()
                .data_type(target.clone())
                .child_data(converted)
                .build()?,
        ));
    }
    arrow::compute::cast_with_options(
        array,
        target,
        &arrow::compute::CastOptions {
            safe: false,
            ..Default::default()
        },
    )
}

impl DagStats {
    pub(crate) fn merge_execution(&mut self, nested: &Self) {
        self.cost.add_work(&nested.cost);
        // Rule traces are one bounded query snapshot, not additive work counters.
        self.constraint_proofs
            .extend(nested.constraint_proofs.iter().cloned());
        self.layout_selections
            .extend(nested.layout_selections.iter().cloned());
        self.representation_selections
            .extend(nested.representation_selections.iter().cloned());
        self.plan_estimates
            .extend(nested.plan_estimates.iter().cloned());
        self.optimizer_decisions
            .extend(nested.optimizer_decisions.iter().cloned());
        self.output_decode_operators += nested.output_decode_operators;
        self.sql_boundaries
            .extend(nested.sql_boundaries.iter().cloned());
        self.runtime_inputs
            .extend(nested.runtime_inputs.iter().cloned());
        if !nested.logical_plan.is_empty() {
            self.logical_plan.push_str("\nExecuted subplan:\n");
            self.logical_plan.push_str(&nested.logical_plan);
        }
        self.duckdb_regions += nested.duckdb_regions;
        self.postgres_regions += nested.postgres_regions;
        self.other_sql_regions += nested.other_sql_regions;
        self.boundary_operators += nested.boundary_operators;
        self.sql_queries.extend(nested.sql_queries.iter().cloned());
        self.write_queries
            .extend(nested.write_queries.iter().cloned());
        if !nested.physical_plan.is_empty() && !self.physical_plan.contains(&nested.physical_plan) {
            self.physical_plan.push_str("\nExecuted subplan:\n");
            self.physical_plan.push_str(&nested.physical_plan);
        }
    }
}

/// Audit the Orchid scheduler: only SQL and explicit boundaries may execute.
pub(crate) fn validate_sql_required_physical(plan: &Arc<dyn ExecutionPlan>) -> RelResult<()> {
    let mut pending = vec![plan.clone()];
    while let Some(plan) = pending.pop() {
        let allowed = super::runtime::is_kernel_exec(plan.as_ref())
            || plan.as_any().is::<engine_exec::InputBatches>()
            || plan
                .as_any()
                .is::<super::dependent::exec::DependentOperationExec>();
        let allowed = allowed || plan.as_any().is::<DuckDbExec>();
        if !allowed {
            return Err(RelError::Unsupported(format!(
                "SQL-required lowering rejected executable operator {}",
                plan.name()
            )));
        }
        pending.extend(plan.children().into_iter().cloned());
    }
    Ok(())
}
