//! One executable relational DAG. DuckDB regions are DataFusion physical sources;
//! all residual relational operators execute through DataFusion. No Graph IR plan
//! is retained by the executor and no graph interpreter fallback is available.
use arrow::array::RecordBatch;
use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

use super::{LoweredPlan, RelResult, sql};
use crate::ir::interpreter::ReturnedBatches;
use async_trait::async_trait;
use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
use datafusion::common::{DFSchemaRef, DataFusionError, Result};
use datafusion::execution::{TaskContext, context::SessionState};
use datafusion::logical_expr::{
    Expr, Extension, LogicalPlan, UserDefinedLogicalNode, UserDefinedLogicalNodeCore,
};
use datafusion::physical_expr::EquivalenceProperties;
use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
    SendableRecordBatchStream,
};
use datafusion::physical_planner::{DefaultPhysicalPlanner, ExtensionPlanner, PhysicalPlanner};
use datafusion::prelude::{SessionConfig, SessionContext};
use futures::stream;

#[derive(Debug, Clone, Default)]
pub struct DagStats {
    pub duckdb_regions: usize,
    pub datafusion_operators: usize,
    pub physical_plan: String,
}

/// Execution resources owned by one graph engine, never shared between engines.
/// SQL source tables are content-addressed by DuckDbExecutor; logical plans and
/// graph snapshots are rebuilt for every query.
pub(crate) struct DagSession {
    session: SessionContext,
    #[cfg(feature = "duckdb")]
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
}
impl DagSession {
    pub(crate) fn new(timeout: Option<std::time::Duration>) -> Self {
        Self {
            session: SessionContext::new_with_config(
                SessionConfig::new().with_target_partitions(1),
            ),
            #[cfg(feature = "duckdb")]
            executor: Arc::new(Mutex::new(
                timeout
                    .map(sql::DuckDbExecutor::with_timeout)
                    .unwrap_or_default(),
            )),
        }
    }
}

/// A planned DuckDB region has no DataFusion inputs: its native relational scans
/// and their Arrow sources are captured by PreparedSql. Its schema remains the
/// original relational schema, including column qualifiers used by consumers.
#[derive(Clone)]
struct DuckDbRegion {
    id: usize,
    schema: DFSchemaRef,
    prepared: Arc<sql::PreparedSql>,
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
        "DuckDbRegion"
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
        write!(f, "DuckDbRegion(id={})", self.id)
    }
    fn with_exprs_and_inputs(
        &self,
        expressions: Vec<Expr>,
        inputs: Vec<LogicalPlan>,
    ) -> Result<Self> {
        if !expressions.is_empty() || !inputs.is_empty() {
            return Err(DataFusionError::Plan("DuckDB region is a source".into()));
        }
        Ok(self.clone())
    }
}

/// Unknown or volatile UDFs and extension nodes execute only in DataFusion.
/// DuckDB eligibility is a planning decision; execution errors are never retried
/// through another engine, avoiding repeated effects or changed error semantics.
/// Analyze each logical node once. A rejected ancestor must not repeatedly
/// rescan its descendants while partitioning into smaller regions.
#[derive(Default)]
struct SqlEligibility {
    reasons: std::collections::HashMap<usize, Option<&'static str>>,
}
impl SqlEligibility {
    fn visit(&mut self, plan: &LogicalPlan) -> Option<&'static str> {
        let key = plan as *const LogicalPlan as usize;
        if let Some(reason) = self.reasons.get(&key) { return *reason; }
        let mut reason = match plan {
            LogicalPlan::Extension(_) => Some("residual extension"),
            LogicalPlan::EmptyRelation(_) => Some("empty relation"),
            _ if plan.schema().fields().is_empty() => Some("zero-column relation"),
            _ => None,
        };
        if reason.is_some() {
            self.reasons.insert(key, reason);
            return reason;
        }
        for expr in plan.expressions() {
            let _ = expr.apply(|expr| {
                if let Expr::ScalarFunction(function) = expr {
                    let native = function.func.name().starts_with(crate::ir::functions::ENGINE_FUNCTION_PREFIX)
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

fn partition<'a>(
    plan: &'a LogicalPlan,
    stats: &'a mut DagStats,
    eligibility: &'a mut SqlEligibility,
) -> futures::future::BoxFuture<'a, Result<LogicalPlan>> {
    Box::pin(async move {
        let explain = std::env::var_os("CRABGRAPH_EXPLAIN_DAG").is_some();
        let reason = eligibility.visit(plan);
        if explain && let Some(reason) = reason {
            eprintln!("DuckDB boundary: {reason}");
        }
        if reason.is_none() {
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
            let prepared = sql::prepare(&candidate, sql::SqlDialect::DuckDb).await;
            if explain && let Err(error) = &prepared {
                eprintln!("DuckDB boundary: SQL preparation: {error}");
            }
            if let Ok(prepared) = prepared {
                if std::env::var_os("CRABGRAPH_EXPLAIN_DAG").is_some() {
                    eprintln!("DuckDB candidate: {}", prepared.query);
                }
                let id = stats.duckdb_regions;
                stats.duckdb_regions += 1;
                return Ok(LogicalPlan::Extension(Extension {
                    node: Arc::new(DuckDbRegion {
                        id,
                        schema: plan.schema().clone(),
                        prepared: Arc::new(prepared),
                    }),
                }));
            }
        }
        let mut inputs = Vec::new();
        for input in plan.inputs() {
            inputs.push(partition(input, stats, eligibility).await?);
        }
        stats.datafusion_operators += 1;
        plan.with_new_exprs(plan.expressions(), inputs)
    })
}

#[cfg(feature = "duckdb")]
#[derive(Debug)]
struct RegionPlanner {
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
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
            properties,
        })))
    }
}

#[cfg(feature = "duckdb")]
#[derive(Debug)]
struct DuckDbExec {
    prepared: Arc<sql::PreparedSql>,
    executor: Arc<Mutex<sql::DuckDbExecutor>>,
    properties: Arc<PlanProperties>,
}
#[cfg(feature = "duckdb")]
impl DisplayAs for DuckDbExec {
    fn fmt_as(&self, _format: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DuckDbExec")
    }
}
#[cfg(feature = "duckdb")]
impl ExecutionPlan for DuckDbExec {
    fn name(&self) -> &str {
        "DuckDbExec"
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
            return Err(DataFusionError::Plan("DuckDbExec is a source".into()));
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
        let prepared = self.prepared.clone();
        let schema = self.schema();
        let expected = schema.clone();
        let work = async move {
            tokio::task::spawn_blocking(move || {
                let mut executor = executor
                    .lock()
                    .map_err(|_| DataFusionError::Execution("DuckDB executor poisoned".into()))?;
                let returned = sql::execute_prepared(&mut *executor, &prepared)
                    .map_err(|e| DataFusionError::Execution(e.to_string()))?;
                let arrays = returned.batch.columns().iter().zip(expected.fields())
                    .map(|(array, field)| {
                        if array.data_type() == field.data_type() { Ok(array.clone()) }
                        else { arrow::compute::cast(array, field.data_type()) }
                    })
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

pub(crate) async fn execute_with_extensions(
    lowered: LoweredPlan,
    mut extensions: Vec<Arc<dyn ExtensionPlanner + Send + Sync>>,
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
    let session = &resources.session;
    // Optimize while relational scans and expressions remain visible, before
    // DuckDB placement turns each region into an opaque physical source.
    // One state snapshot per query keeps execution time and function metadata
    // consistent across logical and physical planning without repeated clones.
    let query_state = session.state();
    let optimized = query_state.optimize(&lowered.plan)?;
    let mut stats = DagStats::default();
    #[cfg(feature = "duckdb")]
    let plan = {
        let mut eligibility = SqlEligibility::default();
        partition(&optimized, &mut stats, &mut eligibility).await?
    };
    #[cfg(not(feature = "duckdb"))]
    let plan = {
        let _ = optimized.apply(|_| {
            stats.datafusion_operators += 1;
            Ok(TreeNodeRecursion::Continue)
        });
        optimized.clone()
    };
    let prepared_at = std::time::Instant::now();
    #[cfg(feature = "duckdb")]
    extensions.push(Arc::new(RegionPlanner {
        executor: resources.executor.clone(),
    }));
    let planner = DefaultPhysicalPlanner::with_extension_planners(extensions);
    let physical = planner
        .create_physical_plan(&plan, &query_state)
        .await?;
    stats.physical_plan = datafusion::physical_plan::displayable(physical.as_ref())
        .indent(true)
        .to_string();
    let planned_at = std::time::Instant::now();
    let schema = physical.schema();
    let mut batches = datafusion::physical_plan::collect(physical, Arc::new(TaskContext::from(&query_state))).await?;
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
    if std::env::var_os("CRABGRAPH_PROFILE_DAG").is_some() {
        eprintln!(
            "dag-profile {}",
            serde_json::json!({
                "prepare_ms": (prepared_at-started).as_secs_f64()*1000.0,
                "physical_plan_ms": (planned_at-prepared_at).as_secs_f64()*1000.0,
                "execute_ms": planned_at.elapsed().as_secs_f64()*1000.0,
                "regions":stats.duckdb_regions,"residuals":stats.datafusion_operators,
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
