//! External SPARQL reads represented as SQL IR scan sources. DataFusion
//! obtains typed Arrow bindings; the normal SQL source stage supplies them
//! to DuckDB for joins and other eligible relational operators.
use arrow::array::{ArrayRef, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use async_trait::async_trait;
use datafusion::catalog::{Session, TableProvider};
use datafusion::error::{DataFusionError, Result};
use datafusion::execution::TaskContext;
use datafusion::logical_expr::{Expr, TableType};
use datafusion::physical_expr::EquivalenceProperties;
use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
    SendableRecordBatchStream,
};
use serde_json::Value;
use std::{any::Any, fmt, sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub(crate) struct ServiceSource {
    url: String,
    query: String,
    outputs: Vec<String>,
    silent: bool,
    schema: SchemaRef,
}

impl ServiceSource {
    pub(super) fn new(url: String, query: String, outputs: Vec<String>, silent: bool) -> Self {
        let mut columns = outputs
            .iter()
            .flat_map(|name| {
                std::iter::once(name.clone()).chain(super::rdf::binding_identity_columns(name))
            })
            .collect::<Vec<_>>();
        if columns.is_empty() {
            columns.push("__service_unit".into());
        }
        let schema = Arc::new(Schema::new(
            columns
                .into_iter()
                .map(|name| Field::new(name, DataType::Utf8, true))
                .collect::<Vec<_>>(),
        ));
        Self {
            url,
            query,
            outputs,
            silent,
            schema,
        }
    }

    async fn fetch(&self) -> std::result::Result<Vec<Vec<Option<String>>>, String> {
        let response = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?
            .post(&self.url)
            .header("Content-Type", "application/sparql-query")
            .header("Accept", "application/sparql-results+json")
            .body(self.query.clone())
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json::<Value>()
            .await
            .map_err(|e| e.to_string())?;
        let bindings = response
            .pointer("/results/bindings")
            .and_then(Value::as_array)
            .ok_or("SERVICE response lacks SELECT bindings")?;
        let scope = crate::spargebra::term::BlankNode::default().as_str().to_owned();
        let mut rows = Vec::new();
        for binding in bindings {
            let mut row = Vec::new();
            for name in &self.outputs {
                let value = &binding[name.trim_start_matches('?')];
                if value.is_null() {
                    row.extend([None, None, None, None]);
                    continue;
                }
                let lexical = value["value"].as_str().ok_or("SERVICE term has no value")?;
                let (lexical, kind, datatype, language) = match value["type"].as_str() {
                    Some("uri") => (lexical.into(), "IRI", None, None),
                    Some("bnode") => (format!("{scope}_{lexical}"), "BLANK", None, None),
                    Some("literal" | "typed-literal") => {
                        let language = value["xml:lang"].as_str().map(str::to_ascii_lowercase);
                        let dt = value["datatype"].as_str().unwrap_or(if language.is_some() {
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
                        } else {
                            "http://www.w3.org/2001/XMLSchema#string"
                        });
                        (lexical.into(), "LITERAL", Some(dt.into()), language)
                    }
                    _ => return Err("Unknown SERVICE RDF term type".into()),
                };
                row.extend([Some(lexical), Some(kind.into()), datatype, language]);
            }
            if self.outputs.is_empty() {
                row.push(Some("1".into()));
            }
            rows.push(row);
        }
        Ok(rows)
    }
}

#[async_trait]
impl TableProvider for ServiceSource {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    fn table_type(&self) -> TableType {
        TableType::Temporary
    }
    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let schema = match projection {
            Some(indices) => Arc::new(self.schema.project(indices)?),
            None => self.schema.clone(),
        };
        Ok(Arc::new(ServiceExec {
            source: self.clone(),
            projection: projection.cloned(),
            limit,
            properties: Arc::new(PlanProperties::new(
                EquivalenceProperties::new(schema),
                Partitioning::UnknownPartitioning(1),
                EmissionType::Final,
                Boundedness::Bounded,
            )),
        }))
    }
}

#[derive(Debug, Clone)]
struct ServiceExec {
    source: ServiceSource,
    projection: Option<Vec<usize>>,
    limit: Option<usize>,
    properties: Arc<PlanProperties>,
}
impl DisplayAs for ServiceExec {
    fn fmt_as(&self, _: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SparqlServiceExec")
    }
}
impl ExecutionPlan for ServiceExec {
    fn name(&self) -> &str {
        "SparqlServiceExec"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![]
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if !children.is_empty() {
            return Err(DataFusionError::Plan(
                "SERVICE is an external source".into(),
            ));
        }
        Ok(self)
    }
    fn execute(&self, partition: usize, _: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(
                "SERVICE has one partition".into(),
            ));
        }
        let execution = self.clone();
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            futures::stream::once(async move {
                let source = &execution.source;
                let rows = match source.fetch().await {
                    Ok(rows) => rows,
                    Err(_) if source.silent => vec![vec![None; source.schema.fields().len()]],
                    Err(error) => {
                        return Err(DataFusionError::Execution(format!(
                            "SERVICE {}: {error}",
                            source.url
                        )));
                    }
                };
                let arrays = (0..source.schema.fields().len())
                    .map(|index| {
                        Arc::new(StringArray::from(
                            rows.iter()
                                .map(|row| row[index].as_deref())
                                .collect::<Vec<_>>(),
                        )) as ArrayRef
                    })
                    .collect();
                let mut batch = RecordBatch::try_new(source.schema.clone(), arrays)?;
                if let Some(indices) = &execution.projection {
                    batch = batch.project(indices)?;
                }
                if let Some(limit) = execution.limit {
                    batch = batch.slice(0, limit.min(batch.num_rows()));
                }
                Ok(batch)
            }),
        )))
    }
}
