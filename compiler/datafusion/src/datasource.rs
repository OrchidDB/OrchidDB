//! Logical source metadata. These types have no scan or execute methods.
use arrow::{datatypes::SchemaRef, record_batch::RecordBatch};
use datafusion_common::{Constraints, DataFusionError, Result};
use datafusion_expr::{LogicalPlan, TableSource, TableType};
use std::{any::Any, borrow::Cow, sync::Arc};
use tokio::sync::RwLock;
pub use datafusion_expr::TableSource as TableProvider;

pub fn provider_as_source(provider: Arc<dyn TableSource>) -> Arc<dyn TableSource> { provider }
pub fn source_as_provider(source: &Arc<dyn TableSource>) -> Result<Arc<dyn TableSource>> { Ok(source.clone()) }

/// Immutable-by-convention Arrow inputs owned by Orchid's SQL transport.
/// Locks retain the existing batch-access contract; no DataFusion operator reads them.
#[derive(Debug)]
pub struct MemTable {
    schema: SchemaRef,
    pub batches: Vec<Arc<RwLock<Vec<RecordBatch>>>>,
    constraints: Constraints,
}
impl MemTable {
    pub fn try_new(schema: SchemaRef, partitions: Vec<Vec<RecordBatch>>) -> Result<Self> {
        for batch in partitions.iter().flatten() {
            if !schema.contains(&batch.schema()) {
                return Err(DataFusionError::Plan("Arrow input schema does not match its declared table schema".into()));
            }
        }
        Ok(Self { schema, batches: partitions.into_iter().map(|p| Arc::new(RwLock::new(p))).collect(), constraints: Constraints::default() })
    }
    pub fn with_constraints(mut self, constraints: Constraints) -> Self { self.constraints = constraints; self }
}
impl TableSource for MemTable {
    fn as_any(&self) -> &dyn Any { self }
    fn schema(&self) -> SchemaRef { self.schema.clone() }
    fn constraints(&self) -> Option<&Constraints> { Some(&self.constraints) }
}

#[derive(Debug)]
pub struct EmptyTable { schema: SchemaRef }
impl EmptyTable { pub fn new(schema: SchemaRef) -> Self { Self { schema } } }
impl TableSource for EmptyTable {
    fn as_any(&self) -> &dyn Any { self }
    fn schema(&self) -> SchemaRef { self.schema.clone() }
}
pub mod empty { pub use super::EmptyTable; }

#[derive(Debug)]
pub struct ViewTable { plan: LogicalPlan, definition: Option<String> }
impl ViewTable {
    pub fn new(plan: LogicalPlan, definition: Option<String>) -> Self { Self { plan, definition } }
    pub fn definition(&self) -> Option<&str> { self.definition.as_deref() }
}
impl TableSource for ViewTable {
    fn as_any(&self) -> &dyn Any { self }
    fn schema(&self) -> SchemaRef { Arc::new(self.plan.schema().as_arrow().clone()) }
    fn table_type(&self) -> TableType { TableType::View }
    fn get_logical_plan(&self) -> Option<Cow<'_, LogicalPlan>> { Some(Cow::Borrowed(&self.plan)) }
}

pub mod cte_worktable {
    use super::*;
    #[derive(Debug)]
    pub struct CteWorkTable { name: String, schema: SchemaRef }
    impl CteWorkTable {
        pub fn new(name: &str, schema: SchemaRef) -> Self { Self { name: name.into(), schema } }
        pub fn name(&self) -> &str { &self.name }
        pub fn schema(&self) -> SchemaRef { self.schema.clone() }
    }
    impl TableSource for CteWorkTable {
        fn as_any(&self) -> &dyn Any { self }
        fn schema(&self) -> SchemaRef { self.schema.clone() }
        fn table_type(&self) -> TableType { TableType::Temporary }
    }
}
