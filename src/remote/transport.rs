//! HTTP sessions for request islands. Requests are structured values, never SQL.
//! Pagination uses a snapshot scroll; backend failures never return partial rows.
use crate::federation::{Session, TransferColumn};
use arrow::{
    array::{ArrayRef, StructArray},
    datatypes::{DataType, Field, Schema},
    record_batch::{RecordBatch, RecordBatchOptions},
};
use datafusion::common::ScalarValue;
use reqwest::{
    Client, Method, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Clone)]
pub enum Authentication {
    None,
    Basic { username: String, password: String },
    Bearer(String),
    ApiKey(String),
}
impl std::fmt::Debug for Authentication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Basic { .. } => "Basic(<redacted>)",
            Self::Bearer(_) => "Bearer(<redacted>)",
            Self::ApiKey(_) => "ApiKey(<redacted>)",
        })
    }
}
#[derive(Clone, Debug)]
pub struct HttpOptions {
    pub endpoint: String,
    pub authentication: Authentication,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    /// Number of documents per snapshot page for unbounded scans.
    pub page_size: usize,
    /// Maximum independent bounded requests per _msearch call.
    pub batch_size: usize,
    pub max_request_bytes: usize,
    pub scroll_keep_alive: String,
    /// Maximum encoded response size for one page, including error responses.
    pub max_response_bytes: usize,
    /// An optional admission limit. Exceeding it errors rather than truncates.
    pub max_rows: Option<usize>,
}
impl HttpOptions {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            authentication: Authentication::None,
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            page_size: 1000,
            batch_size: 64,
            max_request_bytes: 16 * 1024 * 1024,
            scroll_keep_alive: "1m".into(),
            max_response_bytes: 64 * 1024 * 1024,
            max_rows: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Engine {
    Quickwit,
    Elasticsearch,
}
impl Engine {
    fn name(self) -> &'static str {
        match self {
            Self::Quickwit => "quickwit",
            Self::Elasticsearch => "elasticsearch",
        }
    }
}
pub struct HttpSession {
    engine: Engine,
    options: HttpOptions,
    base: Url,
    client: Client,
    metadata: BTreeMap<String, Value>,
    settings: BTreeMap<String, Value>,
}
impl HttpSession {
    #[cfg(feature = "quickwit")]
    pub fn quickwit(endpoint: &str) -> Result<Self, String> {
        Self::new(Engine::Quickwit, HttpOptions::new(endpoint), None)
    }
    #[cfg(feature = "elasticsearch")]
    pub fn elasticsearch(endpoint: &str) -> Result<Self, String> {
        Self::new(Engine::Elasticsearch, HttpOptions::new(endpoint), None)
    }
    /// Execute an explicitly selected API. Path segments are encoded separately;
    /// paths cannot override the configured origin or authentication.
    pub async fn request_json(
        &self,
        method: Method,
        path: &[&str],
        body: Option<&Value>,
    ) -> Result<Value, String> {
        self.send(method, self.url(path)?, body).await
    }
    /// Revalidate mappings after an application changes an index or its alias.
    pub fn clear_metadata_cache(&mut self) {
        self.metadata.clear();
        self.settings.clear();
    }
    fn new(engine: Engine, options: HttpOptions, client: Option<Client>) -> Result<Self, String> {
        let base = Url::parse(&options.endpoint).map_err(|_| "invalid remote HTTP endpoint")?;
        if !matches!(base.scheme(), "http" | "https")
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err("remote endpoint must be HTTP(S), with credentials supplied through Authentication and no query or fragment".into());
        }
        if options.batch_size == 0
            || options.max_request_bytes == 0
            || options.page_size == 0
            || options.page_size > 10_000
            || options.max_response_bytes == 0
            || options.connect_timeout.is_zero()
            || options.request_timeout.is_zero()
            || options.scroll_keep_alive.is_empty()
        {
            return Err("invalid remote HTTP limits".into());
        }
        let client = match client {
            Some(client) => client,
            None => Client::builder()
                .connect_timeout(options.connect_timeout)
                .timeout(options.request_timeout)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| format!("HTTP client setup: {e}"))?,
        };
        Ok(Self {
            engine,
            options,
            base,
            client,
            metadata: BTreeMap::new(),
            settings: BTreeMap::new(),
        })
    }
    fn url(&self, segments: &[&str]) -> Result<Url, String> {
        let mut url = self.base.clone();
        let mut path = url
            .path_segments_mut()
            .map_err(|_| "remote endpoint cannot contain path segments")?;
        path.pop_if_empty();
        for segment in segments {
            if segment.is_empty() || matches!(*segment, "." | "..") {
                return Err("empty or relative remote path segment".into());
            }
            path.push(segment);
        }
        drop(path);
        Ok(url)
    }
    fn elastic_url(&self, segments: &[&str]) -> Result<Url, String> {
        let mut all = vec![];
        if self.engine == Engine::Quickwit {
            all.extend(["api", "v1", "_elastic"]);
        }
        all.extend_from_slice(segments);
        self.url(&all)
    }
    async fn send(&self, method: Method, url: Url, body: Option<&Value>) -> Result<Value, String> {
        self.send_payload(
            method,
            url,
            body.map(|body| ("application/json", body.to_string())),
        )
        .await
    }
    async fn send_payload(
        &self,
        method: Method,
        url: Url,
        body: Option<(&str, String)>,
    ) -> Result<Value, String> {
        self.send_payload_checked(method, url, body, true).await
    }
    async fn send_scroll(&self, method: Method, url: Url, body: &Value) -> Result<Value, String> {
        // The pagination loop captures the cursor before rejecting partial
        // responses, allowing cleanup even when the first page fails.
        self.send_payload_checked(
            method,
            url,
            Some(("application/json", body.to_string())),
            false,
        )
        .await
    }
    async fn send_payload_checked(
        &self,
        method: Method,
        url: Url,
        body: Option<(&str, String)>,
        check_partial: bool,
    ) -> Result<Value, String> {
        let mut request = self
            .client
            .request(method, url)
            .timeout(self.options.request_timeout);
        request = match &self.options.authentication {
            Authentication::None => request,
            Authentication::Basic { username, password } => {
                request.basic_auth(username, Some(password))
            }
            Authentication::Bearer(token) => request.bearer_auth(token),
            Authentication::ApiKey(key) => {
                let mut headers = HeaderMap::new();
                let mut value = HeaderValue::from_str(&format!("ApiKey {key}"))
                    .map_err(|_| "invalid API key header")?;
                value.set_sensitive(true);
                headers.insert(AUTHORIZATION, value);
                request.headers(headers)
            }
        };
        if let Some((content_type, body)) = body {
            if body.len() > self.options.max_request_bytes {
                return Err("remote request exceeds configured byte limit".into());
            }
            request = request
                .header(reqwest::header::CONTENT_TYPE, content_type)
                .body(body);
        }
        let mut response = request.send().await.map_err(|e| {
            format!(
                "{} HTTP request failed: {}",
                self.engine.name(),
                e.without_url()
            )
        })?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|n| n > self.options.max_response_bytes as u64)
        {
            return Err("remote response exceeds configured byte limit".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("remote response read failed: {}", e.without_url()))?
        {
            if bytes.len().saturating_add(chunk.len()) > self.options.max_response_bytes {
                return Err("remote response exceeds configured byte limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let decoded = serde_json::from_slice::<Value>(&bytes);
        if !status.is_success() {
            let detail = decoded
                .as_ref()
                .ok()
                .and_then(|v| v.get("error").or_else(|| v.get("message")))
                .map(ToString::to_string)
                .unwrap_or_else(|| String::from_utf8_lossy(&bytes).chars().take(1024).collect());
            return Err(format!(
                "{} HTTP {}: {}",
                self.engine.name(),
                status.as_u16(),
                detail
            ));
        }
        let value =
            decoded.map_err(|e| format!("invalid {} JSON response: {e}", self.engine.name()))?;
        if check_partial {
            reject_partial(&value)?;
        }
        Ok(value)
    }
    async fn execute(
        &mut self,
        request: &Value,
        columns: &[TransferColumn],
    ) -> Result<Vec<RecordBatch>, String> {
        let request = request
            .as_object()
            .ok_or("remote request must be an object")?;
        if request.get("version").and_then(Value::as_u64) != Some(1) {
            return Err("unsupported remote request version".into());
        }
        if request.get("engine").and_then(Value::as_str) != Some(self.engine.name()) {
            return Err("remote request engine does not match session".into());
        }
        let operation = request
            .get("operation")
            .and_then(Value::as_str)
            .ok_or("remote request requires operation")?;
        if !matches!(operation, "search" | "query") {
            return Err("unsupported remote request operation".into());
        }
        let index = request
            .get("index")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("remote request requires an index")?;
        let projections = request
            .get("columns")
            .and_then(Value::as_array)
            .ok_or("remote request requires columns")?;
        let projections = parameter_null_projections(projections, request.get("parameter_nulls"))?;
        let projections = projections.as_slice();
        validate_projections(projections, columns)?;
        let (limit, offset) = request_bounds(&Value::Object(request.clone()))?;
        let parameters = request
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let root = Value::Object(request.clone());
        if operation == "search" && limit == Some(0)
            || request
                .get("null_query_paths")
                .and_then(Value::as_array)
                .is_some_and(|paths| {
                    paths.iter().any(|p| {
                        p.as_str()
                            .and_then(|p| root.pointer(p))
                            .is_some_and(Value::is_null)
                    })
                })
        {
            return Ok(vec![decode_rows(
                &[],
                projections,
                columns,
                &parameters,
                false,
            )?]);
        }
        let mut body = bound_body(&root, &parameters)?;
        let projections = self.score_projections(projections, &body)?;
        let projections = projections.as_slice();
        if let Some(requirements) = request.get("requirements").and_then(Value::as_array) {
            self.validate_requirements(index, requirements).await?;
        }
        if operation == "query" {
            let api = request
                .get("api")
                .and_then(Value::as_str)
                .unwrap_or("elastic");
            let mut url = match api {
                "elastic" => self.elastic_url(&[index, "_search"])?,
                "native" if self.engine == Engine::Quickwit => {
                    self.url(&["api", "v1", index, "search"])?
                }
                _ => return Err("unknown explicit remote query API".into()),
            };
            if api == "elastic" {
                url.query_pairs_mut()
                    .append_pair("allow_partial_search_results", "false");
            } else {
                body["allow_failed_splits"] = json!(false);
            }
            let response = self.send(Method::POST, url, Some(&body)).await?;
            return Ok(vec![decode_rows(
                &[response],
                projections,
                columns,
                &parameters,
                true,
            )?]);
        }
        match request
            .get("api")
            .and_then(Value::as_str)
            .unwrap_or("elastic")
        {
            "native" => {
                if self.engine != Engine::Quickwit {
                    return Err("native Quickwit API requires a Quickwit session".into());
                }
                let limit=limit.ok_or("Quickwit native search requires a finite limit; snapshot scans use the elastic API")?;
                if projections.iter().any(|p| {
                    matches!(
                        p.get("source").and_then(Value::as_str),
                        Some("score" | "id")
                    )
                }) {
                    return Err("Quickwit native search does not expose score or durable document ID metadata; project stored fields or use elastic scored search".into());
                }
                body["max_hits"] = json!(limit);
                body["start_offset"] = json!(offset);
                body["allow_failed_splits"] = json!(false);
                let response = self
                    .send(
                        Method::POST,
                        self.url(&["api", "v1", index, "search"])?,
                        Some(&body),
                    )
                    .await?;
                let hits = response
                    .get("hits")
                    .and_then(Value::as_array)
                    .ok_or("Quickwit native response requires hits array")?;
                if hits.len() > limit {
                    return Err("remote response exceeded requested limit".into());
                }
                self.check_rows(hits.len())?;
                Ok(vec![decode_rows(
                    hits,
                    projections,
                    columns,
                    &parameters,
                    true,
                )?])
            }
            "elastic" => {
                self.elastic_search(
                    index,
                    body,
                    limit,
                    offset,
                    projections,
                    columns,
                    &parameters,
                )
                .await
            }
            _ => Err("unknown remote API".into()),
        }
    }
    fn check_rows(&self, count: usize) -> Result<(), String> {
        if self.options.max_rows.is_some_and(|max| count > max) {
            Err("remote result exceeds configured row limit".into())
        } else {
            Ok(())
        }
    }
    // Quickwit 0.9 returns BM25 in the matching sort slot, not `_score`.
    // Derive that slot from the actual request so another ordering cannot be
    // mistaken for relevance in explicitly supplied queries.
    fn score_projections(&self, projections: &[Value], body: &Value) -> Result<Vec<Value>, String> {
        let mut projections = projections.to_vec();
        if self.engine == Engine::Quickwit {
            for projection in &mut projections {
                if projection.get("source").and_then(Value::as_str) == Some("score") {
                    let index = body
                        .get("sort")
                        .and_then(Value::as_array)
                        .and_then(|sort| {
                            sort.iter().position(|entry| {
                                entry.as_str() == Some("_score") || entry.get("_score").is_some()
                            })
                        })
                        .ok_or("Quickwit score projection requires an explicit _score sort")?;
                    projection["sort_index"] = json!(index);
                }
            }
        }
        Ok(projections)
    }
    async fn elastic_search(
        &self,
        index: &str,
        mut body: Value,
        limit: Option<usize>,
        offset: usize,
        projections: &[Value],
        columns: &[TransferColumn],
        parameters: &[Value],
    ) -> Result<Vec<RecordBatch>, String> {
        let mut url = self.elastic_url(&[index, "_search"])?;
        // Quickwit requires this flag for scroll creation. Every response is
        // still checked by send()/reject_partial(), and nothing is returned
        // until the entire snapshot has completed successfully.
        url.query_pairs_mut().append_pair(
            "allow_partial_search_results",
            if self.engine == Engine::Quickwit && limit.is_none() {
                "true"
            } else {
                "false"
            },
        );
        if let Some(limit) = limit {
            self.check_rows(limit)?;
            body["size"] = json!(limit);
            body["from"] = json!(offset);
            let response = self.send(Method::POST, url, Some(&body)).await?;
            let hits = elastic_hits(&response)?;
            if hits.len() > limit {
                return Err("remote response exceeded requested limit".into());
            }
            return Ok(vec![decode_rows(
                hits,
                projections,
                columns,
                parameters,
                false,
            )?]);
        }
        if body.get("knn").is_some()
            || body.get("search_after").is_some()
            || body.get("pit").is_some()
        {
            return Err(
                "snapshot scans cannot combine knn, search_after, or caller-owned PIT pagination"
                    .into(),
            );
        }
        body.as_object_mut().unwrap().remove("from");
        body["size"] = json!(self.options.page_size);
        url.query_pairs_mut()
            .append_pair("scroll", &self.options.scroll_keep_alive);
        let mut scroll = None;
        let result: Result<Vec<RecordBatch>, String> = async {
            let mut response = self.send_scroll(Method::POST, url, &body).await?;
            let mut batches = Vec::new();
            let mut total = 0usize;
            let mut skip = offset;
            loop {
                if let Some(id) = response.get("_scroll_id").and_then(Value::as_str) {
                    scroll = Some(id.to_owned());
                }
                reject_partial(&response)?;
                let hits = elastic_hits(&response)?;
                if hits.is_empty() {
                    break;
                }
                let skipped = skip.min(hits.len());
                skip -= skipped;
                total = total
                    .checked_add(hits.len() - skipped)
                    .ok_or("remote row count overflow")?;
                self.check_rows(total)?;
                if skipped < hits.len() {
                    batches.push(decode_rows(
                        &hits[skipped..],
                        projections,
                        columns,
                        parameters,
                        false,
                    )?);
                }
                let id = scroll
                    .as_ref()
                    .ok_or("snapshot search response omitted _scroll_id")?;
                let next = json!({"scroll_id":id,"scroll":self.options.scroll_keep_alive});
                response = self
                    .send_scroll(
                        if self.engine == Engine::Quickwit {
                            Method::GET
                        } else {
                            Method::POST
                        },
                        self.elastic_url(&["_search", "scroll"])?,
                        &next,
                    )
                    .await?;
            }
            if batches.is_empty() {
                batches.push(decode_rows(&[], projections, columns, parameters, false)?);
            }
            Ok(batches)
        }
        .await;
        // Elasticsearch owns server-side scroll contexts. Quickwit uses an
        // expiring cursor and currently has no clear-scroll API.
        if self.engine == Engine::Elasticsearch {
            if let Some(id) = scroll {
                let cleanup = self
                    .send(
                        Method::DELETE,
                        self.elastic_url(&["_search", "scroll"])?,
                        Some(&json!({"scroll_id":[id]})),
                    )
                    .await;
                if result.is_ok() {
                    cleanup?;
                }
            }
        }
        result
    }
}
fn request_bounds(request: &Value) -> Result<(Option<usize>, usize), String> {
    let explicit = request.get("explicit").and_then(Value::as_bool) == Some(true);
    let native = request.get("api").and_then(Value::as_str) == Some("native");
    let limit = if explicit {
        request
            .get("body")
            .and_then(|b| b.get(if native { "max_hits" } else { "size" }))
    } else {
        request.get("limit")
    };
    let offset = if explicit {
        request
            .get("body")
            .and_then(|b| b.get(if native { "start_offset" } else { "from" }))
    } else {
        request.get("offset")
    };
    Ok((
        optional_usize(limit, "limit")?,
        optional_usize(offset, "offset")?.unwrap_or(0),
    ))
}
fn bound_body(request: &Value, parameters: &[Value]) -> Result<Value, String> {
    let mut body = request
        .get("body")
        .cloned()
        .ok_or("remote request requires body")?;
    if !body.is_object() {
        return Err("remote request body must be an object".into());
    }
    if let Some(clauses) = request
        .get("null_guards")
        .or_else(|| request.get("null_filter_paths"))
        .and_then(Value::as_array)
    {
        for clause in clauses {
            let value = clause
                .get("value")
                .and_then(Value::as_str)
                .and_then(|p| request.pointer(p))
                .ok_or("null guard value pointer is invalid")?;
            let path = clause
                .get("clause")
                .and_then(Value::as_str)
                .and_then(|p| p.strip_prefix("/body"))
                .ok_or("null guard clause must reference body")?;
            if value.is_null() {
                *body
                    .pointer_mut(path)
                    .ok_or("null guard clause pointer is invalid")? = json!({"match_none":{}});
            }
        }
    }
    if let Some(clauses) = request.get("boolean_guards").and_then(Value::as_array) {
        for clause in clauses {
            let index = clause
                .get("parameter")
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or("boolean guard requires parameter index")?;
            let value = parameters
                .get(index)
                .ok_or("boolean guard parameter missing")?;
            if !value.is_null() && !value.is_boolean() {
                return Err("boolean guard parameter must be boolean or SQL null".into());
            }
            let truth = clause
                .get("truth")
                .and_then(Value::as_bool)
                .ok_or("boolean guard requires truth boolean")?;
            let path = clause
                .get("clause")
                .and_then(Value::as_str)
                .and_then(|p| p.strip_prefix("/body"))
                .ok_or("boolean guard clause must reference body")?;
            *body
                .pointer_mut(path)
                .ok_or("boolean guard clause pointer is invalid")? =
                if value.as_bool() == Some(truth) {
                    json!({"match_all":{}})
                } else {
                    json!({"match_none":{}})
                };
        }
    }
    Ok(body)
}
fn optional_usize(value: Option<&Value>, name: &str) -> Result<Option<usize>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| format!("remote {name} must be a nonnegative integer or null")),
    }
}
fn reject_partial(value: &Value) -> Result<(), String> {
    if value.get("error").is_some() {
        return Err(format!("remote response error: {}", value["error"]));
    }
    if value.get("timed_out").and_then(Value::as_bool) == Some(true)
        || value.get("terminated_early").and_then(Value::as_bool) == Some(true)
    {
        return Err("remote query timed out or terminated early; partial results rejected".into());
    }
    if value
        .pointer("/_shards/failed")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
        || value
            .get("failed_splits")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
        || value.get("errors").is_some_and(|v| {
            v.as_bool() == Some(true) || v.as_array().is_some_and(|a| !a.is_empty())
        })
    {
        return Err("remote shard or split failure; partial results rejected".into());
    }
    Ok(())
}
fn elastic_hits(value: &Value) -> Result<&[Value], String> {
    value
        .pointer("/hits/hits")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| "remote response requires hits.hits array".into())
}
fn parameter_null_projections(
    projections: &[Value],
    nulls: Option<&Value>,
) -> Result<Vec<Value>, String> {
    let mut projections = projections.to_vec();
    for projection in &mut projections {
        if projection.get("source").and_then(Value::as_str) == Some("parameter") {
            let index = projection
                .get("parameter")
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or("parameter projection requires index")?;
            if let Some(nulls) = nulls {
                let value = nulls
                    .as_array()
                    .and_then(|a| a.get(index))
                    .and_then(Value::as_bool)
                    .ok_or("parameter_nulls must contain a boolean per parameter")?;
                projection["sql_null"] = json!(value);
            }
        }
    }
    Ok(projections)
}
fn validate_projections(projections: &[Value], columns: &[TransferColumn]) -> Result<(), String> {
    if projections.len() != columns.len() {
        return Err("remote projection/schema width mismatch".into());
    }
    for (projection, column) in projections.iter().zip(columns) {
        let _ = column;
        match projection.get("source").and_then(Value::as_str) {
            Some(
                "field" | "parameter" | "literal" | "score" | "id" | "document" | "index"
                | "response",
            ) => {}
            _ => return Err("unknown remote projection source".into()),
        }
    }
    Ok(())
}
fn decode_rows(
    hits: &[Value],
    projections: &[Value],
    columns: &[TransferColumn],
    parameters: &[Value],
    native: bool,
) -> Result<RecordBatch, String> {
    let types = columns
        .iter()
        .map(|column| crate::compiler::data_type(&column.data_type))
        .collect::<Result<Vec<_>, _>>()?;
    let mut values: Vec<Vec<ScalarValue>> = columns
        .iter()
        .map(|_| Vec::with_capacity(hits.len()))
        .collect();
    for (row, hit) in hits.iter().enumerate() {
        if !hit.is_object() {
            return Err(format!("remote hit {row} is not an object"));
        }
        let document = if native {
            Some(hit)
        } else {
            hit.get("_source")
        };
        if !native
            && projections.iter().any(|p| {
                matches!(
                    p.get("source").and_then(Value::as_str),
                    Some("field" | "document")
                )
            })
            && !document.is_some_and(Value::is_object)
        {
            return Err(format!(
                "remote hit {row} omitted its stored _source object"
            ));
        }
        for (index, ((projection, column), ty)) in
            projections.iter().zip(columns).zip(&types).enumerate()
        {
            let value = match projection["source"].as_str().unwrap() {
                "field" => {
                    let path = projection
                        .get("path")
                        .and_then(Value::as_array)
                        .ok_or("field projection requires path array")?;
                    let mut value = document;
                    for segment in path {
                        let segment = segment
                            .as_str()
                            .ok_or("field path segments must be strings")?;
                        value = value.and_then(|v| v.get(segment));
                    }
                    value
                }
                "parameter" => Some(
                    parameters
                        .get(
                            projection
                                .get("parameter")
                                .and_then(Value::as_u64)
                                .ok_or("parameter projection requires index")?
                                as usize,
                        )
                        .ok_or("missing remote parameter value")?,
                ),
                "literal" => Some(
                    projection
                        .get("value")
                        .ok_or("literal projection requires value")?,
                ),
                "score" => {
                    if let Some(index) = projection.get("sort_index").and_then(Value::as_u64) {
                        Some(
                            hit.get("sort")
                                .and_then(Value::as_array)
                                .and_then(|sort| sort.get(index as usize))
                                .ok_or("Quickwit response omitted requested score sort value")?,
                        )
                    } else {
                        hit.get("_score")
                    }
                }
                "id" => hit.get("_id"),
                "index" => hit.get("_index"),
                "document" => document,
                "response" => Some(hit),
                _ => unreachable!(),
            };
            let scalar = decode_value(
                if projection.get("sql_null").and_then(Value::as_bool) == Some(true) {
                    None
                } else {
                    value
                },
                ty,
            )
            .map_err(|e| format!("remote row {row}, column {}: {e}", column.name))?;
            if scalar.is_null() && !column.nullable {
                return Err(format!(
                    "remote row {row}: non-null column {} is missing or null",
                    column.name
                ));
            }
            values[index].push(scalar);
        }
    }
    let arrays = values
        .into_iter()
        .zip(&types)
        .map(|(values, ty)| {
            if values.is_empty() {
                Ok(arrow::array::new_empty_array(ty))
            } else {
                ScalarValue::iter_to_array(values).map_err(|e| e.to_string())
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    let schema = Arc::new(Schema::new(
        columns
            .iter()
            .zip(types)
            .map(|(column, ty)| Field::new(&column.name, ty, column.nullable))
            .collect::<Vec<_>>(),
    ));
    RecordBatch::try_new_with_options(
        schema,
        arrays,
        &RecordBatchOptions::new().with_row_count(Some(hits.len())),
    )
    .map_err(|e| e.to_string())
}
fn decode_value(value: Option<&Value>, ty: &DataType) -> Result<ScalarValue, String> {
    let Some(value) = value else {
        return ScalarValue::try_from(ty).map_err(|e| e.to_string());
    };
    if crate::ir::functions::domain::is_json(ty) {
        return crate::ir::functions::domain::json_scalar(&value.to_string())
            .map_err(|e| e.to_string());
    }
    if value.is_null() {
        return ScalarValue::try_from(ty).map_err(|e| e.to_string());
    }
    if let Some((name, storage)) = crate::ir::functions::domain::descriptor(ty) {
        return crate::ir::functions::domain::scalar(name, decode_value(Some(value), storage)?)
            .map_err(|e| e.to_string());
    }
    match ty {
        DataType::Struct(fields) => {
            let object = value.as_object().ok_or("expected object")?;
            let arrays = fields
                .iter()
                .map(|field| {
                    decode_value(object.get(field.name()), field.data_type())?
                        .to_array_of_size(1)
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<ArrayRef>, String>>()?;
            Ok(ScalarValue::Struct(Arc::new(
                StructArray::try_new(fields.clone(), arrays, None).map_err(|e| e.to_string())?,
            )))
        }
        DataType::List(field) => {
            let values = value
                .as_array()
                .ok_or("expected array")?
                .iter()
                .map(|v| decode_value(Some(v), field.data_type()))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ScalarValue::List(ScalarValue::new_list(
                &values,
                field.data_type(),
                field.is_nullable(),
            )))
        }
        DataType::Utf8 => value
            .as_str()
            .map(|s| ScalarValue::Utf8(Some(s.into())))
            .ok_or_else(|| "expected JSON string".into()),
        DataType::Boolean => value
            .as_bool()
            .map(|b| ScalarValue::Boolean(Some(b)))
            .ok_or_else(|| "expected JSON boolean".into()),
        ty if ty.is_numeric() => {
            if !value.is_number() {
                return Err("expected JSON number".into());
            }
            crate::ir::functions::json::convert(value, ty).map_err(|e| e.to_string())
        }
        DataType::Date32
        | DataType::Date64
        | DataType::Time32(_)
        | DataType::Time64(_)
        | DataType::Timestamp(..)
        | DataType::Binary
        | DataType::LargeBinary => {
            if !value.is_string() {
                return Err(
                    "temporal and binary remote values require explicit string encoding".into(),
                );
            }
            crate::federation::json_scalar(value, ty)
        }
        _ => Err(format!("unsupported remote response type {ty}")),
    }
}

#[async_trait::async_trait(?Send)]
impl Session for HttpSession {
    fn dialect(&self) -> &str {
        self.engine.name()
    }
    async fn execute_request(
        &mut self,
        request: &Value,
        columns: &[TransferColumn],
    ) -> Result<Vec<RecordBatch>, String> {
        self.execute(request, columns).await
    }
    async fn execute_requests(
        &mut self,
        requests: &[Value],
        columns: &[TransferColumn],
    ) -> Result<Vec<Vec<RecordBatch>>, String> {
        self.execute_many(requests, columns).await
    }
}
macro_rules! session {
    ($name:ident,$engine:ident,$feature:literal) => {
        #[cfg(feature=$feature)]
        pub struct $name(HttpSession);
        #[cfg(feature=$feature)]
        impl $name {
            pub fn new(endpoint: impl Into<String>) -> Result<Self, String> {
                Self::with_options(HttpOptions::new(endpoint))
            }
            pub fn with_options(options: HttpOptions) -> Result<Self, String> {
                Ok(Self(HttpSession::new(Engine::$engine, options, None)?))
            }
            pub async fn request_json(
                &self,
                method: Method,
                path: &[&str],
                body: Option<&Value>,
            ) -> Result<Value, String> {
                self.0.request_json(method, path, body).await
            }
            pub fn clear_metadata_cache(&mut self) {
                self.0.clear_metadata_cache();
            }
            /// A caller-configured client supports private CAs, mutual TLS and
            /// proxies. The caller must disable redirects on that client.
            pub fn with_client(options: HttpOptions, client: Client) -> Result<Self, String> {
                Ok(Self(HttpSession::new(
                    Engine::$engine,
                    options,
                    Some(client),
                )?))
            }
        }
        #[cfg(feature=$feature)]
        #[async_trait::async_trait(?Send)]
        impl Session for $name {
            fn dialect(&self) -> &str {
                self.0.engine.name()
            }
            async fn execute_request(
                &mut self,
                request: &Value,
                columns: &[TransferColumn],
            ) -> Result<Vec<RecordBatch>, String> {
                self.0.execute(request, columns).await
            }
            async fn execute_requests(
                &mut self,
                requests: &[Value],
                columns: &[TransferColumn],
            ) -> Result<Vec<Vec<RecordBatch>>, String> {
                self.0.execute_many(requests, columns).await
            }
        }
    };
}
session!(QuickwitSession, Quickwit, "quickwit");
session!(ElasticsearchSession, Elasticsearch, "elasticsearch");

mod batch;
mod metadata;
#[cfg(test)]
mod tests;
