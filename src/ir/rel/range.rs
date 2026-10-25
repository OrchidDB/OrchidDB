//! Compiler-only integer range metadata. DuckDB and Trino execute the SQL table function.
use super::*;
use datafusion::catalog::{Session, TableProvider};
use datafusion::common::DataFusionError;
use datafusion::logical_expr::TableType;
use std::any::Any;

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
        _state: &dyn Session,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> datafusion::common::Result<Arc<dyn datafusion::physical_plan::ExecutionPlan>> {
        Err(DataFusionError::Plan(
            "Integer ranges are compiler metadata and must execute in a DuckDB or Trino SQL island"
                .into(),
        ))
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
    #[test]
    fn range_provider_retains_exact_boundaries_for_sql_lowering() {
        for (start, stop, step) in [
            (i64::MAX - 1, i64::MAX, 1),
            (i64::MIN + 1, i64::MIN, -1),
            (i64::MAX, i64::MIN, i64::MIN),
        ] {
            let provider = IntegerRange::new(start, stop, step, "n".into());
            assert_eq!(
                (provider.start, provider.stop, provider.step),
                (start, stop, step)
            );
            assert_eq!(provider.schema().field(0).data_type(), &DataType::Int64);
        }
    }
}
