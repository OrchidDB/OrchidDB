//! Typed connector sources, bound via source_metadata {format:"trino", options:{...}}.
//! Every predicate above a connector scan remains in Trino SQL. Native query
//! strings are explicit catalog configuration, never inferred from SQL text.
use super::*;
use datafusion::{
    catalog::{Session, TableProvider},
    common::{DataFusionError, Result},
    datasource::source_as_provider,
    logical_expr::{LogicalPlan, TableType},
    physical_plan::ExecutionPlan,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "connector", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectorSource {
    Iceberg {
        catalog: String,
        schema: String,
        table: String,
        #[serde(default)]
        snapshot: Option<i64>,
        #[serde(default)]
        metadata: Option<String>,
    },
    IcebergChanges {
        catalog: String,
        schema: String,
        table: String,
        start_snapshot: i64,
        end_snapshot: i64,
    },
    Delta {
        catalog: String,
        schema: String,
        table: String,
        #[serde(default)]
        version: Option<i64>,
    },
    DeltaChanges {
        catalog: String,
        schema: String,
        table: String,
        since_version: i64,
    },
    Loki {
        catalog: String,
        query: String,
        start: String,
        end: String,
    },
    Prometheus {
        catalog: String,
        metric: String,
        start: String,
        end: String,
    },
    Elasticsearch {
        catalog: String,
        schema: String,
        index: String,
        query: serde_json::Value,
    },
    OpenSearch {
        catalog: String,
        schema: String,
        index: String,
        query: serde_json::Value,
    },
    MongoDb {
        catalog: String,
        database: String,
        collection: String,
        filter: serde_json::Value,
    },
}
impl ConnectorSource {
    /// Connector relation SQL; caller supplies and validates the output schema.
    pub fn relation_sql(&self) -> SqlResult<String> {
        self.relation(false)
    }
    fn relation(&self, ast_form: bool) -> SqlResult<String> {
        let version_keyword = if ast_form { "VERSION" } else { "FOR VERSION" };
        let q = |s: &str| TRINO.quote_ident(s);
        let table = |catalog: &str, schema: &str, name: &str| {
            format!("{}.{}.{}", q(catalog), q(schema), q(name))
        };
        let function = |catalog: &str, name: &str, arguments: String| {
            format!("TABLE({}.system.{name}({arguments}))", q(catalog))
        };
        Ok(match self {
            Self::Iceberg {
                catalog,
                schema,
                table: name,
                snapshot,
                metadata,
            } => {
                if snapshot.is_some() && metadata.is_some() {
                    return Err(invalid("snapshot and metadata are mutually exclusive"));
                }
                let mut name = name.clone();
                if let Some(suffix) = metadata {
                    if !matches!(
                        suffix.as_str(),
                        "properties"
                            | "history"
                            | "metadata_log_entries"
                            | "snapshots"
                            | "manifests"
                            | "all_manifests"
                            | "partitions"
                            | "files"
                            | "entries"
                            | "all_entries"
                            | "refs"
                    ) {
                        return Err(invalid("unsupported Iceberg metadata table"));
                    }
                    name.push('$');
                    name.push_str(suffix);
                }
                let mut sql = table(catalog, schema, &name);
                if let Some(id) = snapshot {
                    if *id < 0 {
                        return Err(invalid("negative snapshot"));
                    }
                    sql.push_str(&format!(" {version_keyword} AS OF {id}"));
                }
                sql
            }
            Self::IcebergChanges {
                catalog,
                schema,
                table,
                start_snapshot,
                end_snapshot,
            } => {
                if *start_snapshot < 0 || *end_snapshot < 0 {
                    return Err(invalid("negative snapshot"));
                }
                function(
                    catalog,
                    "table_changes",
                    format!(
                        "schema_name => {}, table_name => {}, start_snapshot_id => {start_snapshot}, end_snapshot_id => {end_snapshot}",
                        string(schema),
                        string(table)
                    ),
                )
            }
            Self::Delta {
                catalog,
                schema,
                table: name,
                version,
            } => {
                let mut sql = table(catalog, schema, name);
                if let Some(v) = version {
                    if *v < 0 {
                        return Err(invalid("negative Delta version"));
                    }
                    sql.push_str(&format!(" {version_keyword} AS OF {v}"));
                }
                sql
            }
            Self::DeltaChanges {
                catalog,
                schema,
                table,
                since_version,
            } => {
                if *since_version < 0 {
                    return Err(invalid("negative Delta version"));
                }
                function(
                    catalog,
                    "table_changes",
                    format!(
                        "schema_name => {}, table_name => {}, since_version => {since_version}",
                        string(schema),
                        string(table)
                    ),
                )
            }
            Self::Loki {
                catalog,
                query,
                start,
                end,
            } => {
                let (start, end) = bounds(start, end)?;
                function(
                    catalog,
                    "query_range",
                    format!(
                        "{}, TIMESTAMP {}, TIMESTAMP {}",
                        string(query),
                        string(&start),
                        string(&end)
                    ),
                )
            }
            Self::Prometheus {
                catalog,
                metric,
                start,
                end,
            } => {
                let (start, end) = bounds(start, end)?;
                // Trino's connector reads raw samples; it does not implement a
                // PromQL passthrough function or the direct adapter's 60s grid.
                format!(
                    "(SELECT * FROM {} WHERE timestamp >= TIMESTAMP {} AND timestamp < TIMESTAMP {})",
                    table(catalog, "default", metric),
                    string(&(start + " UTC")),
                    string(&(end + " UTC"))
                )
            }
            Self::Elasticsearch {
                catalog,
                schema,
                index,
                query,
            }
            | Self::OpenSearch {
                catalog,
                schema,
                index,
                query,
            } => {
                if !query.is_object() {
                    return Err(invalid("search query must be a JSON object"));
                }
                function(
                    catalog,
                    "raw_query",
                    format!(
                        "schema => {}, index => {}, query => {}",
                        string(schema),
                        string(index),
                        string(&query.to_string())
                    ),
                )
            }
            Self::MongoDb {
                catalog,
                database,
                collection,
                filter,
            } => {
                if !filter.is_object() {
                    return Err(invalid("MongoDB filter must be a JSON object"));
                }
                function(
                    catalog,
                    "query",
                    format!(
                        "database => {}, collection => {}, filter => {}",
                        string(database),
                        string(collection),
                        string(&filter.to_string())
                    ),
                )
            }
        })
    }
    pub fn provider(
        self,
        schema: arrow::datatypes::SchemaRef,
    ) -> SqlResult<Arc<dyn TableProvider>> {
        self.relation_sql()?;
        Ok(Arc::new(ConnectorProvider {
            source: self,
            schema,
        }))
    }
    fn column(&self, name: &str, ty: &DataType) -> SqlResult<String> {
        let q = TRINO.quote_ident(name);
        Ok(match self {
            Self::Loki { .. } | Self::Prometheus { .. } => match name {
                "timestamp" => format!(
                    "CAST(timestamp AT TIME ZONE 'UTC' AS {})",
                    TRINO.sql_type(ty)?
                ),
                "value" => "value".into(),
                "line" if matches!(self, Self::Loki { .. }) => "value".into(),
                "labels" => "json_format(CAST(labels AS JSON))".into(),
                _ => format!("element_at(labels, {})", string(name)),
            },
            _ => q,
        })
    }
}
fn bounds(start: &str, end: &str) -> SqlResult<(String, String)> {
    let parse = |s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|t| t.with_timezone(&chrono::Utc))
            .map_err(|_| invalid("time bounds must be RFC3339 timestamps"))
    };
    let start = parse(start)?;
    let end = parse(end)?;
    if end <= start {
        return Err(invalid("time window must have start < end"));
    }
    Ok((
        start.format("%Y-%m-%d %H:%M:%S%.9f").to_string(),
        end.format("%Y-%m-%d %H:%M:%S%.9f").to_string(),
    ))
}
fn invalid(s: &str) -> SqlError {
    SqlError::Unsupported(format!("trino connector: {s}"))
}
#[derive(Debug)]
struct ConnectorProvider {
    source: ConnectorSource,
    schema: arrow::datatypes::SchemaRef,
}
#[async_trait::async_trait]
impl TableProvider for ConnectorProvider {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn schema(&self) -> arrow::datatypes::SchemaRef {
        self.schema.clone()
    }
    fn table_type(&self) -> TableType {
        TableType::Base
    }
    async fn scan(
        &self,
        _: &dyn Session,
        _: Option<&Vec<usize>>,
        _: &[datafusion::logical_expr::Expr],
        _: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Err(DataFusionError::Execution(
            "Trino connector source requires Trino execution".into(),
        ))
    }
}
pub(super) fn provider(
    metadata: &crate::ir::rel::source_metadata::SourceMetadata,
    schema: arrow::datatypes::SchemaRef,
) -> SqlResult<Option<Arc<dyn TableProvider>>> {
    if metadata.format.as_deref() != Some("trino") {
        return Ok(None);
    }
    let source: ConnectorSource = serde_json::from_value(
        serde_json::to_value(&metadata.options).map_err(|e| invalid(&e.to_string()))?,
    )
    .map_err(|e| invalid(&e.to_string()))?;
    source.provider(schema).map(Some)
}
fn connector(provider: &Arc<dyn TableProvider>) -> Option<&ConnectorProvider> {
    if let Some(p) = provider.as_any().downcast_ref::<ConnectorProvider>() {
        return Some(p);
    }
    if let Some(p) = provider
        .as_any()
        .downcast_ref::<crate::ir::rel::constraints::ConstrainedProvider>()
    {
        return connector(&p.inner);
    }
    if let Some(p) = crate::ir::rel::statistics::statistics_provider(provider) {
        return connector(&p.inner);
    }
    None
}
pub(super) fn lower(plan: &LogicalPlan) -> SqlResult<Option<ast::Statement>> {
    let LogicalPlan::TableScan(scan) = plan else {
        return Ok(None);
    };
    let provider = source_as_provider(&scan.source)?;
    let Some(provider) = connector(&provider) else {
        return Ok(None);
    };
    let columns = scan
        .projected_schema
        .fields()
        .iter()
        .map(|f| {
            Ok(format!(
                "{} AS {}",
                provider.source.column(f.name(), f.data_type())?,
                TRINO.quote_ident(f.name())
            ))
        })
        .collect::<SqlResult<Vec<_>>>()?;
    // Emit pushed scan predicates too; provider does not claim exact pushdown.
    let dialect = SqlDialect::Custom(&TRINO);
    let unparser_dialect = dialect.unparser_dialect();
    let unparser = datafusion::sql::unparser::Unparser::new(unparser_dialect.as_ref());
    let mut filters = Vec::new();
    for filter in &scan.filters {
        use datafusion::common::tree_node::{Transformed, TreeNode};
        let filter = filter
            .clone()
            .transform_up(|e| match e {
                datafusion::logical_expr::Expr::Column(mut c) => {
                    c.relation = None;
                    Ok(Transformed::yes(datafusion::logical_expr::Expr::Column(c)))
                }
                _ => Ok(Transformed::no(e)),
            })?
            .data;
        let mut expr = unparser.expr_to_sql(&filter)?;
        super::super::functions::prepare_scoped_ast(&mut expr, dialect, false)?;
        filters.push(expr.to_string());
    }
    let mut sql = format!(
        "SELECT {} FROM {}",
        if columns.is_empty() {
            "1".into()
        } else {
            columns.join(", ")
        },
        provider.source.relation(true)?
    );
    if !filters.is_empty() {
        sql = format!(
            "SELECT * FROM ({sql}) AS __connector WHERE {}",
            filters.join(" AND ")
        );
    }
    if let Some(limit) = scan.fetch {
        sql.push_str(&format!(" LIMIT {limit}"));
    }
    let mut statements =
        datafusion::sql::sqlparser::parser::Parser::parse_sql(&super::TrinoParser, &sql)
            .map_err(|e| invalid(&e.to_string()))?;
    Ok(Some(statements.remove(0)))
}
