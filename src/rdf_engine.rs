//! SPARQL queries and mapped updates over existing DuckDB RDF tables.
//!
//! `RdfGraphEngine` accepts an `RdfDatasetMapping` whose registered table
//! providers describe the source schemas. The table names also exist in the
//! supplied DuckDB executor. Queries use the general SPARQL planner, then
//! lower quad scans through the mapping and execute SQL against those tables.

use std::sync::Arc;

use arrow::array::{Array, BooleanArray, StringArray};

use crate::ir::catalog::PropertyGraph;
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::policy::ResultForm;
use crate::ir::rel::rdf::{RdfDatasetMapping, binding_identity_columns};
use crate::ir::rel::sql::{
    DuckDbExecutor, PreparedSql, SqlExecutor, execute_prepared, prepare_with_external,
};
use crate::ir::rel::{RelBackend, RelBackendOptions};
use crate::language::sparql::SparqlPlanner;

mod scalar;
mod update;

/// Runs SPARQL queries and explicitly mapped updates against user-owned tables.
/// The dataset name selects sources registered in `RdfDatasetMapping`.
pub struct RdfGraphEngine {
    executor: DuckDbExecutor,
    mapping: Arc<RdfDatasetMapping>,
    dataset: String,
    scalar_registered: bool,
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
            scalar_registered: false,
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

    /// Run a query and decode its result as typed RDF terms: solution rows
    /// for SELECT, a boolean for ASK, and triples for CONSTRUCT.
    pub async fn query(&mut self, query: &str) -> Result<SparqlResults, String> {
        let output = self.sparql(query).await?;
        decode_results(&output)
    }

    /// Parse, lower, and run a SPARQL read query using this engine's dataset.
    /// The batch holds the visible field values first, followed by the kind,
    /// datatype, and language columns of each field (see
    /// `binding_identity_columns`); [`RdfGraphEngine::query`] decodes them.
    pub async fn sparql(&mut self, query: &str) -> Result<ReturnedBatches, String> {
        if !self.scalar_registered {
            self.executor.connection().map_err(|error| error.to_string())?
                .register_scalar_function::<scalar::SparqlScalar>("__crabgraph_sparql_scalar")
                .map_err(|error| error.to_string())?;
            self.scalar_registered = true;
        }
        let prepared = self.prepare(query).await?;
        stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024,
            || execute_prepared(&mut self.executor, &prepared)).map_err(|error| error.to_string())
    }

    /// The DuckDB SQL that [`RdfGraphEngine::sparql`] would execute.
    pub async fn sql(&self, query: &str) -> Result<String, String> {
        Ok(self.prepare(query).await?.query)
    }

    async fn prepare(&self, query: &str) -> Result<PreparedSql, String> {
        // Typed RDF expressions expand into several correlated SQL columns.
        // Preserve the same session and async execution while allowing the
        // logical planner's synchronous recursion to use a larger stack.
        let mut preparation = Box::pin(self.prepare_inner(query));
        futures::future::poll_fn(|cx| stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024,
            || std::future::Future::poll(preparation.as_mut(), cx))).await
    }

    async fn prepare_inner(&self, query: &str) -> Result<PreparedSql, String> {
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
        prepare_with_external(
            &lowered,
            self.executor.dialect(),
            &self.mapping.physical_table_names(),
        )
        .await
        .map_err(|error| error.to_string())
    }
}

/// One RDF term in a query result.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RdfTermValue {
    Iri(String),
    BlankNode(String),
    Literal {
        lexical: String,
        datatype: String,
        language: Option<String>,
    },
}

impl RdfTermValue {
    pub fn iri(value: impl Into<String>) -> Self {
        Self::Iri(value.into())
    }

    /// A simple literal (`xsd:string`).
    pub fn string(value: impl Into<String>) -> Self {
        Self::typed(value, "http://www.w3.org/2001/XMLSchema#string")
    }

    pub fn typed(value: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self::Literal {
            lexical: value.into(),
            datatype: datatype.into(),
            language: None,
        }
    }

    pub fn lang(value: impl Into<String>, language: impl Into<String>) -> Self {
        Self::Literal {
            lexical: value.into(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into(),
            language: Some(language.into()),
        }
    }
}

/// Decoded SPARQL query results.
#[derive(Debug, Clone, PartialEq)]
pub enum SparqlResults {
    /// Variables (with their `?` prefix) and rows; `None` is unbound.
    Solutions {
        variables: Vec<String>,
        rows: Vec<Vec<Option<RdfTermValue>>>,
    },
    Boolean(bool),
    Graph(Vec<[RdfTermValue; 3]>),
}

fn decode_results(output: &ReturnedBatches) -> Result<SparqlResults, String> {
    let batch = &output.batch;
    if output.result_form == ResultForm::Boolean {
        let column = batch
            .column(0)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .ok_or("ASK result is not boolean")?;
        return Ok(SparqlResults::Boolean(
            batch.num_rows() == 1 && !column.is_null(0) && column.value(0),
        ));
    }
    let text = |name: &str| -> Result<&StringArray, String> {
        let index = batch
            .schema()
            .index_of(name)
            .map_err(|_| format!("result column `{name}` is missing"))?;
        batch
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| format!("result column `{name}` is not text"))
    };
    let mut columns = Vec::new();
    for field in &output.fields {
        let [kind, datatype, language] = binding_identity_columns(field);
        columns.push([
            text(field)?,
            text(&kind)?,
            text(&datatype)?,
            text(&language)?,
        ]);
    }
    let cell = |columns: &[&StringArray; 4], row: usize| -> Result<Option<RdfTermValue>, String> {
        let [value, kind, datatype, language] = columns;
        if kind.is_null(row) {
            return Ok(None);
        }
        let lexical = value.value(row).to_string();
        Ok(Some(match kind.value(row) {
            "IRI" => RdfTermValue::Iri(lexical),
            "BLANK" => RdfTermValue::BlankNode(lexical),
            "LITERAL" => RdfTermValue::Literal {
                lexical,
                datatype: if datatype.is_null(row) {
                    "http://www.w3.org/2001/XMLSchema#string".into()
                } else {
                    datatype.value(row).to_string()
                },
                language: (!language.is_null(row)).then(|| language.value(row).to_string()),
            },
            other => return Err(format!("unknown RDF term kind `{other}`")),
        }))
    };
    if output.result_form == ResultForm::RdfGraph {
        let mut triples = Vec::new();
        for row in 0..batch.num_rows() {
            let mut terms = Vec::new();
            for column in &columns {
                terms.push(cell(column, row)?.ok_or("CONSTRUCT produced an unbound term")?);
            }
            let [s, p, o]: [RdfTermValue; 3] = terms.try_into().expect("three columns");
            triples.push([s, p, o]);
        }
        return Ok(SparqlResults::Graph(triples));
    }
    let mut rows = Vec::new();
    for row in 0..batch.num_rows() {
        rows.push(
            columns
                .iter()
                .map(|column| cell(column, row))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(SparqlResults::Solutions {
        variables: output.fields.clone(),
        rows,
    })
}
