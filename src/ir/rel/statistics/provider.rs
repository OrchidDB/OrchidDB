use super::*;
use arrow::datatypes::SchemaRef;
use datafusion::{
    catalog::{Session, TableProvider},
    common::{Result, Statistics, stats::Precision},
    logical_expr::{Expr, TableProviderFilterPushDown, TableType},
    physical_plan::ExecutionPlan,
};
use std::{any::Any, sync::Arc};
#[derive(Debug)]
pub struct StatisticsProvider {
    pub inner: Arc<dyn TableProvider>,
    pub source: SourceRef,
    pub revision: String,
}
#[derive(Debug)]
pub struct SourceRef {
    snapshot: Arc<StatisticsSnapshot>,
    name: String,
}
impl std::ops::Deref for SourceRef {
    type Target = SourceStatistics;
    fn deref(&self) -> &Self::Target {
        &self.snapshot.sources[&self.name]
    }
}
impl StatisticsProvider {
    pub fn wrap(
        inner: Arc<dyn TableProvider>,
        snapshot: Arc<StatisticsSnapshot>,
        name: String,
    ) -> Arc<dyn TableProvider> {
        Arc::new(Self {
            inner,
            revision: snapshot.revision.clone(),
            source: SourceRef { snapshot, name },
        })
    }
}
pub fn statistics_provider(p: &Arc<dyn TableProvider>) -> Option<&StatisticsProvider> {
    if let Some(p) = p.as_any().downcast_ref::<StatisticsProvider>() {
        return Some(p);
    }
    if let Some(p) = p
        .as_any()
        .downcast_ref::<super::super::constraints::ConstrainedProvider>()
    {
        return statistics_provider(&p.inner);
    }
    None
}
#[async_trait::async_trait]
impl TableProvider for StatisticsProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.inner.schema()
    }
    fn table_type(&self) -> TableType {
        self.inner.table_type()
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }
    fn statistics(&self) -> Option<Statistics> {
        let mut s = Statistics::new_unknown(self.schema().as_ref());
        s.num_rows = self
            .source
            .estimated_rows
            .map(|x| Precision::Inexact(x as usize))
            .unwrap_or(Precision::Absent);
        s.total_byte_size = self
            .source
            .estimated_bytes
            .map(|x| Precision::Inexact(x as usize))
            .unwrap_or(Precision::Absent);
        for (i, f) in self.schema().fields().iter().enumerate() {
            if let Some(c) = self.source.columns.get(f.name()) {
                s.column_statistics[i].distinct_count = c
                    .estimated_distinct
                    .map(|x| Precision::Inexact(x as usize))
                    .unwrap_or(Precision::Absent);
                if let Some(rows) = self.source.estimated_rows {
                    s.column_statistics[i].null_count = Precision::Inexact(
                        (rows * c.nulls as f64 / c.observations.max(1) as f64) as usize,
                    );
                }
            }
        }
        Some(s)
    }
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        // Cost hints are not a claim that the inner source enforces predicates.
        // Residual filters remain in the plan. A pushed limit could precede them.
        self.inner.scan(state, projection, &[], None).await
    }
}
