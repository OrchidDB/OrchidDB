//! Execute graph queries through a caller-owned connection and registered schema.
pub use arrow;
pub use arrow::record_batch::{RecordBatch, RecordBatchReader};
/// SQL work is an engine-adapter contract, never a customer compilation result.
pub use orchiddb::compiler::CompiledSql as SqlWork;
pub use orchiddb::compiler::{Authorization, PermissionRelation, PermissionScope};
pub use orchiddb::execution::{SqlDialect, SqlSession};
pub use orchiddb::session::{Query, Schema};
mod statistics;

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("graph query: {0}")]
    Query(String),
    #[error(transparent)]
    Execution(orchiddb::execution::ExecutionError<E>),
}

/// Borrows or owns the supplied adapter; graph schema is retained on this connection.
pub struct Connection<S> {
    session: S,
    schema: Schema,
    statistics: statistics::Statistics,
}
impl<S: SqlSession> Connection<S> {
    pub fn new(session: S, schema: Schema) -> Self {
        Self {
            session,
            schema,
            statistics: Default::default(),
        }
    }
    pub async fn query(&mut self, query: Query) -> Result<S::Output<'_>, Error<S::Error>> {
        let request = self
            .schema
            .request(self.session.dialect().name(), &query)
            .map_err(Error::Query)?;
        let work = self
            .statistics
            .compile_plan(serde_json::to_value(request).map_err(|e| Error::Query(e.to_string()))?)
            .await
            .map_err(Error::Query)?;
        orchiddb::execution::execute(&mut self.session, &work)
            .await
            .map_err(Error::Execution)
    }
    pub async fn generate_statistics<C: statistics::StatisticsCollector>(
        &mut self,
        collector: C,
    ) -> Result<serde_json::Value, String> {
        let request = self
            .schema
            .request(self.session.dialect().name(), &Query::cypher("RETURN 1"))?;
        self.statistics
            .generate(
                serde_json::to_value(request).map_err(|e| e.to_string())?,
                collector,
            )
            .await?;
        Ok(self.statistics.report().cloned().unwrap_or_default())
    }
    pub fn save_statistics(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        self.statistics.save(path)
    }
    pub async fn load_statistics(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), String> {
        self.statistics.load(path).await
    }
    pub async fn clear_statistics(&mut self) -> Result<(), String> {
        self.statistics.clear().await
    }
    pub fn into_session(self) -> S {
        self.session
    }
}
pub use statistics::StatisticsCollector;

/// Engine adapter for federated execution. The connection owns query routing.
pub use orchiddb::federation::Session as EngineSession;
pub struct FederatedConnection {
    schema: Schema,
    dialect: String,
    sessions: std::collections::BTreeMap<String, Box<dyn EngineSession>>,
}
impl FederatedConnection {
    pub fn new(
        schema: Schema,
        target: &str,
        sessions: std::collections::BTreeMap<String, Box<dyn EngineSession>>,
    ) -> Result<Self, String> {
        let dialect = sessions
            .get(target)
            .ok_or("Missing execution engine")?
            .dialect()
            .to_owned();
        let mut config = serde_json::to_value(schema).map_err(|e| e.to_string())?;
        config["execution_engine"] = target.into();
        Ok(Self {
            schema: Schema::from_value(config)?,
            dialect,
            sessions,
        })
    }
    pub async fn query(&mut self, query: Query) -> Result<Vec<RecordBatch>, String> {
        let work = orchiddb::compiler::compile(self.schema.request(&self.dialect, &query)?).await?;
        orchiddb::federation::execute(&work, &mut self.sessions).await
    }
}
#[cfg(any(feature = "quickwit", feature = "elasticsearch"))]
pub mod engines {
    pub use orchiddb::remote::transport::HttpSession;
}
