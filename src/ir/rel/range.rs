//! Streaming integer ranges. SQL backends execute these as table functions;
//! DataFusion generates bounded batches with overflow-safe arithmetic.
use super::*;
use datafusion::catalog::{Session, TableProvider};
use datafusion::common::{DataFusionError, Result};
use datafusion::execution::TaskContext;
use datafusion::logical_expr::TableType;
use datafusion::physical_expr::EquivalenceProperties;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, Partitioning, PlanProperties, SendableRecordBatchStream,
};
use std::any::Any;
use std::fmt;

#[derive(Debug)]
pub(crate) struct IntegerRange {
    pub start: i64,
    pub stop: i64,
    pub step: i64,
    pub column: String,
    schema: SchemaRef,
}
impl IntegerRange {
    pub fn new(start: i64, stop: i64, step: i64, column: String) -> Self {
        let schema = Arc::new(Schema::new(vec![Field::new(
            &column,
            DataType::Int64,
            false,
        )]));
        Self {
            start,
            stop,
            step,
            column,
            schema,
        }
    }
}
#[async_trait::async_trait]
impl TableProvider for IntegerRange {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    fn table_type(&self) -> TableType {
        TableType::Temporary
    }
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> datafusion::common::Result<Arc<dyn ExecutionPlan>> {
        let _ = filters; // The default provider contract does not push filters.
        let schema = match projection {
            Some(indices) => Arc::new(self.schema.project(indices)?),
            None => self.schema(),
        };
        Ok(Arc::new(RangeExec {
            start: self.start as i128,
            stop: self.stop as i128,
            step: self.step as i128,
            batch_size: state.config_options().execution.batch_size.max(1),
            limit,
            source_schema: self.schema(),
            projection: projection.cloned(),
            properties: Arc::new(PlanProperties::new(
                EquivalenceProperties::new(schema),
                Partitioning::UnknownPartitioning(1),
                EmissionType::Incremental,
                Boundedness::Bounded,
            )),
        }))
    }
}

#[derive(Debug)]
struct RangeExec {
    start: i128,
    stop: i128,
    step: i128,
    batch_size: usize,
    limit: Option<usize>,
    source_schema: SchemaRef,
    projection: Option<Vec<usize>>,
    properties: Arc<PlanProperties>,
}
impl DisplayAs for RangeExec {
    fn fmt_as(&self, _: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "IntegerRangeExec: {}..={} step {}",
            self.start, self.stop, self.step
        )
    }
}
impl ExecutionPlan for RangeExec {
    fn name(&self) -> &str {
        "IntegerRangeExec"
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
            return Err(DataFusionError::Plan(
                "Integer range has no children".into(),
            ));
        }
        Ok(self)
    }
    fn execute(&self, partition: usize, _: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(
                "Integer range has one partition".into(),
            ));
        }
        let (mut current, stop, step, batch_size) =
            (self.start, self.stop, self.step, self.batch_size);
        let mut remaining = self.limit.unwrap_or(usize::MAX);
        let schema = self.source_schema.clone();
        let projection = self.projection.clone();
        let stream = futures::stream::iter(std::iter::from_fn(move || {
            let within = |value| {
                if step > 0 {
                    value <= stop
                } else {
                    value >= stop
                }
            };
            if remaining == 0 || !within(current) {
                return None;
            }
            let mut values = Vec::with_capacity(batch_size.min(remaining));
            while values.len() < batch_size && remaining > 0 && within(current) {
                values.push(current as i64);
                // i128 holds the first value beyond either i64 endpoint.
                current += step;
                remaining -= 1;
            }
            let result =
                RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(values))])
                    .and_then(|batch| match &projection {
                        Some(indices) => batch.project(indices),
                        None => Ok(batch),
                    })
                    .map_err(DataFusionError::from);
            Some(result)
        }));
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            stream,
        )))
    }
}

impl LoweringContext<'_> {
    pub(super) fn lower_range_unwind(
        &mut self,
        input: LoweredNode,
        args: &[IrExpr],
        bind: &str,
        outer: bool,
    ) -> RelResult<LoweredNode> {
        let (start, stop, step) = literal_range_bounds(args)?;
        if outer && ((step > 0 && start > stop) || (step < 0 && start < stop)) {
            let mut projection = existing_columns(&input.plan, &BTreeSet::from([bind.to_owned()]));
            projection.push(lit(ScalarValue::Int64(None)).alias(bind));
            let plan = LogicalPlanBuilder::from(input.plan.clone())
                .project(projection)?
                .build()?;
            return Ok(input.with_plan(plan));
        }
        let name = format!("__w_integer_range_{}", self.scan_counter);
        self.scan_counter += 1;
        let provider = Arc::new(IntegerRange::new(start, stop, step, bind.to_owned()));
        let range = LogicalPlanBuilder::scan(
            name,
            datafusion::datasource::provider_as_source(provider),
            None,
        )?
        .build()?;
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .cross_join(range)?
            .build()?;
        Ok(input.with_plan(plan))
    }
}

pub(super) fn literal_range_bounds(args: &[IrExpr]) -> RelResult<(i64, i64, i64)> {
    let ([start, stop] | [start, stop, _]) = args else {
        return Err(RelError::Unsupported("range arity".into()));
    };
    let start = literal_i64(start)
        .ok_or_else(|| RelError::Unsupported("range start must be a literal integer".into()))?;
    let stop = literal_i64(stop)
        .ok_or_else(|| RelError::Unsupported("range stop must be a literal integer".into()))?;
    let step = args
        .get(2)
        .map(literal_i64)
        .unwrap_or(Some(1))
        .ok_or_else(|| RelError::Unsupported("range step must be a literal integer".into()))?;
    if step == 0 {
        return Err(RelError::Unsupported("range step cannot be zero".into()));
    }
    Ok((start, stop, step))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn datafusion_streams_i64_endpoints_and_respects_limit() {
        let ctx = datafusion::prelude::SessionContext::new();
        for (start, stop, step, expected) in [
            (i64::MAX - 1, i64::MAX, 1, vec![i64::MAX - 1, i64::MAX]),
            (i64::MIN + 1, i64::MIN, -1, vec![i64::MIN + 1, i64::MIN]),
            (5, 1, 1, vec![]),
            (1, 5, -1, vec![]),
            (i64::MAX, i64::MIN, i64::MIN, vec![i64::MAX, -1]),
        ] {
            let table = Arc::new(IntegerRange::new(start, stop, step, "n".into()));
            let batches = ctx.read_table(table).unwrap().collect().await.unwrap();
            let actual = batches
                .iter()
                .flat_map(|batch| {
                    batch
                        .column(0)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .unwrap()
                        .values()
                        .to_vec()
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
        let table = Arc::new(IntegerRange::new(i64::MIN, i64::MAX, 1, "n".into()));
        let batches = ctx
            .read_table(table)
            .unwrap()
            .limit(0, Some(3))
            .unwrap()
            .collect()
            .await
            .unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 3);
    }
}
