//! SPARQL uses the same connection and transaction owner as the other languages.
use super::*;
use crate::rdf_engine::{RdfSession, SparqlResults};

impl GraphEngine {
    fn rdf_catalog(&self) -> EngineResult<Arc<crate::ir::rel::rdf::RdfDatasetMapping>> {
        let mapping = self
            .mapping
            .as_ref()
            .ok_or("SPARQL datasets require a mapped catalog")?;
        Ok(Arc::new(mapping.rdf_mapping()))
    }

    /// Read a declared RDF dataset over the engine's application tables.
    pub async fn sparql_dataset(
        &mut self,
        query: &str,
        dataset: &str,
    ) -> EngineResult<QueryResult> {
        let plan = crate::language::sparql::SparqlPlanner::new(dataset)
            .plan_str(query)
            .map_err(|e| e.to_string())?;
        self.rdf_catalog()?;
        self.execute_plan(&plan).await
    }
    pub async fn sparql_query(
        &mut self,
        query: &str,
        dataset: &str,
    ) -> EngineResult<SparqlResults> {
        crate::rdf_engine::decode_results(&self.sparql_dataset(query, dataset).await?.returned)
    }
    pub async fn sparql_update(
        &mut self,
        query: &str,
        dataset: &str,
        base: Option<&str>,
    ) -> EngineResult<()> {
        self.run_rdf(query, dataset, base, true, None)
            .await
            .map(|_| ())
    }
    pub async fn sparql_sql(&self, query: &str, dataset: &str) -> EngineResult<String> {
        let resources = crate::ir::rel::dag::DagSession::new(self.sql_timeout);
        let context = RdfSession {
            resources: &resources,
            mapping: self.rdf_catalog()?,
            dataset: dataset.into(),
            scalar_registered: false,
            last_query_stats: None,
        };
        context.sql(query).await
    }
    pub fn into_executor(mut self) -> sql::DuckDbExecutor {
        let connection = std::mem::replace(
            &mut self.storage,
            Connection::open_in_memory().expect("open placeholder connection"),
        );
        self.in_transaction = false;
        sql::DuckDbExecutor::from_connection(connection)
    }
    pub(super) async fn run_rdf(
        &mut self,
        query: &str,
        dataset: &str,
        base: Option<&str>,
        update: bool,
        catalog: Option<Arc<crate::ir::rel::rdf::RdfDatasetMapping>>,
    ) -> EngineResult<Option<QueryResult>> {
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        let mapping = match catalog {
            Some(mapping) => mapping,
            None => self.rdf_catalog()?,
        };
        let placeholder = Connection::open_in_memory().map_err(|e| e.to_string())?;
        let storage = std::mem::replace(&mut self.storage, placeholder);
        let mut connection = MappedConnectionLease {
            target: &mut self.storage,
            executor: sql::DuckDbExecutor::from_connection(storage),
        };
        if let Some(timeout) = self.sql_timeout {
            connection.executor.set_timeouts(timeout, timeout);
        }
        let automatic = !self.in_transaction;
        if automatic {
            connection.executor.begin().map_err(|e| e.to_string())?;
        }
        // Dropping a future rolls back an automatic transaction and poisons an explicit one.
        if !automatic {
            self.failed_transaction = true;
        }
        let shared = Arc::new(std::sync::Mutex::new(std::mem::take(
            &mut connection.executor,
        )));
        let mut lease = MappedExecutorLease {
            target: &mut connection.executor,
            shared: shared.clone(),
            automatic,
            finished: false,
        };
        let mut resources = crate::ir::rel::dag::DagSession::with_shared(
            shared.clone(),
            mapping.physical_table_names(),
        );
        resources.region_session = self.dag_session.region_session.clone();
        let mut context = RdfSession {
            resources: &resources,
            mapping,
            dataset: dataset.into(),
            scalar_registered: false,
            last_query_stats: None,
        };
        let result = if update {
            context.update(query, base).await.map(|_| None)
        } else {
            context.sparql(query).await.map(|returned| {
                Some(QueryResult {
                    returned,
                    backend: ExecutionBackend::Hybrid,
                    stats: context.last_query_stats.take().unwrap_or_default(),
                })
            })
        };
        drop(context);
        drop(resources);
        let result = if automatic && result.is_ok() {
            shared
                .lock()
                .map_err(|e| e.to_string())?
                .commit()
                .map_err(|e| e.to_string())
                .and(result)
        } else {
            result
        };
        lease.finished = result.is_ok();
        drop(lease);
        drop(connection);
        if update && result.is_ok() { self.invalidate_statistics(); }
        if result.is_ok() {
            self.failed_transaction = false;
        }
        result
    }
}
