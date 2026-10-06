//! Typed RDF results and the compatibility facade for GraphEngine.
//!
//! RDF vocabulary resolves to application tables in the common mapping catalog.
//! Language execution contexts borrow the engine's DAG session and never own
//! a separate connection or transaction manager.

use std::sync::Arc;

use arrow::array::{Array, BooleanArray, StringArray};

use crate::ir::catalog::PropertyGraph;
use crate::ir::policy::ResultForm;
use crate::ir::rel::rdf::{RdfDatasetMapping, binding_identity_columns};
use crate::ir::rel::sql::{DuckDbExecutor, PreparedSql, SqlExecutor, SqlDialect};
use crate::ir::rel::{RelBackend, RelBackendOptions};
use crate::ir::runtime::ReturnedBatches;
use crate::language::sparql::SparqlPlanner;

mod relational_update;
pub(crate) mod scalar;
mod update;

/// Compatibility facade. Execution and transactions belong to GraphEngine.
pub struct RdfGraphEngine {
    engine: Result<crate::engine::GraphEngine, String>,
    mapping: Arc<RdfDatasetMapping>,
    dataset: String,
    last_query_stats: Option<crate::ir::exec::ExecStats>,
}
impl RdfGraphEngine {
    pub fn new(
        mut executor: DuckDbExecutor,
        mapping: Arc<RdfDatasetMapping>,
        dataset: impl Into<String>,
    ) -> Self {
        let timeout = executor.query_timeout();
        let language_functions = executor.language_functions_enabled();
        let engine = executor
            .connection()
            .map(|_| ())
            .map_err(|e| e.to_string())
            .and_then(|_| {
                let connection = executor
                    .take_connection()
                    .ok_or("missing database connection")?;
                crate::engine::GraphEngine::mapped(
                    connection,
                    Arc::new(
                        crate::ir::rel::mapping::GraphMapping::new()
                            .with_rdf_mapping((*mapping).clone()),
                    ),
                )
            })
            .and_then(|mut engine| {
                if let Some(timeout) = timeout {
                    engine.set_sql_timeout(timeout);
                }
                engine.set_language_functions(language_functions)?;
                Ok(engine)
            });
        Self {
            engine,
            mapping,
            dataset: dataset.into(),
            last_query_stats: None,
        }
    }
    pub fn dataset(&self) -> &str {
        &self.dataset
    }
    pub fn mapping(&self) -> &RdfDatasetMapping {
        &self.mapping
    }
    pub fn last_query_stats(&self) -> Option<&crate::ir::exec::ExecStats> {
        self.last_query_stats.as_ref()
    }
    pub fn into_executor(self) -> DuckDbExecutor {
        self.engine
            .expect("RDF engine initialization failed")
            .into_executor()
    }
    pub async fn query(&mut self, query: &str) -> Result<SparqlResults, String> {
        decode_results(&self.sparql(query).await?)
    }
    pub async fn sparql(&mut self, query: &str) -> Result<ReturnedBatches, String> {
        let result = self
            .engine
            .as_mut()
            .map_err(|e| e.clone())?
            .sparql_dataset(query, &self.dataset)
            .await?;
        self.last_query_stats = Some(result.stats);
        Ok(result.returned)
    }
    pub async fn sql(&self, query: &str) -> Result<String, String> {
        self.engine
            .as_ref()
            .map_err(|e| e.clone())?
            .sparql_sql(query, &self.dataset)
            .await
    }
    pub async fn update(&mut self, query: &str, base: Option<&str>) -> Result<(), String> {
        self.engine
            .as_mut()
            .map_err(|e| e.clone())?
            .sparql_update(query, &self.dataset, base)
            .await
    }
}

/// Borrowed language execution context, using the common engine's DAG session.
pub(crate) struct RdfSession<'a> {
    pub(crate) resources: &'a crate::ir::rel::dag::DagSession,
    pub(crate) mapping: Arc<RdfDatasetMapping>,
    pub(crate) dataset: String,
    pub(crate) scalar_registered: bool,
    pub(crate) last_query_stats: Option<crate::ir::exec::ExecStats>,
}
impl RdfSession<'_> {
    fn executor(&self) -> Result<std::sync::MutexGuard<'_, DuckDbExecutor>, String> {
        self.resources.executor()
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
        self.last_query_stats = None;
        let parsed = crate::language::sparql::parse_query(query).map_err(|e| e.to_string())?;
        self.sparql_parsed(&parsed).await
    }

    async fn sparql_parsed(
        &mut self,
        query: &crate::spargebra::Query,
    ) -> Result<ReturnedBatches, String> {
        let plan = SparqlPlanner::new(&self.dataset)
            .plan(query)
            .map_err(|e| e.to_string())?;
        let (output, stats) = self.execute_plan(&plan).await?;
        self.last_query_stats = Some(stats.into());
        Ok(output)
    }
    pub(crate) async fn execute_plan(
        &mut self,
        plan: &crate::ir::plan::GraphPlan,
    ) -> Result<(ReturnedBatches, crate::ir::rel::dag::DagStats), String> {
        if !self.scalar_registered {
            self.executor()?
                .connection()
                .map_err(|error| error.to_string())?
                .register_scalar_function::<scalar::SparqlScalar>("__orchiddb_sparql_scalar")
                .map_err(|error| error.to_string())?;
            self.scalar_registered = true;
        }
        let mapping = if self.resources.region_session.is_some() {
            let mut mapping = (*self.mapping).clone();
            let mut tables = std::collections::BTreeMap::new();
            for (name, _) in mapping.registered_tables() {
                let sql = format!("SELECT * FROM {}", SqlDialect::DuckDb.quote_ident(&name));
                let batch = {
                    let mut executor = self.executor()?;
                    let connection = executor.connection().map_err(|e| e.to_string())?;
                    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
                    let reader = statement.query_arrow([]).map_err(|e| e.to_string())?;
                    let schema = reader.get_schema();
                    arrow::compute::concat_batches(&schema, &reader.collect::<Vec<_>>()).map_err(|e| e.to_string())?
                };
                let provider: Arc<dyn datafusion::datasource::TableProvider> = Arc::new(datafusion::datasource::MemTable::try_new(batch.schema(), vec![vec![batch]]).map_err(|e| e.to_string())?);
                tables.insert(name, provider);
            }
            mapping.extend_tables(&tables);
            Arc::new(mapping)
        } else { self.mapping.clone() };
        let lowered = RelBackend::with_options(RelBackendOptions {
            rdf_datasets: Some(mapping),
            language_functions: self.resources.language_functions_enabled(),
            ..Default::default()
        })
        .lower(plan, &PropertyGraph::new())
        .map_err(|e| e.to_string())?;
        let mut execution = Box::pin(crate::ir::rel::dag::execute_with_extensions(
            lowered,
            vec![],
            None,
            Some(&self.resources),
        ));
        let (output, stats) = futures::future::poll_fn(|cx| {
            stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, || {
                std::future::Future::poll(execution.as_mut(), cx)
            })
        })
        .await
        .map_err(|error| error.to_string())?;
        drop(execution);
        Ok((output, stats))
    }

    /// The DuckDB SQL that [`RdfGraphEngine::sparql`] would execute.
    pub async fn sql(&self, query: &str) -> Result<String, String> {
        Ok(self.prepare(query).await?.query)
    }

    pub(crate) async fn prepare(&self, query: &str) -> Result<PreparedSql, String> {
        let parsed = crate::language::sparql::parse_query(query).map_err(|e| e.to_string())?;
        self.prepare_parsed(&parsed).await
    }

    async fn prepare_parsed(&self, query: &crate::spargebra::Query) -> Result<PreparedSql, String> {
        // Typed RDF expressions expand into several correlated SQL columns.
        // Preserve the same session and async execution while allowing the
        // logical planner's synchronous recursion to use a larger stack.
        let mut preparation = Box::pin(self.prepare_inner(query));
        futures::future::poll_fn(|cx| {
            stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, || {
                std::future::Future::poll(preparation.as_mut(), cx)
            })
        })
        .await
    }

    async fn prepare_inner(&self, query: &crate::spargebra::Query) -> Result<PreparedSql, String> {
        let lowered = self.lower_query(query)?;
        let dialect = self.executor()?.dialect();
        crate::ir::rel::sql::prepare_with_external(
            &lowered,
            dialect,
            &self.mapping.physical_table_names(),
        )
        .await
        .map_err(|error| error.to_string())
    }

    pub(crate) fn lower_query(
        &self,
        query: &crate::spargebra::Query,
    ) -> Result<crate::ir::rel::LoweredPlan, String> {
        stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, || {
            self.lower_query_inner(query)
        })
    }

    fn lower_query_inner(
        &self,
        query: &crate::spargebra::Query,
    ) -> Result<crate::ir::rel::LoweredPlan, String> {
        let plan = SparqlPlanner::new(&self.dataset)
            .plan(query)
            .map_err(|error| error.to_string())?;
        let backend = RelBackend::with_options(RelBackendOptions {
            rdf_datasets: Some(Arc::clone(&self.mapping)),
            language_functions: self.resources.language_functions_enabled(),
            ..RelBackendOptions::default()
        });
        backend
            .lower(&plan, &PropertyGraph::new())
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

pub fn decode_results(output: &ReturnedBatches) -> Result<SparqlResults, String> {
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

/// Preserve the scalar Arrow layout of the older ontology entry points.
pub(crate) fn legacy_columns(mut output: ReturnedBatches) -> Result<ReturnedBatches, String> {
    if output.batch.num_columns() <= output.fields.len() || output.result_form != ResultForm::RowSet
    {
        return Ok(output);
    }
    if output.fields.is_empty() {
        output.batch = output.batch.project(&[]).map_err(|e| e.to_string())?;
        return Ok(output);
    }
    let mut fields = Vec::new();
    let mut arrays = Vec::new();
    for name in &output.fields {
        let index = output
            .batch
            .schema()
            .index_of(name)
            .map_err(|e| e.to_string())?;
        let mut array = output.batch.column(index).clone();
        let dt = binding_identity_columns(name)[1].clone();
        if let Ok(dt_index) = output.batch.schema().index_of(&dt) {
            if let Some(values) = output
                .batch
                .column(dt_index)
                .as_any()
                .downcast_ref::<StringArray>()
            {
                let types = values
                    .iter()
                    .flatten()
                    .collect::<std::collections::BTreeSet<_>>();
                if types.len() == 1 {
                    if let Some(kind) =
                        crate::ir::rel::rdf_mapping::legacy_scalar_type(types.first().unwrap())
                    {
                        array = arrow::compute::cast(&array, &kind).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        fields.push(arrow::datatypes::Field::new(
            name,
            array.data_type().clone(),
            true,
        ));
        arrays.push(array);
    }
    output.batch =
        arrow::array::RecordBatch::try_new(Arc::new(arrow::datatypes::Schema::new(fields)), arrays)
            .map_err(|e| e.to_string())?;
    Ok(output)
}
