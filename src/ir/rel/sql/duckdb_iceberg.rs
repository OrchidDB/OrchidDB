//! Typed Iceberg sources; the host owns extension installation and credentials.
use super::*;
use arrow::datatypes::SchemaRef;
use datafusion::{
    catalog::{Session, TableProvider},
    datasource::source_as_provider,
    logical_expr::{TableProviderFilterPushDown, TableType},
    physical_plan::ExecutionPlan,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    path: String,
    #[serde(default)]
    timestamp_as_of: Option<String>,
    #[serde(default)]
    snapshot: Option<u64>,
}
#[derive(Debug)]
struct Provider {
    schema: SchemaRef,
    relation: String,
}
#[async_trait::async_trait]
impl TableProvider for Provider {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    fn table_type(&self) -> TableType {
        TableType::Base
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> datafusion::common::Result<Vec<TableProviderFilterPushDown>> {
        Ok(vec![
            TableProviderFilterPushDown::Unsupported;
            filters.len()
        ])
    }
    async fn scan(
        &self,
        _: &dyn Session,
        _: Option<&Vec<usize>>,
        _: &[Expr],
        _: Option<usize>,
    ) -> datafusion::common::Result<Arc<dyn ExecutionPlan>> {
        Err(datafusion::common::DataFusionError::Execution(
            "Iceberg source requires DuckDB SQL execution".into(),
        ))
    }
}
fn invalid(message: impl std::fmt::Display) -> SqlError {
    SqlError::Unsupported(format!("DuckDB Iceberg source: {message}"))
}
fn string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(crate) fn provider(
    metadata: &super::super::source_metadata::SourceMetadata,
    schema: SchemaRef,
) -> SqlResult<Option<Arc<dyn TableProvider>>> {
    if metadata.format.as_deref() != Some("duckdb_iceberg") {
        return Ok(None);
    }
    let options: Options =
        serde_json::from_value(serde_json::to_value(&metadata.options).map_err(invalid)?)
            .map_err(invalid)?;
    if options.path.is_empty() {
        return Err(invalid("path must be nonempty"));
    }
    if options.timestamp_as_of.is_some() && options.snapshot.is_some() {
        return Err(invalid(
            "snapshot and timestamp_as_of are mutually exclusive",
        ));
    }
    let mut arguments = vec![string(&options.path)];
    if let Some(timestamp) = options.timestamp_as_of {
        let time = chrono::DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|_| invalid("timestamp_as_of must be RFC3339 with an explicit offset"))?
            .with_timezone(&chrono::Utc);
        arguments.push(format!(
            "snapshot_from_timestamp = TIMESTAMP {}",
            string(&time.format("%Y-%m-%d %H:%M:%S%.6f").to_string())
        ));
    }
    if let Some(snapshot) = options.snapshot {
        arguments.push(format!("snapshot_from_id = {snapshot}"));
    }
    Ok(Some(Arc::new(Provider {
        schema,
        relation: format!("iceberg_scan({})", arguments.join(", ")),
    })))
}
fn source(provider: &Arc<dyn TableProvider>) -> Option<&Provider> {
    if let Some(p) = provider.as_any().downcast_ref::<Provider>() {
        return Some(p);
    }
    if let Some(p) = provider
        .as_any()
        .downcast_ref::<super::super::constraints::ConstrainedProvider>()
    {
        return source(&p.inner);
    }
    if let Some(p) = super::super::statistics::statistics_provider(provider) {
        return source(&p.inner);
    }
    None
}
pub(super) fn lower(
    plan: &LogicalPlan,
) -> SqlResult<Option<datafusion::sql::sqlparser::ast::Statement>> {
    let LogicalPlan::TableScan(scan) = plan else {
        return Ok(None);
    };
    let provider = source_as_provider(&scan.source)?;
    let Some(provider) = source(&provider) else {
        return Ok(None);
    };
    if !scan.filters.is_empty() {
        return Err(invalid("unexpected pushed filters on Iceberg source"));
    }
    let columns = scan
        .projected_schema
        .fields()
        .iter()
        .map(|f| SqlDialect::DuckDb.quote_ident(f.name()))
        .collect::<Vec<_>>();
    let mut sql = format!(
        "SELECT {} FROM {}",
        if columns.is_empty() {
            "1".into()
        } else {
            columns.join(", ")
        },
        provider.relation
    );
    if let Some(limit) = scan.fetch {
        sql.push_str(&format!(" LIMIT {limit}"));
    }
    let mut statements = datafusion::sql::sqlparser::parser::Parser::parse_sql(
        &datafusion::sql::sqlparser::dialect::DuckDbDialect {},
        &sql,
    )
    .map_err(invalid)?;
    Ok(Some(statements.remove(0)))
}
