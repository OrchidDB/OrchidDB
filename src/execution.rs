//! Optional hand-off to a caller-owned SQL session. No driver or data conversion.
//!
//! The caller chooses the session, obtains schema metadata, compiles the query,
//! then calls [`execute`]. Use the same schema/session snapshot across those steps.
//! Connection pools, transactions, UDFs, cancellation, and cache policy belong to
//! the adapter. This is a single-engine boundary, not a federated coordinator.
pub mod exchange;
pub mod stream;
#[cfg(feature = "trino")]
pub mod trino;

/// The only relational execution environments. Remote APIs are sources, not engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutableEngine {
    #[serde(rename = "duckdb")]
    DuckDb,
    Trino,
}
impl ExecutableEngine {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "duckdb" => Ok(Self::DuckDb),
            "trino" => Ok(Self::Trino),
            _ => Err(format!(
                "unsupported executable engine {name:?}; expected duckdb or trino"
            )),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::DuckDb => "duckdb",
            Self::Trino => "trino",
        }
    }
}

use crate::compiler::CompiledSql;
pub use crate::ir::rel::sql::{DialectAdapter, EngineAdapter, SqlDialect};
pub use arrow::record_batch::{RecordBatch, RecordBatchReader};

/// An application-owned session. Results can borrow it (for example a cursor),
/// and expose native Arrow record batches. The compiler never buffers or converts results.
/// Retained batches own reference-counted buffers and remain valid after advancing
/// or dropping the reader. Drivers may materialize execution internally; this
/// interface guarantees batch transport, not streaming database execution.
///
/// Futures need not be `Send`, allowing thread-confined embedded connections.
/// Implementations own cancellation and resource cleanup, including on drop.
#[allow(async_fn_in_trait)]
pub trait SqlSession {
    type Error;
    type Output<'session>: RecordBatchReader
    where
        Self: 'session;

    fn dialect(&self) -> SqlDialect;

    /// Execute exactly the supplied SQL without implicit setup or commits.
    /// Copy SQL into driver-owned storage if a cursor needs to retain it.
    async fn query<'session>(
        &'session mut self,
        query: &CompiledSql,
    ) -> Result<Self::Output<'session>, Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError<E> {
    #[error("unsupported compiled SQL version: {0}")]
    Version(u32),
    #[error("compiled SQL dialect {query} does not match session dialect {session}")]
    Dialect {
        query: String,
        session: &'static str,
    },
    #[error("SQL execution: {0}")]
    Driver(E),
    #[error("query requires federated execution")]
    Federation,
    #[error("unsupported executable engine: {0}; expected duckdb or trino")]
    UnsupportedEngine(String),
}

/// Validate the compiler protocol and dialect before handing SQL to a session.
/// Engine routing stays with the caller: matching dialects do not imply that
/// two connections refer to the same database or schema.
pub async fn execute<'session, S: SqlSession>(
    session: &'session mut S,
    query: &CompiledSql,
) -> Result<S::Output<'session>, ExecutionError<S::Error>> {
    ExecutableEngine::parse(&query.dialect)
        .map_err(|_| ExecutionError::UnsupportedEngine(query.dialect.clone()))?;
    if query.version != 1 {
        return Err(ExecutionError::Version(query.version));
    }
    if query.dialect != session.dialect().name() {
        return Err(ExecutionError::Dialect {
            query: query.dialect.clone(),
            session: session.dialect().name(),
        });
    }
    if !query.transfers.is_empty() {
        return Err(ExecutionError::Federation);
    }
    session.query(query).await.map_err(ExecutionError::Driver)
}
