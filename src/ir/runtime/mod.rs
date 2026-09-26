//! Shared scalar semantics, row types and native DataFusion operator kernels.
//! Graph plans are compiled by `ir::rel::runtime`; this module does not execute
//! or recursively dispatch Graph IR plans.

use std::collections::BTreeMap;

use arrow::array::RecordBatch;

use crate::ir::catalog::CatalogError;
use crate::ir::policy::ResultForm;
use crate::ir::value::Value;

mod element_id;
pub(crate) mod expr;
pub(crate) mod ops;
pub(crate) mod output;
pub(crate) mod context;
pub(crate) mod scalar;
pub(crate) use scalar::is_known_function;

pub(crate) use crate::ir::runtime::expr::compare_values;
pub use crate::ir::runtime::expr::eval;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("{message}")]
    Diagnosed { code: crate::ir::diagnostics::RuntimeDiagnosis, message: String },
    #[error("catalog: {0}")]
    Catalog(#[from] CatalogError),
    #[error("type error: {0}")]
    Type(String),
    #[error("{0}")]
    Runtime(String),
    #[error("unbound binding `{0}`")]
    Unbound(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("execution limit exceeded: {0}")]
    ExecutionLimit(String),
}

pub type IrResult<T> = Result<T, RuntimeError>;

/// One row passed between DataFusion graph kernels. Bindings flow forward; each
/// operator may add, remove, or rewrite them.
#[derive(Debug, Clone)]
pub struct Row {
    pub bindings: BTreeMap<String, Value>,
    /// Gremlin traverser bulk. Defaults to 1; observed by `countBulk` and
    /// reset by `dedup`.
    pub bulk: u64,
}

impl Row {
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
            bulk: 1,
        }
    }

    pub fn with(mut self, key: impl Into<String>, value: Value) -> Self {
        self.bindings.insert(key.into(), value);
        self
    }

    pub fn get(&self, key: &str) -> Value {
        self.bindings.get(key).cloned().unwrap_or(Value::Null)
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

/// The Arrow-typed result of evaluating a `GraphReturn` boundary.
#[derive(Debug, Clone)]
pub struct ReturnedBatches {
    pub fields: Vec<String>,
    pub result_form: ResultForm,
    pub batch: RecordBatch,
}
