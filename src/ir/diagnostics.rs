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
    pub fn from_classification(kind: &str, detail: &str, phase: &str) -> Option<Self> {
        [
            Self::ArgumentType,
            Self::NumberOutOfRange,
            Self::InvalidType,
            Self::MapKeyType,
            Self::InvalidValue,
            Self::DeleteConnectedNode,
            Self::MergeReadOwnWrites,
            Self::NegativeIntegerArgument,
            Self::InvalidSliceArgument,
            Self::DeletedEntityAccess,
            Self::InvalidPropertyType,
        ]
        .into_iter()
        .find(|code| code.classification() == (kind, detail, phase))
    }
    pub fn classification(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::DeletedEntityAccess => {
                return ("EntityNotFound", "DeletedEntityAccess", "runtime");
            }
            Self::InvalidPropertyType => return ("TypeError", "InvalidPropertyType", "runtime"),
            Self::InvalidType => return ("TypeError", "InvalidArgumentType", "runtime"),
            Self::MapKeyType => return ("TypeError", "MapElementAccessByNonString", "runtime"),
            Self::InvalidValue => return ("TypeError", "InvalidArgumentValue", "runtime"),
            Self::DeleteConnectedNode => {
                return (
                    "ConstraintVerificationFailed",
                    "DeleteConnectedNode",
                    "runtime",
                );
            }
            Self::MergeReadOwnWrites => return ("SemanticError", "MergeReadOwnWrites", "runtime"),
            Self::NegativeIntegerArgument => {
                return ("SyntaxError", "NegativeIntegerArgument", "runtime");
            }
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

const SQL_ERROR_PREFIX: &str = "ORCHIDDB_QUERY_ERROR_V1:";

#[derive(serde::Deserialize)]
struct SqlErrorEnvelope {
    version: u8,
    message: String,
    diagnosis: Option<RuntimeDiagnosis>,
}

impl QueryExecutionError {
    /// DuckDB's C scalar API carries only an error string. Transport the
    /// operator's machine diagnosis explicitly instead of classifying prose.
    pub(crate) fn sql_error(&self) -> String {
        format!(
            "{SQL_ERROR_PREFIX}{}",
            serde_json::json!({
                "version":1,"message":self.message,"diagnosis":self.diagnosis
            })
        )
    }
    fn from_sql_error(message: &str) -> Option<Self> {
        for (_, suffix) in message
            .match_indices(SQL_ERROR_PREFIX)
            .map(|(offset, marker)| (offset, &message[offset + marker.len()..]))
        {
            let Some(Ok(error)) = serde_json::Deserializer::from_str(suffix)
                .into_iter::<SqlErrorEnvelope>()
                .next()
            else {
                continue;
            };
            if error.version == 1 {
                return Some(Self {
                    message: error.message,
                    diagnosis: error.diagnosis,
                });
            }
        }
        None
    }
    pub fn from_error(error: impl Error + 'static) -> Self {
        let message = error.to_string();
        let mut source: Option<&(dyn Error + 'static)> = Some(&error);
        let mut diagnosis = None;
        while let Some(error) = source {
            if matches!(
                error.downcast_ref::<crate::ir::catalog::CatalogError>(),
                Some(crate::ir::catalog::CatalogError::DeleteIntegrity(_))
            ) {
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
        if diagnosis.is_none() {
            if let Some(transported) = Self::from_sql_error(&message) {
                return transported;
            }
        }
        Self { message, diagnosis }
    }
}

impl From<String> for QueryExecutionError {
    fn from(message: String) -> Self {
        Self::from_sql_error(&message).unwrap_or(Self {
            message,
            diagnosis: None,
        })
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
    conformance_sql_with_estimates(connection, mapping, plan, procedures, false).await
}

/// Internal plan-audit entry point. EXPLAIN (never EXPLAIN ANALYZE) binds and
/// optimizes SQL against the prepared fixture without opening query results.
#[cfg(feature = "duckdb")]
#[doc(hidden)]
pub async fn conformance_sql_with_estimates(
    connection: duckdb::Connection,
    mapping: std::sync::Arc<crate::ir::rel::mapping::GraphMapping>,
    plan: &crate::ir::plan::GraphPlan,
    procedures: std::sync::Arc<crate::ir::procedures::ProcedureCatalog>,
    estimates: bool,
) -> Result<serde_json::Value, String> {
    use crate::engine::mapped_source;
    use crate::ir::rel::{dag::DagSession, sql::DuckDbExecutor};
    use std::sync::{Arc, Mutex};
    if crate::ir::exec::contains_mutation(&plan.root) {
        return Err("SQL report excludes mutating queries".into());
    }
    let mut executor = DuckDbExecutor::from_connection(connection);
    executor
        .set_language_functions(true)
        .map_err(|e| e.to_string())?;
    let resources = DagSession::with_shared(
        Arc::new(Mutex::new(executor)),
        mapping.physical_table_names(),
    );
    let mut graph = mapped_source::attach(resources.shared_executor(), mapping)?;
    graph.procedures = procedures;
    let stats = crate::ir::rel::runtime::inspect_with_session(plan, &graph, &resources).await?;
    let full = stats.sql_queries.len() == 1 && stats.boundary_operators == stats.output_decode_operators;
    let mut output = serde_json::json!({
        "schema_mode":"conformance_runner_mapping", "capture":"prepared_not_executed",
        "sql": if full {stats.sql_queries.first().cloned()} else {None},
        "regions": stats.sql_queries.iter().map(|sql| serde_json::json!({"sql":sql})).collect::<Vec<_>>(),
        "datafusion_operators":stats.datafusion_operators,
        "boundary_operators":stats.boundary_operators,
        "residual_operators":stats.boundary_operators,
        "residual_compute_operators":stats.boundary_operators.saturating_sub(stats.output_decode_operators),
        "output_decode_operators":stats.output_decode_operators, "sql_boundaries":stats.sql_boundaries,
        "runtime_input_tables":stats.runtime_inputs.iter().map(|t|t.name.clone()).collect::<Vec<_>>(),
        "logical_plan":stats.logical_plan, "physical_plan":stats.physical_plan,
        "execution_kind":if full {"sql"} else {"hybrid"}, "fully_lowered":full,
        "validation":"prepared_with_conformance_mapping",
    });
    if estimates {
        let shared = resources.shared_executor();
        let mut executor = shared.lock().map_err(|e| e.to_string())?;
        let connection = executor
            .source_query_connection()
            .map_err(|e| e.to_string())?;
        // Runtime inputs have known schemas but unknown rows at inspection.
        // Bind their interfaces temporarily; never execute the native producer.
        let mut installed: Vec<String> = Vec::new();
        for table in &stats.runtime_inputs {
            let ddl = crate::ir::rel::sql::create_table_sql(
                crate::ir::rel::sql::SqlDialect::DuckDb,
                table,
            )
            .map_err(|e| e.to_string())?;
            if let Err(error) = connection.execute_batch(&ddl) {
                for name in &installed {
                    let _ = connection.execute_batch(&format!(
                        "DROP TABLE {}",
                        crate::ir::rel::sql::SqlDialect::DuckDb.quote_ident(name)
                    ));
                }
                return Err(error.to_string());
            }
            installed.push(table.name.clone());
        }
        let plans = stats.sql_queries.iter().map(|sql| {
            let explain = (|| -> Result<serde_json::Value, String> {
                let mut statement = connection.prepare(&format!("EXPLAIN (FORMAT JSON) {sql}"))
                    .map_err(|e| e.to_string())?;
                let json: String = statement.query_row([], |row| row.get(1)).map_err(|e| e.to_string())?;
                serde_json::from_str(&json).map_err(|e| e.to_string())
            })();
            match explain {
                Ok(plan) => serde_json::json!({"sql":sql,"plan":plan,"runtime_input_estimates_unknown":stats.runtime_inputs.iter().any(|table|sql.contains(&table.name))}),
                Err(error) => serde_json::json!({"sql":sql,"error":error}),
            }
        }).collect::<Vec<_>>();
        for name in installed {
            connection
                .execute_batch(&format!(
                    "DROP TABLE {}",
                    crate::ir::rel::sql::SqlDialect::DuckDb.quote_ident(&name)
                ))
                .map_err(|e| e.to_string())?;
        }
        output["sql_plans"] = serde_json::json!(plans);
        output["estimate_method"] =
            serde_json::json!("EXPLAIN (FORMAT JSON); target query not executed");
    }
    Ok(output)
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    #[test]
    fn machine_envelope_is_versioned_and_existing_diagnoses_win() {
        let error = QueryExecutionError {
            message: "exact operator message".into(),
            diagnosis: Some(RuntimeDiagnosis::NegativeIntegerArgument),
        };
        let wire = format!(
            "Invalid Input Error: {}\nLINE 1: SELECT ...",
            error.sql_error()
        );
        let decoded = QueryExecutionError::from(wire.clone());
        assert_eq!(decoded.message, error.message);
        assert_eq!(decoded.diagnosis, error.diagnosis);
        let typed = QueryExecutionError::from_error(QueryExecutionError {
            message: wire,
            diagnosis: Some(RuntimeDiagnosis::ArgumentType),
        });
        assert_eq!(typed.diagnosis, Some(RuntimeDiagnosis::ArgumentType));
        assert!(
            QueryExecutionError::from("SKIP/LIMIT require a non-negative integer")
                .diagnosis
                .is_none()
        );
        assert!(QueryExecutionError::from(format!("{SQL_ERROR_PREFIX}{{\"version\":2,\"message\":\"future\",\"diagnosis\":\"NegativeIntegerArgument\"}}")).diagnosis.is_none());
    }
}
