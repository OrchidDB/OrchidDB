//! Engine-independent operator lookup and typed calls, separate from language
//! semantics and SQL rendering (the operator-table / dialect split in Calcite).
//!
//! Language-defined functions resolve first. Remaining ordinary function names
//! resolve against the selected engine's catalog; the grammar never enumerates
//! engine function names. Table functions are catalogued but are not expressions.

mod call;
mod mappings;
pub use mappings::FunctionRegistry;
#[cfg(feature = "duckdb")]
mod duckdb;
mod typed_cast;
mod types;
pub use call::{ENGINE_FUNCTION_PREFIX, native_aggregate, native_scalar};
#[cfg(feature = "duckdb")]
pub use duckdb::DuckDbCatalog;
pub(crate) use typed_cast::{ENGINE_CAST_FUNCTION, typed_argument_cast};

use arrow::datatypes::DataType;
use datafusion::common::{DFSchema, Result};
use datafusion::logical_expr::Expr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Scalar,
    Aggregate,
    Macro,
    Table,
    Other,
}

/// One catalog overload. SQL type spellings deliberately retain polymorphic
/// types, decimal parameters and extension types instead of guessing Arrow types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionOverload {
    pub name: String,
    pub kind: FunctionKind,
    pub parameter_types: Vec<String>,
    pub varargs: Option<String>,
    pub return_type: Option<String>,
    pub stability: Option<String>,
}

/// A backend owns overload resolution and return-type inference. Binding must
/// not evaluate user calls (including volatile functions and side effects).
pub trait OperatorTable: Send + Sync {
    fn engine(&self) -> &str;
    /// SQL function identity after resolving a language-facing alias.
    fn target_name(&self, name: &str) -> String {
        name.to_ascii_lowercase()
    }
    /// Preserve a code-declared overload with explicit argument casts before
    /// binding and rendering. Catalog-only tables leave arguments unchanged.
    fn prepare_args(
        &self,
        _name: &str,
        _kind: FunctionKind,
        args: Vec<Expr>,
        _schema: &DFSchema,
    ) -> Result<Vec<Expr>> {
        Ok(args)
    }
    fn overloads(&self, name: &str) -> &[FunctionOverload];
    fn bind(
        &self,
        name: &str,
        kind: FunctionKind,
        args: &[Expr],
        schema: &DFSchema,
    ) -> Result<DataType>;
}

thread_local! {
    static ACTIVE_TABLE: std::cell::RefCell<Option<std::sync::Arc<dyn OperatorTable>>> =
        std::cell::RefCell::new(None);
}

/// Select the operator table for synchronous parsing/planning and relational
/// lowering inside `operation`. Nested scopes and panics restore the previous
/// selection. Selection is thread-local: do not return an async future expecting
/// it to inherit this scope. Prepare/execute the resulting plan afterwards.
pub fn with_operator_table<T>(
    table: std::sync::Arc<dyn OperatorTable>,
    operation: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<std::sync::Arc<dyn OperatorTable>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE_TABLE.with(|active| *active.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(ACTIVE_TABLE.with(|active| active.replace(Some(table))));
    operation()
}

pub(super) fn selected_operator_table() -> Result<std::sync::Arc<dyn OperatorTable>> {
    if let Some(table) = ACTIVE_TABLE.with(|active| active.borrow().clone()) {
        return Ok(table);
    }
    #[cfg(feature = "duckdb")]
    {
        Ok(duckdb::default_catalog()?)
    }
    #[cfg(not(feature = "duckdb"))]
    {
        Err(datafusion::common::DataFusionError::NotImplemented(
            "a function operator table must be selected without the duckdb feature".into(),
        ))
    }
}

pub fn is_native_aggregate(name: &str) -> bool {
    if let Ok(catalog) = selected_operator_table() {
        return catalog
            .overloads(name)
            .iter()
            .any(|f| f.kind == FunctionKind::Aggregate)
            && !catalog
                .overloads(name)
                .iter()
                .any(|f| matches!(f.kind, FunctionKind::Scalar | FunctionKind::Macro));
    }
    false
}

pub(crate) fn is_registered_function(name: &str) -> bool {
    selected_operator_table().is_ok_and(|catalog| !catalog.overloads(name).is_empty())
}

pub(crate) fn is_volatile_function(name: &str) -> bool {
    selected_operator_table().is_ok_and(|catalog| catalog.overloads(name).iter()
        .any(|overload| overload.stability.as_deref().is_some_and(|stability| stability.eq_ignore_ascii_case("volatile"))))
}
