//! Read-only SPARQL queries over existing DuckDB RDF quad tables.
//!
//! `RdfGraphEngine` accepts an `RdfDatasetMapping` whose registered table
//! providers describe the source schemas. The table names also exist in the
//! supplied DuckDB executor. Queries use the general SPARQL planner, then
//! lower quad scans through the mapping and execute SQL against those tables.

use std::sync::Arc;

use crate::ir::catalog::PropertyGraph;
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::rel::rdf::RdfDatasetMapping;
use crate::ir::rel::sql::{DuckDbExecutor, SqlExecutor, execute_prepared, prepare_with_external};
use crate::ir::rel::{RelBackend, RelBackendOptions};
use crate::language::sparql::SparqlPlanner;

/// Runs SPARQL read queries against user-owned RDF quad tables in DuckDB.
/// The dataset name selects sources registered in `RdfDatasetMapping`.
pub struct RdfGraphEngine {
    executor: DuckDbExecutor,
    mapping: Arc<RdfDatasetMapping>,
    dataset: String,
}

impl RdfGraphEngine {
    pub fn new(
        executor: DuckDbExecutor,
        mapping: Arc<RdfDatasetMapping>,
        dataset: impl Into<String>,
    ) -> Self {
        Self {
            executor,
            mapping,
            dataset: dataset.into(),
        }
    }

    pub fn dataset(&self) -> &str {
        &self.dataset
    }

    pub fn mapping(&self) -> &RdfDatasetMapping {
        &self.mapping
    }

    /// Return the executor after queries, for example to inspect the source
    /// tables or continue using the same DuckDB session elsewhere.
    pub fn into_executor(self) -> DuckDbExecutor {
        self.executor
    }

    /// Parse, lower, and run a SPARQL read query using this engine's dataset.
    /// Typed RDF variables can participate in patterns and joins, but direct
    /// typed-term results are rejected because `ReturnedBatches` does not yet
    /// carry datatype or language metadata.
    pub async fn sparql(&mut self, query: &str) -> Result<ReturnedBatches, String> {
        let plan = SparqlPlanner::new(&self.dataset)
            .plan_str(query)
            .map_err(|error| error.to_string())?;
        let backend = RelBackend::with_options(RelBackendOptions {
            rdf_datasets: Some(Arc::clone(&self.mapping)),
            ..RelBackendOptions::default()
        });
        let lowered = backend
            .lower(&plan, &PropertyGraph::new())
            .map_err(|error| error.to_string())?;
        let prepared = prepare_with_external(
            &lowered,
            self.executor.dialect(),
            &self.mapping.physical_table_names(),
        )
        .await
        .map_err(|error| error.to_string())?;
        execute_prepared(&mut self.executor, &prepared).map_err(|error| error.to_string())
    }
}
