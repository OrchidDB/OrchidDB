//! Structured execution diagnoses retained across DataFusion and public APIs.
use std::error::Error;
mod folding;
pub(crate) use folding::constant_folding_enabled;
#[doc(hidden)]
pub use folding::without_constant_folding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RuntimeDiagnosis {
    ArgumentType,
    NumberOutOfRange,
    InvalidType,
    MapKeyType,
    InvalidValue,
    DeleteConnectedNode,
    MergeReadOwnWrites,
    NegativeIntegerArgument,
    InvalidSliceArgument,
    DeletedEntityAccess,
    InvalidPropertyType,
}

impl RuntimeDiagnosis {
    pub fn classification(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::DeletedEntityAccess => return ("EntityNotFound", "DeletedEntityAccess", "runtime"),
            Self::InvalidPropertyType => return ("TypeError", "InvalidPropertyType", "runtime"),
            Self::InvalidType => return ("TypeError", "InvalidArgumentType", "runtime"),
            Self::MapKeyType => return ("TypeError", "MapElementAccessByNonString", "runtime"),
            Self::InvalidValue => return ("TypeError", "InvalidArgumentValue", "runtime"),
            Self::DeleteConnectedNode => return ("ConstraintVerificationFailed", "DeleteConnectedNode", "runtime"),
            Self::MergeReadOwnWrites => return ("SemanticError", "MergeReadOwnWrites", "runtime"),
            Self::NegativeIntegerArgument => return ("SyntaxError", "NegativeIntegerArgument", "runtime"),
            Self::InvalidSliceArgument => return ("SyntaxError", "InvalidArgumentType", "runtime"),
            _ => {}
        }
        let detail = match self {
            Self::ArgumentType => "InvalidArgumentType",
            Self::NumberOutOfRange => "NumberOutOfRange",
            _ => unreachable!(),
        };
        ("ArgumentError", detail, "runtime")
    }
}

/// The message remains compatible with the string-returning engine APIs;
/// typed callers also receive the diagnosis made by the failing operator.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct QueryExecutionError {
    pub message: String,
    pub diagnosis: Option<RuntimeDiagnosis>,
}

impl QueryExecutionError {
    pub fn from_error(error: impl Error + 'static) -> Self {
        let message = error.to_string();
        let mut source: Option<&(dyn Error + 'static)> = Some(&error);
        let mut diagnosis = None;
        while let Some(error) = source {
            if matches!(error.downcast_ref::<crate::ir::catalog::CatalogError>(),
                Some(crate::ir::catalog::CatalogError::DeleteIntegrity(_))) {
                diagnosis = Some(RuntimeDiagnosis::DeleteConnectedNode);
                break;
            }
            if let Some(error) = error.downcast_ref::<Self>() {
                diagnosis = error.diagnosis;
                break;
            }
            if let Some(crate::ir::RuntimeError::Diagnosed { code, .. }) =
                error.downcast_ref::<crate::ir::RuntimeError>()
            {
                diagnosis = Some(*code);
                break;
            }
            source = error.source();
        }
        Self { message, diagnosis }
    }
}

impl From<String> for QueryExecutionError {
    fn from(message: String) -> Self {
        Self {
            message,
            diagnosis: None,
        }
    }
}
impl From<&str> for QueryExecutionError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

/// Internal conformance tooling: prepare on the runner's actual mapped catalog.
/// This is not a customer query API and does not execute the target query.
#[cfg(feature = "duckdb")]
#[doc(hidden)]
pub async fn conformance_sql(
    connection: duckdb::Connection,
    mapping: std::sync::Arc<crate::ir::rel::mapping::GraphMapping>,
    plan: &crate::ir::plan::GraphPlan,
    procedures: std::sync::Arc<crate::ir::procedures::ProcedureCatalog>,
) -> Result<serde_json::Value, String> {
    use crate::ir::rel::{dag::DagSession, sql::DuckDbExecutor};
    use crate::engine::mapped_source;
    use std::sync::{Arc, Mutex};
    if crate::ir::exec::contains_mutation(&plan.root) {
        return Err("SQL report excludes mutating queries".into());
    }
    let mut executor = DuckDbExecutor::from_connection(connection);
    executor.set_language_functions(true).map_err(|e| e.to_string())?;
    let resources = DagSession::with_shared(Arc::new(Mutex::new(executor)), mapping.physical_table_names());
    let mut graph = mapped_source::attach(resources.shared_executor(), mapping)?;
    graph.procedures = procedures;
    let stats = crate::ir::rel::runtime::inspect_with_session(plan, &graph, &resources).await?;
    let full = stats.sql_queries.len() == 1 && stats.datafusion_operators == 0;
    Ok(serde_json::json!({
        "schema_mode":"conformance_runner_mapping", "capture":"prepared_not_executed",
        "sql": if full {stats.sql_queries.first().cloned()} else {None},
        "regions": if full {Vec::<serde_json::Value>::new()} else {stats.sql_queries.iter().map(|sql| serde_json::json!({"sql":sql})).collect()},
        "residual_operators":stats.datafusion_operators,
        "logical_plan":stats.logical_plan, "physical_plan":stats.physical_plan,
        "execution_kind":if full {"sql"} else {"hybrid"}, "fully_lowered":full,
        "validation":"prepared_with_conformance_mapping",
    }))
}
