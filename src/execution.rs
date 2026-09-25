//! Optional hand-off to a caller-owned SQL session. No driver or data conversion.
//!
//! The caller chooses the session, obtains schema metadata, compiles the query,
//! then calls [`execute`]. Use the same schema/session snapshot across those steps.
//! Connection pools, transactions, UDFs, cancellation, and cache policy belong to
//! the adapter. This is a single-engine boundary, not a federated coordinator.
use crate::compiler::CompiledSql;
pub use crate::ir::rel::sql::SqlDialect;
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
}

/// Validate the compiler protocol and dialect before handing SQL to a session.
/// Engine routing stays with the caller: matching dialects do not imply that
/// two connections refer to the same database or schema.
pub async fn execute<'session, S: SqlSession>(
    session: &'session mut S,
    query: &CompiledSql,
) -> Result<S::Output<'session>, ExecutionError<S::Error>> {
    if query.version != 1 {
        return Err(ExecutionError::Version(query.version));
    }
    if query.dialect != session.dialect().name() {
        return Err(ExecutionError::Dialect {
            query: query.dialect.clone(),
            session: session.dialect().name(),
        });
    }
    session.query(query).await.map_err(ExecutionError::Driver)
}
