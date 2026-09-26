use super::*;
use arrow::datatypes::SchemaRef;
use datafusion::{
    catalog::{Session, TableProvider},
    common::{Constraints, Result, Statistics},
    logical_expr::{Expr, LogicalPlan, TableProviderFilterPushDown, TableType},
    physical_plan::ExecutionPlan,
};
use std::{any::Any, borrow::Cow, sync::Arc};

#[derive(Debug)]
pub(crate) struct ConstrainedProvider {
    pub inner: Arc<dyn TableProvider>,
    pub table: String,
    pub facts: Vec<Constraint>,
    constraints: Constraints,
}
pub(crate) fn bind(
    table: &str,
    inner: Arc<dyn TableProvider>,
    catalog: &ConstraintCatalog,
    scope: Option<&str>,
) -> Result<Arc<dyn TableProvider>> {
    let facts: Vec<_> = catalog
        .tables
        .get(table)
        .into_iter()
        .flatten()
        .filter(|c| c.evidence.usable(scope))
        .cloned()
        .collect();
    if facts.is_empty() {
        return Ok(inner);
    }
    let schema = inner.schema();
    for c in &facts {
        let columns = match &c.fact {
            Fact::Unique { columns, .. }
            | Fact::NonNull { columns }
            | Fact::ForeignKey { columns, .. } => columns.iter().collect::<Vec<_>>(),
            Fact::FunctionalDependency {
                determinant,
                dependent,
            } => determinant.iter().chain(dependent).collect(),
        };
        for column in columns {
            schema.field_with_name(column).map_err(|_|datafusion::common::DataFusionError::Plan(format!("constraint {} references missing column {table}.{column} in current source schema",c.name)))?;
        }
    }
    // DataFusion 53's generic DISTINCT rule treats nullable UNIQUE and some
    // fan-out dependencies as row uniqueness. Keep supplied proofs in our
    // analyzer instead of exposing those weaker facts to that rule.
    let constraints = Vec::new();
    Ok(Arc::new(ConstrainedProvider {
        inner,
        table: table.into(),
        facts,
        constraints: Constraints::new_unverified(constraints),
    }))
}
#[async_trait::async_trait]
impl TableProvider for ConstrainedProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.inner.schema()
    }
    fn constraints(&self) -> Option<&Constraints> {
        Some(&self.constraints)
    }
    fn table_type(&self) -> TableType {
        self.inner.table_type()
    }
    fn get_logical_plan(&self) -> Option<Cow<'_, LogicalPlan>> {
        // Keep facts attached to this source boundary. Unconstrained views are
        // still inlined by the mapping provider; constrained views retain their
        // own row domain and delegate execution to the wrapped provider.
        None
    }
    fn get_table_definition(&self) -> Option<&str> {
        self.inner.get_table_definition()
    }
    fn get_column_default(&self, column: &str) -> Option<&Expr> {
        self.inner.get_column_default(column)
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        self.inner.supports_filters_pushdown(filters)
    }
    fn statistics(&self) -> Option<Statistics> {
        self.inner.statistics()
    }
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        self.inner.scan(state, projection, filters, limit).await
    }
}
