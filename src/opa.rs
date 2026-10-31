//! Optional, host-owned OPA authorization for read queries.
//!
//! Decisions use Trino's context/action envelope. Policies are consulted once per
//! distinct resource per query, before relational optimization and execution.
use crate::{compiler::CompileRequest, ir::rel::mapping::GraphMapping};
use datafusion::{
    common::{
        DataFusionError, TableReference,
        tree_node::{Transformed, TreeNodeRecursion},
    },
    logical_expr::{Expr, ExprSchemable, LogicalPlan, LogicalPlanBuilder},
};
use futures::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// Identity authenticated by the embedding application, never by query text.
#[derive(Clone, Debug, Serialize)]
pub struct Identity {
    pub user: String,
    pub groups: BTreeSet<String>,
}
impl Identity {
    pub fn new(user: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            groups: BTreeSet::new(),
        }
    }
}

/// OPA decision URLs, not an OPA server base URL. Optional endpoints enable
/// resource batching, row filters, and column masks independently.
#[derive(Clone, Debug)]
pub struct Config {
    pub policy_uri: String,
    pub batched_uri: Option<String>,
    pub row_filters_uri: Option<String>,
    pub column_masking_uri: Option<String>,
    pub batch_column_masking_uri: Option<String>,
    pub timeout: Duration,
    /// Shared across all queries using this client.
    pub max_concurrent_requests: usize,
    pub max_batch_size: usize,
    pub max_response_bytes: usize,
    pub default_catalog: String,
    pub default_schema: String,
    pub context: BTreeMap<String, String>,
}
impl Config {
    pub fn new(policy_uri: impl Into<String>) -> Self {
        Self {
            policy_uri: policy_uri.into(),
            batched_uri: None,
            row_filters_uri: None,
            column_masking_uri: None,
            batch_column_masking_uri: None,
            timeout: Duration::from_secs(10),
            max_concurrent_requests: 16,
            max_batch_size: 256,
            max_response_bytes: 1024 * 1024,
            default_catalog: "orchiddb".into(),
            default_schema: "default".into(),
            context: BTreeMap::new(),
        }
    }
}

/// Reusable pooled HTTP transport. No decision cache survives a query.
#[derive(Debug)]
pub struct Client {
    config: Config,
    http: reqwest::Client,
    permits: tokio::sync::Semaphore,
}
impl Client {
    pub fn new(config: Config) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(config.timeout.min(Duration::from_secs(3)))
            .build()
            .map_err(|_| "OPA: cannot create HTTP client")?;
        Self::with_http_client(config, http)
    }
    /// Supply a trusted HTTP client for TLS roots, bearer headers, or proxies.
    /// Configure redirects off when using credentials.
    pub fn with_http_client(config: Config, http: reqwest::Client) -> Result<Self, String> {
        if config.timeout.is_zero()
            || config.max_concurrent_requests == 0
            || config.max_batch_size == 0
            || config.max_response_bytes == 0
        {
            return Err("OPA: timeout and resource limits must be positive".into());
        }
        if config.column_masking_uri.is_some() && config.batch_column_masking_uri.is_some() {
            return Err("OPA: choose individual or batch column masking".into());
        }
        for uri in std::iter::once(&config.policy_uri)
            .chain(config.batched_uri.iter())
            .chain(config.row_filters_uri.iter())
            .chain(config.column_masking_uri.iter())
            .chain(config.batch_column_masking_uri.iter())
        {
            let url = reqwest::Url::parse(uri).map_err(|_| "OPA: invalid policy URL")?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    "OPA: policy URLs must use HTTP(S), without userinfo or fragments".into(),
                );
            }
        }
        if config.default_catalog.is_empty() || config.default_schema.is_empty() {
            return Err("OPA: default catalog and schema must be nonempty".into());
        }
        Ok(Self {
            permits: tokio::sync::Semaphore::new(config.max_concurrent_requests),
            config,
            http,
        })
    }
    async fn decision(&self, uri: &str, context: &Value, action: Value) -> Result<Value, String> {
        // Include semaphore queueing in the deadline; a saturated policy service
        // must not hold query execution open indefinitely.
        tokio::time::timeout(self.config.timeout, async {
            let _permit = self
                .permits
                .acquire()
                .await
                .map_err(|_| "OPA: client closed")?;
            let mut response = self
                .http
                .post(uri)
                .json(&json!({"input":{"context":context,"action":action}}))
                .send()
                .await
                .map_err(|e| format!("OPA: policy request failed: {:?}", e.without_url()))?;
            if !response.status().is_success() {
                return Err(format!(
                    "OPA: policy service returned HTTP {}",
                    response.status().as_u16()
                ));
            }
            if response
                .content_length()
                .is_some_and(|n| n > self.config.max_response_bytes as u64)
            {
                return Err("OPA: response size limit exceeded".into());
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "OPA: could not read policy response")?
            {
                if chunk.len() > self.config.max_response_bytes.saturating_sub(bytes.len()) {
                    return Err("OPA: response size limit exceeded".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            let mut body: Value =
                serde_json::from_slice(&bytes).map_err(|_| "OPA: invalid JSON response")?;
            body.as_object_mut()
                .and_then(|o| o.remove("result"))
                .ok_or_else(|| "OPA: undefined policy result".into())
        })
        .await
        .map_err(|_| "OPA: policy request timed out".to_owned())?
    }
}

/// A connection's policy and authenticated identity. Construct once and clone
/// cheaply for each execution; query IDs and decisions remain query-local.
#[derive(Clone, Debug)]
pub struct Policy {
    client: Arc<Client>,
    identity: Identity,
}
impl Policy {
    pub fn new(client: Arc<Client>, identity: Identity) -> Result<Self, String> {
        if identity.user.trim().is_empty() {
            return Err("OPA: authenticated user is required".into());
        }
        Ok(Self { client, identity })
    }
    pub(crate) async fn begin(&self, request: &CompileRequest) -> Result<Value, String> {
        // These sources may expand to executable code or select a different
        // physical source after this authorization boundary. Never silently
        // authorize only a placeholder for those operations.
        if request.managed_table.is_some()
            || !request.procedures.is_empty()
            || !request.functions.is_empty()
            || !request.logical_sources.is_empty()
            || !request.representation_sources.is_empty()
        {
            return Err("OPA: managed stores, host functions/procedures, and alternate physical source layouts are not supported on protected connections".into());
        }
        static NEXT_QUERY: AtomicU64 = AtomicU64::new(1);
        let context = json!({"identity":self.identity,"queryId":format!("orchid-{}-{}",std::process::id(),NEXT_QUERY.fetch_add(1,Ordering::Relaxed)),
            "softwareStack":{"orchiddbVersion":env!("CARGO_PKG_VERSION")},"custom":self.client.config.context});
        let allow = self
            .client
            .decision(
                &self.client.config.policy_uri,
                &context,
                json!({"operation":"ExecuteQuery"}),
            )
            .await?;
        require_allow(&allow)?;
        Ok(context)
    }
    pub(crate) async fn enforce(
        &self,
        plan: LogicalPlan,
        mapping: &GraphMapping,
        context: &Value,
    ) -> Result<LogicalPlan, String> {
        // Only prune projections; no predicate simplification, join elimination,
        // or placement may occur before policy filters/masks are installed.
        let plan = datafusion::optimizer::Optimizer::with_rules(vec![
            crate::ir::rel::optimizer::projection_rule(),
        ])
        .optimize(
            plan,
            &datafusion::optimizer::OptimizerContext::new().with_skip_failing_rules(false),
            |_, _| {},
        )
        .map_err(|e| e.to_string())?;
        let external: BTreeSet<_> = mapping
            .physical_table_names()
            .iter()
            .map(|name| TableReference::from(name.as_str()).to_string())
            .collect();
        let mut recursive_sources = BTreeSet::new();
        plan.apply_with_subqueries(|node| {
            if let LogicalPlan::RecursiveQuery(recursive) = node {
                recursive_sources.insert(recursive.name.clone());
            }
            Ok(TreeNodeRecursion::Continue)
        })
        .map_err(|e| e.to_string())?;
        let mut tables: BTreeMap<String, Resource> = BTreeMap::new();
        plan.apply_with_subqueries(|node| {
            if let LogicalPlan::TableScan(scan) = node {
                let name = scan.table_name.to_string();
                let generated_range = datafusion::datasource::source_as_provider(&scan.source)
                    .ok()
                    .is_some_and(|p| p.as_any().is::<crate::ir::rel::range::IntegerRange>());
                // Only compiler-owned range and recursive work tables are exempt.
                // An unfamiliar or differently quoted source must still be authorized.
                if !generated_range
                    && (external.contains(&name) || !recursive_sources.contains(&name))
                {
                    let entry = tables
                        .entry(name)
                        .or_insert_with(|| Resource::new(&scan.table_name, &self.client.config));
                    for field in scan.projected_schema.fields() {
                        entry
                            .columns
                            .insert(field.name().clone(), field.data_type().clone());
                    }
                    for filter in &scan.filters {
                        for column in filter.column_refs() {
                            let field = scan
                                .source
                                .schema()
                                .field_with_name(&column.name)
                                .map_err(|e| DataFusionError::Plan(e.to_string()))?
                                .clone();
                            entry
                                .columns
                                .insert(field.name().clone(), field.data_type().clone());
                        }
                    }
                }
            }
            Ok(TreeNodeRecursion::Continue)
        })
        .map_err(|e| e.to_string())?;
        let decisions = self.resources(context, &tables).await?;
        apply(plan, mapping, &decisions)
    }
    async fn resources(
        &self,
        context: &Value,
        tables: &BTreeMap<String, Resource>,
    ) -> Result<BTreeMap<String, Restrictions>, String> {
        let config = &self.client.config;
        let mut jobs = Vec::new();
        let entries = tables.iter().collect::<Vec<_>>();
        if let Some(uri) = &config.batched_uri {
            for batch in entries.chunks(config.max_batch_size) {
                jobs.push(Job { uri: uri.clone(), action: json!({"operation":"SelectFromColumns","filterResources":batch.iter().map(|(_,r)|r.table(true)).collect::<Vec<_>>()}), kind: Kind::BatchAllow(batch.len()) });
            }
        } else {
            for (_, resource) in &entries {
                jobs.push(Job { uri: config.policy_uri.clone(), action: json!({"operation":"SelectFromColumns","resource":resource.table(true)}), kind: Kind::Allow });
            }
        }
        for (name, resource) in entries {
            if let Some(uri) = &config.row_filters_uri {
                jobs.push(Job {
                    uri: uri.clone(),
                    action: json!({"operation":"GetRowFilters","resource":resource.table(false)}),
                    kind: Kind::Rows(name.clone()),
                });
            }
            if let Some(uri) = &config.batch_column_masking_uri {
                let columns = resource.columns.keys().cloned().collect::<Vec<_>>();
                for batch in columns.chunks(config.max_batch_size) {
                    jobs.push(Job { uri: uri.clone(), action: json!({"operation":"GetColumnMask","filterResources":batch.iter().map(|c|resource.column(c)).collect::<Vec<_>>()}), kind: Kind::Masks(name.clone(), batch.to_vec()) });
                }
            } else if let Some(uri) = &config.column_masking_uri {
                for column in resource.columns.keys() {
                    jobs.push(Job { uri: uri.clone(), action: json!({"operation":"GetColumnMask","resource":resource.column(column)}), kind: Kind::Mask(name.clone(), column.clone()) });
                }
            }
        }
        let results: Vec<_> = stream::iter(jobs)
            .map(|job| async move {
                let value = self.client.decision(&job.uri, context, job.action).await?;
                Ok::<_, String>((job.kind, value))
            })
            .buffer_unordered(config.max_concurrent_requests)
            .try_collect()
            .await?;
        let mut restrictions: BTreeMap<String, Restrictions> = tables
            .keys()
            .map(|name| (name.clone(), Restrictions::default()))
            .collect();
        for (kind, value) in results {
            match kind {
                Kind::Allow => require_allow(&value)?,
                Kind::BatchAllow(len) => {
                    let indices: Vec<usize> = serde_json::from_value(value)
                        .map_err(|_| "OPA: invalid batch authorization result")?;
                    let allowed: BTreeSet<_> = indices.iter().copied().collect();
                    if allowed.len() != indices.len() || allowed.iter().any(|i| *i >= len) {
                        return Err("OPA: invalid batch authorization indices".into());
                    }
                    if allowed.len() != len {
                        return Err("OPA: access denied".into());
                    }
                }
                Kind::Rows(name) => {
                    let rows: Vec<ViewExpression> =
                        serde_json::from_value(value).map_err(|_| "OPA: invalid row filters")?;
                    for row in &rows {
                        row.validate()?;
                    }
                    restrictions.get_mut(&name).unwrap().rows = rows;
                }
                Kind::Mask(name, column) => {
                    if !value.is_null() {
                        let mask: ViewExpression = serde_json::from_value(value)
                            .map_err(|_| "OPA: invalid column mask")?;
                        mask.validate()?;
                        restrictions
                            .get_mut(&name)
                            .unwrap()
                            .masks
                            .insert(column, mask);
                    }
                }
                Kind::Masks(name, columns) => {
                    let masks: Vec<IndexedMask> = serde_json::from_value(value)
                        .map_err(|_| "OPA: invalid batch column masks")?;
                    let target = &mut restrictions.get_mut(&name).unwrap().masks;
                    for mask in masks {
                        mask.view_expression.validate()?;
                        let column = columns
                            .get(mask.index)
                            .ok_or("OPA: mask index out of range")?;
                        if target
                            .insert(column.clone(), mask.view_expression)
                            .is_some()
                        {
                            return Err("OPA: duplicate column mask".into());
                        }
                    }
                }
            }
        }
        Ok(restrictions)
    }
}
fn require_allow(value: &Value) -> Result<(), String> {
    match value {
        Value::Bool(true) => Ok(()),
        Value::Bool(false) | Value::Null => Err("OPA: access denied".into()),
        _ => Err("OPA: authorization result must be boolean".into()),
    }
}
struct Job {
    uri: String,
    action: Value,
    kind: Kind,
}
enum Kind {
    Allow,
    BatchAllow(usize),
    Rows(String),
    Mask(String, String),
    Masks(String, Vec<String>),
}
#[derive(Default)]
struct Restrictions {
    rows: Vec<ViewExpression>,
    masks: BTreeMap<String, ViewExpression>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewExpression {
    expression: String,
    #[serde(default)]
    identity: Option<String>,
}
impl ViewExpression {
    fn validate(&self) -> Result<(), String> {
        if self.expression.trim().is_empty() {
            return Err("OPA: empty policy expression".into());
        }
        if self.identity.is_some() {
            return Err("OPA: policy expression impersonation is unsupported".into());
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IndexedMask {
    index: usize,
    view_expression: ViewExpression,
}
struct Resource {
    catalog: String,
    schema: String,
    table: String,
    columns: BTreeMap<String, arrow::datatypes::DataType>,
}
impl Resource {
    fn new(table: &TableReference, config: &Config) -> Self {
        Self {
            catalog: table.catalog().unwrap_or(&config.default_catalog).into(),
            schema: table.schema().unwrap_or(&config.default_schema).into(),
            table: table.table().into(),
            columns: BTreeMap::new(),
        }
    }
    fn table(&self, columns: bool) -> Value {
        let mut table =
            json!({"catalogName":self.catalog,"schemaName":self.schema,"tableName":self.table});
        if columns {
            table["columns"] = json!(self.columns.keys().collect::<Vec<_>>());
        }
        json!({"table":table})
    }
    fn column(&self, name: &str) -> Value {
        json!({"column":{"catalogName":self.catalog,"schemaName":self.schema,"tableName":self.table,"columnName":name,
            "columnType":crate::ir::rel::sql::SqlDialect::DuckDb.sql_type(&self.columns[name]).unwrap_or_else(|_| self.columns[name].to_string())}})
    }
}
fn apply(
    plan: LogicalPlan,
    mapping: &GraphMapping,
    decisions: &BTreeMap<String, Restrictions>,
) -> Result<LogicalPlan, String> {
    plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::TableScan(scan) = &node else {
            return Ok(Transformed::no(node));
        };
        let Some(policy) = decisions.get(&scan.table_name.to_string()) else {
            return Ok(Transformed::no(node));
        };
        if policy.rows.is_empty() && policy.masks.is_empty() {
            return Ok(Transformed::no(node));
        }
        let invalid =
            |e: String| DataFusionError::Plan(format!("OPA: invalid policy expression: {e}"));
        // Start from the full raw row. Masks must precede user predicates and
        // LIMIT, including predicates already embedded in a mapped SQL scan.
        let raw = LogicalPlanBuilder::scan(scan.table_name.clone(), scan.source.clone(), None)?
            .build()?;
        let mut secured = raw;
        for row in &policy.rows {
            let expression = mapping
                .bind_scalar(&row.expression, secured.schema())
                .map_err(|e| invalid(e.to_string()))?;
            if expression.get_type(secured.schema())? != arrow::datatypes::DataType::Boolean {
                return Err(invalid("row filter must be boolean".into()));
            }
            secured = LogicalPlanBuilder::from(secured)
                .filter(expression)?
                .build()?;
        }
        let mut projection = Vec::new();
        for field in secured.schema().fields() {
            let expression = if let Some(mask) = policy.masks.get(field.name()) {
                let expression = mapping
                    .bind_scalar(&mask.expression, secured.schema())
                    .map_err(|e| invalid(e.to_string()))?;
                expression.cast_to(field.data_type(), secured.schema())?
            } else {
                Expr::Column(datafusion::common::Column::new(
                    Some(scan.table_name.clone()),
                    field.name(),
                ))
            };
            projection.push(expression.alias(field.name()));
        }
        secured = LogicalPlanBuilder::from(secured)
            .project(projection)?
            .alias(scan.table_name.clone())?
            .build()?;
        for filter in &scan.filters {
            secured = LogicalPlanBuilder::from(secured)
                .filter(filter.clone())?
                .build()?;
        }
        if let Some(projection) = &scan.projection {
            let full = scan.source.schema();
            secured = LogicalPlanBuilder::from(secured)
                .project(
                    projection
                        .iter()
                        .map(|i| {
                            Expr::Column(datafusion::common::Column::new(
                                Some(scan.table_name.clone()),
                                full.field(*i).name(),
                            ))
                        })
                        .collect::<Vec<_>>(),
                )?
                .build()?;
        }
        if let Some(fetch) = scan.fetch {
            secured = LogicalPlanBuilder::from(secured)
                .limit(0, Some(fetch))?
                .build()?;
        }
        Ok(Transformed::yes(secured))
    })
    .map(|p| p.data)
    .map_err(|e| e.to_string())
}
