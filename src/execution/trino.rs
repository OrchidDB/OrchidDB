//! Trino executable-engine session. SQL executes remotely; results and input
//! exchanges are consumed incrementally without a DataFusion execution context.
use super::{
    exchange::{ExchangeLease, ExchangeRegistry},
    stream::{BatchStream, InputRelation},
};
use arrow::datatypes::SchemaRef;
use futures::{StreamExt, stream};
use reqwest::{Client, Method, Url};
use serde_json::Value;
use std::{collections::VecDeque, sync::Arc, time::Duration};
mod codec;

#[derive(Clone)]
pub struct TrinoOptions {
    pub endpoint: String,
    pub user: String,
    pub bearer_token: Option<String>,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    /// Catalog where the Orchid Arrow connector is installed.
    pub exchange_catalog: String,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub max_response_bytes: usize,
    pub result_batch_rows: usize,
}
impl TrinoOptions {
    pub fn new(endpoint: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            user: user.into(),
            bearer_token: None,
            catalog: None,
            schema: None,
            exchange_catalog: "orchid".into(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            max_response_bytes: 64 * 1024 * 1024,
            result_batch_rows: 1024,
        }
    }
}
impl std::fmt::Debug for TrinoOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrinoOptions")
            .field("endpoint", &self.endpoint)
            .field("user", &self.user)
            .field("authenticated", &self.bearer_token.is_some())
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct TrinoSession {
    client: Client,
    options: Arc<TrinoOptions>,
    endpoint: Url,
    exchanges: ExchangeRegistry,
}
impl TrinoSession {
    pub fn new(options: TrinoOptions, exchanges: ExchangeRegistry) -> Result<Self, String> {
        let endpoint = Url::parse(&options.endpoint).map_err(|_| "invalid Trino endpoint")?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || options.user.is_empty()
            || options.exchange_catalog.is_empty()
            || options.result_batch_rows == 0
            || options.max_response_bytes == 0
            || options.connect_timeout.is_zero()
            || options.request_timeout.is_zero()
        {
            return Err("invalid Trino endpoint/options".into());
        }
        let client = Client::builder()
            .connect_timeout(options.connect_timeout)
            .timeout(options.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            options: Arc::new(options),
            endpoint,
            exchanges,
        })
    }
    pub async fn execute(
        &self,
        sql: &str,
        inputs: Vec<InputRelation>,
    ) -> Result<BatchStream, String> {
        let (sql, leases) =
            bind_inputs(sql, inputs, &self.options.exchange_catalog, &self.exchanges)?;
        let url = self
            .endpoint
            .join("/v1/statement")
            .map_err(|e| e.to_string())?;
        let mut guard = QueryGuard {
            client: self.client.clone(),
            options: self.options.clone(),
            endpoint: self.endpoint.clone(),
            next: None,
            _leases: leases,
        };
        let mut page = request(&guard, Method::POST, url, Some(sql)).await?;
        loop {
            update_page(&mut guard, &page)?;
            if let Some(columns) = page.get("columns") {
                let schema = codec::schema(columns)?;
                let rows = take_rows(&mut page)?;
                let cursor = Cursor {
                    guard,
                    schema: schema.clone(),
                    rows,
                };
                let batches = stream::try_unfold(cursor, |mut cursor| async move {
                    loop {
                        if !cursor.rows.is_empty() {
                            let count = cursor
                                .guard
                                .options
                                .result_batch_rows
                                .min(cursor.rows.len());
                            let rows = cursor.rows.drain(..count).collect::<Vec<_>>();
                            let batch = codec::batch(cursor.schema.clone(), &rows)?;
                            return Ok(Some((batch, cursor)));
                        }
                        let Some(next) = cursor.guard.next.clone() else {
                            return Ok(None);
                        };
                        let mut page = request(&cursor.guard, Method::GET, next, None).await?;
                        update_page(&mut cursor.guard, &page)?;
                        if let Some(columns) = page.get("columns") {
                            if codec::schema(columns)? != cursor.schema {
                                return Err("Trino changed result schema".into());
                            }
                        }
                        cursor.rows = take_rows(&mut page)?;
                    }
                });
                return Ok(BatchStream::new(schema, batches));
            }
            let Some(next) = guard.next.clone() else {
                // Statements with no output still have a typed, empty stream.
                let schema = Arc::new(arrow::datatypes::Schema::empty());
                return Ok(BatchStream::new(schema, stream::empty()));
            };
            page = request(&guard, Method::GET, next, None).await?;
        }
    }
}
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for TrinoSession {
    fn supports_concurrent_statements(&self) -> bool {
        true
    }
    fn dialect(&self) -> &str {
        "trino"
    }
    async fn query_stream(
        &mut self,
        sql: &str,
        inputs: Vec<InputRelation>,
    ) -> Result<BatchStream, String> {
        self.execute(sql, inputs).await
    }
    async fn query_requests_stream(
        &mut self,
        queries: crate::federation::QueryStream,
        columns: Vec<crate::federation::TransferColumn>,
    ) -> Result<BatchStream, String> {
        let fields = columns
            .iter()
            .map(|column| {
                Ok(arrow::datatypes::Field::new(
                    &column.name,
                    crate::compiler::data_type(&column.data_type)?,
                    column.nullable,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let schema = Arc::new(arrow::datatypes::Schema::new(fields));
        let expected = schema.clone();
        let batches = stream::try_unfold(
            (self.clone(), queries, None::<BatchStream>),
            move |(session, mut queries, mut active)| {
                let schema = expected.clone();
                async move {
                    loop {
                        if let Some(current) = &mut active {
                            if let Some(batch) = current.next().await {
                                let batch = batch?;
                                if batch.num_columns() != schema.fields().len() {
                                    return Err("dependent Trino result width mismatch".into());
                                }
                                let mut arrays = Vec::new();
                                for (array, field) in batch.columns().iter().zip(schema.fields()) {
                                    let cast = arrow::compute::cast(array, field.data_type())
                                        .map_err(|e| e.to_string())?;
                                    if array.data_type() != field.data_type() {
                                        let back = arrow::compute::cast(&cast, array.data_type())
                                            .map_err(|e| e.to_string())?;
                                        if back.as_ref() != array.as_ref() {
                                            return Err("dependent Trino result conversion would lose information".into());
                                        }
                                    }
                                    arrays.push(cast);
                                }
                                let batch = arrow::record_batch::RecordBatch::try_new_with_options(
                                    schema.clone(),
                                    arrays,
                                    &arrow::record_batch::RecordBatchOptions::new()
                                        .with_row_count(Some(batch.num_rows())),
                                )
                                .map_err(|e| e.to_string())?;
                                return Ok(Some((batch, (session, queries, active))));
                            }
                        }
                        let Some(query) = queries.next().await else {
                            return Ok(None);
                        };
                        active = Some(session.execute(&query?, vec![]).await?);
                    }
                }
            },
        );
        Ok(BatchStream::new(schema, batches))
    }
}
struct Cursor {
    guard: QueryGuard,
    schema: SchemaRef,
    rows: VecDeque<Value>,
}
struct QueryGuard {
    client: Client,
    options: Arc<TrinoOptions>,
    endpoint: Url,
    next: Option<Url>,
    _leases: Vec<ExchangeLease>,
}
impl Drop for QueryGuard {
    fn drop(&mut self) {
        if let Some(next) = self.next.take() {
            let client = self.client.clone();
            let options = self.options.clone();
            let cancel = async move {
                let _ = headers(client.delete(next), &options).send().await;
            };
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(cancel);
            } else {
                std::thread::spawn(move || {
                    if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        runtime.block_on(cancel)
                    }
                });
            }
        }
        // Leases revoke input streams even if best-effort remote DELETE fails.
    }
}
fn headers(
    mut request: reqwest::RequestBuilder,
    options: &TrinoOptions,
) -> reqwest::RequestBuilder {
    request = request
        .header("X-Trino-User", &options.user)
        .header("X-Trino-Source", "OrchidDB")
        .header("X-Trino-Client-Capabilities", "PARAMETRIC_DATETIME");
    if let Some(token) = &options.bearer_token {
        request = request.bearer_auth(token);
    }
    if let Some(catalog) = &options.catalog {
        request = request.header("X-Trino-Catalog", catalog);
    }
    if let Some(schema) = &options.schema {
        request = request.header("X-Trino-Schema", schema);
    }
    request
}
async fn request(
    guard: &QueryGuard,
    method: Method,
    url: Url,
    sql: Option<String>,
) -> Result<Value, String> {
    let mut builder = headers(guard.client.request(method, url), &guard.options);
    if let Some(sql) = sql {
        builder = builder.header("Content-Type", "text/plain").body(sql);
    }
    let mut response = builder
        .send()
        .await
        .map_err(|e| format!("Trino HTTP: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Trino HTTP status {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > guard.options.max_response_bytes as u64)
    {
        return Err("Trino response exceeds byte limit".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len().saturating_add(chunk.len()) > guard.options.max_response_bytes {
            return Err("Trino response exceeds byte limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|e| format!("Trino response JSON: {e}"))
}
fn update_page(guard: &mut QueryGuard, page: &Value) -> Result<(), String> {
    // Save continuation before surfacing an error so drop can cancel the query.
    guard.next = match page.get("nextUri").and_then(Value::as_str) {
        Some(uri) => {
            let url = Url::parse(uri).map_err(|_| "invalid Trino continuation URI")?;
            if url.origin() != guard.endpoint.origin() {
                return Err("Trino continuation changed origin".into());
            }
            Some(url)
        }
        None => None,
    };
    if let Some(error) = page.get("error") {
        return Err(format!(
            "Trino {}: {}",
            error
                .get("errorName")
                .and_then(Value::as_str)
                .unwrap_or("query error"),
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
        ));
    }
    Ok(())
}
fn take_rows(page: &mut Value) -> Result<VecDeque<Value>, String> {
    match page.get_mut("data").map(Value::take) {
        None | Some(Value::Null) => Ok(VecDeque::new()),
        Some(Value::Array(rows)) => Ok(rows.into()),
        _ => Err("unsupported Trino result encoding; direct row protocol required".into()),
    }
}
fn bind_inputs(
    sql: &str,
    inputs: Vec<InputRelation>,
    catalog: &str,
    registry: &ExchangeRegistry,
) -> Result<(String, Vec<ExchangeLease>), String> {
    if inputs.is_empty() {
        return Ok((sql.into(), vec![]));
    }
    use datafusion::sql::sqlparser::{ast, parser::Parser};
    let dialect = crate::ir::rel::sql::SqlDialect::resolve("trino").map_err(|e| e.to_string())?;
    let parser_dialect = dialect.parser_dialect();
    let mut statements = Parser::parse_sql(parser_dialect.as_ref(), sql).map_err(|e| e.to_string())?;
    if statements.len() != 1 {
        return Err("exchange binding requires one SQL statement".into());
    }
    let ast::Statement::Query(query) = &mut statements[0] else {
        return Err("exchange binding requires a query".into());
    };
    let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
    let mut names = query
        .with
        .as_ref()
        .map(|w| {
            w.cte_tables
                .iter()
                .map(|c| c.alias.name.value.to_lowercase())
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut bindings = Vec::new();
    let mut leases = Vec::new();
    for input in inputs {
        if !names.insert(input.name.to_lowercase()) {
            return Err("duplicate exchange binding".into());
        }
        let projection = if input.source.schema().fields().is_empty() {
            "TRUE AS __orchid_unit"
        } else {
            "*"
        };
        // Trino requires column aliases on every CTE in a recursive WITH,
        // including nonrecursive exchange relations prepended by the host.
        let columns = if input.source.schema().fields().is_empty() {
            "__orchid_unit".to_string()
        } else {
            input.source.schema().fields().iter().map(|field| quote(field.name())).collect::<Vec<_>>().join(", ")
        };
        let estimate = input.source.estimated_rows();
        let lease = registry.register(vec![input.source], estimate)?;
        let binding = format!(
            "WITH {} ({columns}) AS (SELECT {projection} FROM {}.exchange.{}) SELECT 1",
            quote(&input.name),
            quote(catalog),
            quote(lease.id())
        );
        let mut parsed = Parser::parse_sql(parser_dialect.as_ref(), &binding).map_err(|e| e.to_string())?;
        let ast::Statement::Query(binding) = parsed.remove(0) else {
            unreachable!()
        };
        bindings.push(binding.with.unwrap().cte_tables.remove(0));
        leases.push(lease);
    }
    if let Some(existing) = &mut query.with {
        bindings.append(&mut existing.cte_tables);
        existing.cte_tables = bindings;
    } else {
        query.with = Some(ast::With {
            with_token: ast::helpers::attached_token::AttachedToken::empty(),
            recursive: false,
            cte_tables: bindings,
        });
    }
    Ok((statements.remove(0).to_string(), leases))
}

impl std::fmt::Debug for TrinoSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrinoSession")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}
impl crate::ir::rel::sql::RegionSession for TrinoSession {
    fn supports_concurrent_statements(&self) -> bool {
        true
    }
    fn dialect(&self) -> crate::ir::rel::sql::SqlDialect {
        crate::ir::rel::sql::SqlDialect::Custom(&crate::ir::rel::sql::trino::TRINO)
    }
    fn query(
        &mut self,
        sql: &str,
        schema: SchemaRef,
    ) -> crate::ir::rel::sql::SqlResult<arrow::record_batch::RecordBatch> {
        let stream = self.query_stream(sql, schema.clone(), vec![])?;
        let batches = futures::executor::block_on(stream.collect_batches())
            .map_err(crate::ir::rel::sql::SqlError::Execution)?;
        arrow::compute::concat_batches(&schema, &batches).map_err(Into::into)
    }
    fn query_stream(
        &mut self,
        sql: &str,
        schema: SchemaRef,
        inputs: Vec<InputRelation>,
    ) -> crate::ir::rel::sql::SqlResult<BatchStream> {
        use futures::SinkExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        // RegionSession is synchronous; a dedicated I/O task bridges it to the
        // same async session with bounded prefetch. No SQL or row execution here.
        let session = self.clone();
        let sql = sql.to_owned();
        let expected = schema.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let (mut sender, receiver) = futures::channel::mpsc::channel(2);
        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = sender.try_send(Err(error.to_string()));
                    return;
                }
            };
            runtime.block_on(async {
                let produce=async {
                    let mut stream=session.execute(&sql,inputs).await?;
                    while let Some(batch)=stream.next().await {
                        let batch=batch?;
                        if batch.num_columns()!=expected.fields().len(){return Err("Trino SQL region result width mismatch".to_string());}
                        let arrays=batch.columns().iter().zip(expected.fields()).map(|(array,field)|crate::ir::rel::dag::coerce_sql_array(array,field.data_type()).map_err(|e|e.to_string())).collect::<Result<Vec<_>,_>>()?;
                        let batch=arrow::record_batch::RecordBatch::try_new_with_options(expected.clone(),arrays,&arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(batch.num_rows()))).map_err(|e|e.to_string())?;
                        sender.send(Ok(batch)).await.map_err(|_|"Trino region consumer closed".to_string())?;
                    }
                    Ok::<(),String>(())
                };
                let result=tokio::select! {result=produce=>result,_=async{while !stop.load(Ordering::Acquire){tokio::time::sleep(Duration::from_millis(10)).await;}}=>Ok(())};
                if let Err(error)=result{let _=sender.send(Err(error)).await;}
                // Allow a dropped query guard's cancellation DELETE to be sent.
                tokio::time::sleep(Duration::from_millis(25)).await;
            });
        });
        struct Cancel(Arc<AtomicBool>);
        impl Drop for Cancel {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let guard = Cancel(cancelled);
        Ok(BatchStream::new(
            schema,
            stream::unfold((receiver, guard), |(mut receiver, guard)| async move {
                receiver
                    .next()
                    .await
                    .map(|batch| (batch, (receiver, guard)))
            }),
        ))
    }
}
