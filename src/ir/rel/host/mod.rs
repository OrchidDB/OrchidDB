//! Caller-owned relational access for existing graph storage and residual kernels.
//! Implementations must preserve the caller's transaction and Arrow types. A
//! borrowed host is synchronous; shared hosts must additionally be Send + Sync.
//! Thread-affine hosts should share only a checked execution token, never an
//! unsafely Send/Sync borrowed connection or ClientContext pointer.
use arrow::array::RecordBatch;
use datafusion::common::ScalarValue;
use std::{fmt::Debug, sync::Arc};
#[cfg(feature = "duckdb")]
pub(crate) mod legacy;
pub mod managed;
pub mod mapped_source;
pub mod mapped_storage;
pub mod observation;

#[derive(Debug, Clone)]
pub struct HostRelation {
    pub name: String,
    pub batch: RecordBatch,
}
#[derive(Debug, Clone)]
pub struct HostRequest {
    pub sql: String,
    /// One-based $1, $2, ... placeholders, bound with their original Arrow type.
    pub parameters: Vec<ScalarValue>,
    /// Query-scoped named relations; no persistent or connection-global imports.
    pub relations: Vec<HostRelation>,
}
impl HostRequest {
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            parameters: vec![],
            relations: vec![],
        }
    }
    pub fn parameters(mut self, values: Vec<ScalarValue>) -> Self {
        self.parameters = values;
        self
    }
    pub fn relation(mut self, name: impl Into<String>, batch: RecordBatch) -> Self {
        self.relations.push(HostRelation {
            name: name.into(),
            batch,
        });
        self
    }
}
pub trait HostRelational: Debug {
    /// Return a typed empty batch for empty results, retaining the bound schema.
    fn query(&self, request: HostRequest) -> Result<RecordBatch, String>;
    fn execute(&self, request: HostRequest) -> Result<(), String>;
    /// Existing source-derived logical plans still execute in the selected host.
    fn execute_plan(
        &self,
        mapping: &super::mapping::GraphMapping,
        plan: datafusion::logical_expr::LogicalPlan,
    ) -> Result<(RecordBatch, Vec<String>), String> {
        let _ = mapping;
        let lowered = super::LoweredPlan {
            fields: plan
                .schema()
                .fields()
                .iter()
                .map(|f| f.name().clone())
                .collect(),
            plan,
            result_form: crate::ir::policy::ResultForm::RowSet,
            islands: Default::default(),
        };
        let sql = super::sql::unparse(&lowered, super::sql::SqlDialect::DuckDb)
            .map_err(|e| e.to_string())?;
        self.query(HostRequest::new(&sql))
            .map(|batch| (batch, vec![sql]))
    }
    /// Only the legacy scheduler uses this. Host extensions never create one.
    #[cfg(feature = "duckdb")]
    fn legacy_executor(&self) -> Option<Arc<std::sync::Mutex<super::sql::DuckDbExecutor>>> {
        None
    }
}
pub type SharedHost = Arc<dyn HostRelational + Send + Sync>;
