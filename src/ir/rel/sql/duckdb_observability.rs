//! Optional DuckDB community-extension sources. Planning does no I/O and never
//! installs extensions. The caller owns the DuckDB session and its secrets.
use super::{SqlDialect, SqlError, SqlResult};
use arrow::datatypes::{DataType, SchemaRef, TimeUnit};
use datafusion::{
    catalog::TableProvider,
    common::{
        Result,
        tree_node::{Transformed, TreeNode},
    },
    datasource::source_as_provider,
    logical_expr::{Expr, LogicalPlan, TableProviderFilterPushDown, TableType},
    sql::{
        sqlparser::{ast, parser::Parser},
        unparser::Unparser,
    },
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    endpoint: Option<String>,
    secret: Option<String>,
    /// Explicit native expression; otherwise derive a selector from predicates.
    query: Option<String>,
    metric: Option<String>,
    start: Option<String>,
    end: Option<String>,
}
#[derive(Debug)]
struct Provider {
    engine: &'static str,
    options: Options,
    schema: SchemaRef,
    start: Option<i64>,
    end: Option<i64>,
}
fn invalid(message: impl std::fmt::Display) -> SqlError {
    SqlError::Unsupported(format!("DuckDB observability source: {message}"))
}
fn string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn timestamp(value: &str) -> SqlResult<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .and_then(|t| t.timestamp_nanos_opt())
        .ok_or_else(|| invalid("bounds must be RFC3339 timestamps in nanosecond range"))
}
fn stamp(ns: i64) -> String {
    // Both extension APIs accept typed timestamps. UTC strings avoid dependence
    // on the session's timezone; Loki retains nanoseconds with TIMESTAMP_NS.
    chrono::DateTime::from_timestamp(
        ns.div_euclid(1_000_000_000),
        ns.rem_euclid(1_000_000_000) as u32,
    )
    .expect("validated nanosecond timestamp")
    .format("%Y-%m-%d %H:%M:%S%.9f")
    .to_string()
}
pub(crate) fn provider(
    metadata: &crate::ir::rel::source_metadata::SourceMetadata,
    schema: SchemaRef,
) -> SqlResult<Option<Arc<dyn TableProvider>>> {
    let engine = match metadata.format.as_deref() {
        Some("duckdb_prometheus") => "prometheus",
        Some("duckdb_loki") => "loki",
        _ => return Ok(None),
    };
    let options: Options =
        serde_json::from_value(serde_json::to_value(&metadata.options).map_err(invalid)?)
            .map_err(invalid)?;
    if options.endpoint.as_deref().is_some_and(str::is_empty)
        || options.secret.as_deref().is_some_and(str::is_empty)
    {
        return Err(invalid("endpoint and secret cannot be empty"));
    }
    if options.endpoint.is_none() && (engine == "prometheus" || options.secret.is_none()) {
        return Err(invalid(
            "endpoint is required (Loki may use a named secret instead)",
        ));
    }
    if engine == "prometheus" && options.secret.is_some() {
        return Err(invalid(
            "the Prometheus extension does not support secrets/authentication",
        ));
    }
    if engine == "loki" && options.metric.is_some()
        || options.query.is_some() && options.metric.is_some()
    {
        return Err(invalid(
            "metric is only supported for generated PromQL selectors",
        ));
    }
    if options
        .query
        .as_deref()
        .is_some_and(|q| q.trim().is_empty())
    {
        return Err(invalid("query cannot be empty"));
    }
    for field in schema.fields() {
        let valid = match field.name().as_str() {
            "timestamp" => *field.data_type() == DataType::Timestamp(TimeUnit::Microsecond, None),
            "value" if engine == "prometheus" => *field.data_type() == DataType::Float64,
            _ => *field.data_type() == DataType::Utf8,
        };
        if !valid {
            return Err(invalid(format!(
                "unsupported {} column {}: {}",
                engine,
                field.name(),
                field.data_type()
            )));
        }
    }
    let start = options.start.as_deref().map(timestamp).transpose()?;
    let end = options.end.as_deref().map(timestamp).transpose()?;
    if let (Some(a), Some(b)) = (start, end) {
        if a < 0 || b <= a {
            return Err(invalid("bounds require 0 <= start < end"));
        }
    }
    Ok(Some(Arc::new(Provider {
        engine,
        options,
        schema,
        start,
        end,
    })))
}
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
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        // Every predicate is retained above the scan. Pushdown only supplies a
        // sound native prefilter and bounds, never SQL's complete semantics.
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }
}
fn bound(provider: &Arc<dyn TableProvider>) -> Option<&Provider> {
    if let Some(p) = provider.as_any().downcast_ref::<Provider>() {
        return Some(p);
    }
    if let Some(p) = provider
        .as_any()
        .downcast_ref::<crate::ir::rel::constraints::ConstrainedProvider>()
    {
        return bound(&p.inner);
    }
    if let Some(p) = crate::ir::rel::statistics::statistics_provider(provider) {
        return bound(&p.inner);
    }
    None
}
pub(super) fn push_filters(plan: LogicalPlan) -> SqlResult<(LogicalPlan, bool)> {
    use datafusion::common::tree_node::TreeNodeRecursion;
    let mut found = false;
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            if let Ok(provider) = source_as_provider(&scan.source) {
                found |= bound(&provider).is_some();
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    if !found {
        return Ok((plan, false));
    }
    use datafusion::optimizer::{Optimizer, OptimizerContext, push_down_filter::PushDownFilter};
    Ok((
        Optimizer::with_rules(vec![Arc::new(PushDownFilter::new())]).optimize(
            plan,
            &OptimizerContext::new().with_skip_failing_rules(false),
            |_, _| {},
        )?,
        true,
    ))
}
pub(super) fn lower(plan: &LogicalPlan) -> SqlResult<Option<ast::Statement>> {
    let LogicalPlan::TableScan(scan) = plan else {
        return Ok(None);
    };
    let Ok(source) = source_as_provider(&scan.source) else {
        return Ok(None);
    };
    let Some(p) = bound(&source) else {
        return Ok(None);
    };
    let (query, start, end) = if let Some(query) = &p.options.query {
        // Raw expressions may contain rate/aggregations/pipelines. Filtering their
        // inputs changes meaning, so all SQL filters stay outside the expression.
        let (Some(start), Some(end)) = (p.start, p.end) else {
            return Err(invalid("explicit query requires start and end"));
        };
        (query.clone(), start, end)
    } else {
        crate::observability::range_query(
            p.engine,
            p.options
                .metric
                .as_deref()
                .unwrap_or(scan.table_name.table()),
            &scan.filters,
            p.start,
            p.end,
        )
        .map_err(invalid)?
    };
    let logs = p.engine == "loki";
    // Prometheus evaluation is epoch-aligned, matching the native adapter. A
    // narrowed bound must not shift samples to a different evaluation grid.
    let step = 60_000_000_000i64;
    let eval_start = if logs {
        start
    } else {
        start
            .checked_add(step - 1)
            .ok_or_else(|| invalid("timestamp overflow"))?
            / step
            * step
    };
    let columns = p.schema.fields().iter().map(|f| {
        let name = f.name();
        let expression = match name.as_str() {
            "timestamp" if logs => "CAST(timestamp AS TIMESTAMP)".into(),
            "timestamp" => "timezone('UTC', timestamp)".into(),
            "value" if !logs => "value".into(),
            "line" if logs => "line".into(),
            "labels" => "CAST(to_json(labels) AS VARCHAR)".into(),
            "structured_metadata" if logs => "CAST(to_json(structured_metadata) AS VARCHAR)".into(),
            "sample_id" => format!("CAST(\"json_array\"(CAST(epoch_ns(timestamp) AS VARCHAR), to_json(map_from_entries(list_sort(map_entries(labels)))), {}) AS VARCHAR)", if logs { "line" } else { "NULL" }),
            _ => format!("labels[{}]", string(name)),
        };
        format!("{expression} AS {}", ident(name))
    }).collect::<Vec<_>>();
    let mut arguments = vec![string(&query)];
    if logs {
        arguments.push(format!(
            "start := CAST({} AS TIMESTAMP_NS)",
            string(&stamp(start))
        ));
        arguments.push(format!(
            "\"end\" := CAST({} AS TIMESTAMP_NS)",
            string(&stamp(end))
        ));
        // Override the extension's default total cap (5000), without pushing SQL
        // LIMIT across residual filters/joins/aggregates. Paging stays extension-owned.
        arguments.push("\"limit\" := 9223372036854775807".into());
        arguments.push("direction := 'forward'".into());
    } else {
        arguments.push(format!(
            "TIMESTAMPTZ {}",
            string(&(stamp(eval_start) + "+00"))
        ));
        // API end is inclusive. Microsecond precision is sufficient on the 60s grid.
        arguments.push(format!(
            "TIMESTAMPTZ {}",
            string(&(stamp((end - 1) / 1000 * 1000) + "+00"))
        ));
        arguments.push("step := INTERVAL '60 seconds'".into());
    }
    if let Some(endpoint) = &p.options.endpoint {
        arguments.push(format!("endpoint := {}", string(endpoint)));
    }
    if let Some(secret) = &p.options.secret {
        arguments.push(format!("secret := {}", string(secret)));
    }
    let mut sql = if eval_start >= end {
        // Avoid issuing an invalid range request for an empty evaluation grid.
        format!(
            "SELECT {} WHERE false",
            p.schema
                .fields()
                .iter()
                .map(|f| Ok(format!(
                    "CAST(NULL AS {}) AS {}",
                    SqlDialect::DuckDb.sql_type(f.data_type())?,
                    ident(f.name())
                )))
                .collect::<SqlResult<Vec<_>>>()?
                .join(", ")
        )
    } else {
        format!(
            "SELECT {} FROM {}_scan({})",
            columns.join(", "),
            p.engine,
            arguments.join(", ")
        )
    };
    let dialect = SqlDialect::DuckDb;
    let unparser_dialect = dialect.unparser_dialect();
    let unparser = Unparser::new(unparser_dialect.as_ref());
    let filters = scan
        .filters
        .iter()
        .map(|filter| {
            let filter = filter
                .clone()
                .transform_up(|e| match e {
                    Expr::Column(mut c) => {
                        c.relation = None;
                        Ok(Transformed::yes(Expr::Column(c)))
                    }
                    _ => Ok(Transformed::no(e)),
                })?
                .data;
            let mut expr = unparser.expr_to_sql(&filter)?;
            super::functions::prepare_scoped_ast(&mut expr, dialect, false)?;
            Ok(format!("({expr})"))
        })
        .collect::<SqlResult<Vec<_>>>()?;
    // Filter the full schema before projection: a pushed predicate can reference
    // columns absent from the requested output.
    let selected = scan
        .projected_schema
        .fields()
        .iter()
        .map(|f| ident(f.name()))
        .collect::<Vec<_>>();
    sql = format!(
        "SELECT {} FROM ({sql}) AS __telemetry{}",
        if selected.is_empty() {
            "1".into()
        } else {
            selected.join(", ")
        },
        if filters.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", filters.join(" AND "))
        }
    );
    if let Some(limit) = scan.fetch {
        sql.push_str(&format!(" LIMIT {limit}"));
    }
    let mut statements =
        Parser::parse_sql(dialect.parser_dialect().as_ref(), &sql).map_err(invalid)?;
    Ok(Some(statements.remove(0)))
}
