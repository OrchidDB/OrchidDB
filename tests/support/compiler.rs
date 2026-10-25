//! Logical test fixtures and scalar-vector oracles. No DataFusion session or
//! physical query engine is constructed; relational execution uses DuckDB.
#![allow(dead_code)]
use arrow::array::RecordBatch;
use datafusion::{
    common::{Result, config::ConfigOptions},
    datasource::{MemTable, provider_as_source},
    logical_expr::{Expr, LogicalPlan, LogicalPlanBuilder},
    optimizer::{Optimizer, OptimizerContext, analyzer::Analyzer},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub struct CompilerFixture {
    batches: Mutex<BTreeMap<String, RecordBatch>>,
}
impl CompilerFixture {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register_batch(&self, name: &str, batch: RecordBatch) -> Result<()> {
        self.batches.lock().unwrap().insert(name.into(), batch);
        Ok(())
    }
    pub fn read_batch(&self, batch: RecordBatch) -> Result<FixturePlan> {
        scan("fixture", batch)
    }
    pub async fn table(&self, name: &str) -> Result<FixturePlan> {
        scan(name, self.batches.lock().unwrap()[name].clone())
    }
    pub fn state(&self) -> CompilerState {
        CompilerState::default()
    }

    /// Evaluate only projections over fixture batches using native scalar
    /// kernels. This remains an independent oracle for emitted backend SQL.
    pub fn evaluate_projection(&self, plan: &LogicalPlan) -> Result<Vec<RecordBatch>> {
        // Match logical scalar preparation before invoking vector kernels:
        // arrow_cast and lazy conditional UDFs simplify into primitive Exprs.
        self.evaluate_prepared_projection(&optimize(plan)?)
    }

    fn evaluate_prepared_projection(&self, plan: &LogicalPlan) -> Result<Vec<RecordBatch>> {
        match plan {
            LogicalPlan::Projection(projection) => self
                .evaluate_projection(&projection.input)?
                .into_iter()
                .map(|batch| {
                    let props = datafusion::logical_expr::execution_props::ExecutionProps::new();
                    let columns = projection
                        .expr
                        .iter()
                        .map(|expr| {
                            datafusion_physical_expr::create_physical_expr(
                                expr,
                                projection.input.schema(),
                                &props,
                            )?
                            .evaluate(&batch)?
                            .into_array(batch.num_rows())
                        })
                        .collect::<Result<Vec<_>>>()?;
                    Ok(RecordBatch::try_new(
                        Arc::new(projection.schema.as_arrow().clone()),
                        columns,
                    )?)
                })
                .collect(),
            LogicalPlan::TableScan(scan) => {
                let batch = self.batches.lock().unwrap()[scan.table_name.table()].clone();
                Ok(vec![if let Some(indices) = &scan.projection {
                    batch.project(indices)?
                } else {
                    batch
                }])
            }
            other => datafusion::common::plan_err!(
                "scalar oracle only supports projections and fixture scans: {other}"
            ),
        }
    }
}
fn scan(name: &str, batch: RecordBatch) -> Result<FixturePlan> {
    Ok(FixturePlan(
        LogicalPlanBuilder::scan(
            name,
            provider_as_source(Arc::new(MemTable::try_new(
                batch.schema(),
                vec![vec![batch]],
            )?)),
            None,
        )?
        .build()?,
    ))
}
pub struct FixturePlan(LogicalPlan);
impl FixturePlan {
    pub fn select(self, expr: Vec<Expr>) -> Result<Self> {
        Ok(Self(
            LogicalPlanBuilder::from(self.0).project(expr)?.build()?,
        ))
    }
    pub fn distinct(self) -> Result<Self> {
        Ok(Self(LogicalPlanBuilder::from(self.0).distinct()?.build()?))
    }
    pub fn into_unoptimized_plan(self) -> LogicalPlan {
        self.0
    }
    pub fn into_optimized_plan(self) -> Result<LogicalPlan> {
        optimize(&self.0)
    }
}
#[derive(Default)]
pub struct CompilerState {
    options: ConfigOptions,
}
impl CompilerState {
    pub fn analyzer(&self) -> Analyzer {
        Analyzer::new()
    }
    pub fn config_options(&self) -> &ConfigOptions {
        &self.options
    }
    pub fn optimize(&self, plan: &LogicalPlan) -> Result<LogicalPlan> {
        optimize(plan)
    }
}
pub fn optimize(plan: &LogicalPlan) -> Result<LogicalPlan> {
    let options = ConfigOptions::default();
    let plan = Analyzer::new().execute_and_check(plan.clone(), &options, |_, _| {})?;
    Optimizer::new().optimize(
        plan,
        &OptimizerContext::new().with_skip_failing_rules(false),
        |_, _| {},
    )
}
#[cfg(feature = "duckdb")]
pub async fn execute(plan: LogicalPlan) -> Vec<RecordBatch> {
    use orchiddb::ir::{
        policy::ResultForm,
        rel::{
            LoweredPlan,
            sql::{DuckDbExecutor, execute_lowered_sql},
        },
    };
    let fields = plan
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect();
    let lowered = LoweredPlan {
        plan,
        fields,
        result_form: ResultForm::RowSet,
        rule_context: Default::default(),
        islands: Default::default(),
    };
    vec![
        execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap()
            .batch,
    ]
}
