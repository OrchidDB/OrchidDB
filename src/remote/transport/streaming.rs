//! Demand-driven source requests. Only one bounded response batch/page is held
//! at a time; unbounded snapshot searches advance their cursor incrementally.
use super::*;
use crate::execution::stream::BatchStream;
use futures::StreamExt;
use std::collections::VecDeque;

pub(super) fn request_stream(
    session: HttpSession,
    requests: crate::federation::RequestStream,
    columns: Vec<TransferColumn>,
) -> Result<BatchStream, String> {
    let schema = crate::federation::transfer_schema(&columns)?;
    struct State {
        session: HttpSession,
        requests: crate::federation::RequestStream,
        columns: Vec<TransferColumn>,
        pending: VecDeque<Value>,
        batches: VecDeque<RecordBatch>,
        active: Option<BatchStream>,
    }
    let state = State {
        session,
        requests,
        columns,
        pending: VecDeque::new(),
        batches: VecDeque::new(),
        active: None,
    };
    Ok(BatchStream::new(
        schema,
        futures::stream::try_unfold(state, |mut state| async move {
            loop {
                if let Some(batch) = state.batches.pop_front() {
                    return Ok(Some((batch, state)));
                }
                if let Some(active) = state.active.as_mut() {
                    if let Some(batch) = active.next().await {
                        return Ok(Some((batch?, state)));
                    }
                    state.active = None;
                }
                if state.pending.is_empty() {
                    match state.requests.next().await {
                        Some(requests) => state.pending.extend(requests?),
                        None => return Ok(None),
                    }
                    continue;
                }
                let request = state.pending.front().unwrap();
                if unbounded(&state.session, request)? {
                    let request = state.pending.pop_front().unwrap();
                    state.active = Some(scroll_stream(
                        state.session.clone(),
                        request,
                        state.columns.clone(),
                    )?);
                    continue;
                }
                let mut group = Vec::new();
                while group.len() < state.session.options.batch_size {
                    let Some(next) = state.pending.front() else {
                        break;
                    };
                    if unbounded(&state.session, next)? {
                        break;
                    }
                    group.push(state.pending.pop_front().unwrap());
                }
                let results = state.session.execute_many(&group, &state.columns).await?;
                if results.len() != group.len() {
                    return Err("source response count does not match bound request batch".into());
                }
                state.batches.extend(results.into_iter().flatten());
            }
        }),
    ))
}
fn unbounded(session: &HttpSession, request: &Value) -> Result<bool, String> {
    Ok(matches!(
        session.engine,
        SourceKind::Quickwit | SourceKind::Elasticsearch
    ) && request["operation"] == "search"
        && request
            .get("api")
            .and_then(Value::as_str)
            .unwrap_or("elastic")
            == "elastic"
        && request_bounds(request)?.0.is_none())
}
struct Scroll {
    session: HttpSession,
    body: Value,
    index: String,
    requirements: Vec<Value>,
    projections: Vec<Value>,
    columns: Vec<TransferColumn>,
    parameters: Vec<Value>,
    cursor: Option<String>,
    started: bool,
    skip: usize,
    total: usize,
    empty: bool,
}
impl Scroll {
    async fn cleanup(&mut self) -> Result<(), String> {
        if self.session.engine == SourceKind::Elasticsearch {
            if let Some(id) = self.cursor.take() {
                self.session
                    .send(
                        Method::DELETE,
                        self.session.elastic_url(&["_search", "scroll"])?,
                        Some(&json!({"scroll_id":[id]})),
                    )
                    .await?;
            }
        }
        Ok(())
    }
    async fn next(&mut self) -> Result<Option<RecordBatch>, String> {
        if self.empty {
            return Ok(None);
        }
        loop {
            let response = if !self.started {
                self.started = true;
                if !self.requirements.is_empty() {
                    self.session
                        .validate_requirements(&self.index, &self.requirements)
                        .await?;
                }
                let mut url = self.session.elastic_url(&[&self.index, "_search"])?;
                url.query_pairs_mut()
                    .append_pair(
                        "allow_partial_search_results",
                        if self.session.engine == SourceKind::Quickwit {
                            "true"
                        } else {
                            "false"
                        },
                    )
                    .append_pair("scroll", &self.session.options.scroll_keep_alive);
                self.session
                    .send_scroll(Method::POST, url, &self.body)
                    .await?
            } else {
                let id = self
                    .cursor
                    .as_ref()
                    .ok_or("snapshot search response omitted _scroll_id")?;
                self.session
                    .send_scroll(
                        if self.session.engine == SourceKind::Quickwit {
                            Method::GET
                        } else {
                            Method::POST
                        },
                        self.session.elastic_url(&["_search", "scroll"])?,
                        &json!({"scroll_id":id,"scroll":self.session.options.scroll_keep_alive}),
                    )
                    .await?
            };
            if let Some(cursor) = response.get("_scroll_id").and_then(Value::as_str) {
                self.cursor = Some(cursor.into());
            }
            reject_partial(&response)?;
            let hits = elastic_hits(&response)?;
            if hits.is_empty() {
                self.cleanup().await?;
                return Ok(None);
            }
            let skipped = self.skip.min(hits.len());
            self.skip -= skipped;
            self.total = self
                .total
                .checked_add(hits.len() - skipped)
                .ok_or("source row count overflow")?;
            self.session.check_rows(self.total)?;
            if self.cursor.is_none() {
                return Err("snapshot search response omitted _scroll_id".into());
            }
            if skipped < hits.len() {
                return decode_rows(
                    &hits[skipped..],
                    &self.projections,
                    &self.columns,
                    &self.parameters,
                    false,
                )
                .map(Some);
            }
        }
    }
}
impl Drop for Scroll {
    fn drop(&mut self) {
        // On cancellation/error, clear remote cursor without holding up drop.
        // Quickwit cursors expire by their configured keep-alive.
        if self.session.engine != SourceKind::Elasticsearch {
            return;
        }
        let Some(id) = self.cursor.take() else {
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let session = self.session.clone();
            runtime.spawn(async move {
                if let Ok(url) = session.elastic_url(&["_search", "scroll"]) {
                    let _ = session
                        .send(Method::DELETE, url, Some(&json!({"scroll_id":[id]})))
                        .await;
                }
            });
        }
    }
}
fn scroll_stream(
    session: HttpSession,
    request: Value,
    columns: Vec<TransferColumn>,
) -> Result<BatchStream, String> {
    if request["version"] != 1 || request["engine"] != session.engine.name() {
        return Err("invalid snapshot source request".into());
    }
    let index = request["index"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("snapshot request requires index")?
        .to_owned();
    let projections = request["columns"]
        .as_array()
        .ok_or("snapshot request requires columns")?;
    let projections = parameter_null_projections(projections, request.get("parameter_nulls"))?;
    validate_projections(&projections, &columns)?;
    let parameters = request
        .get("parameters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut body = bound_body(&request, &parameters)?;
    let projections = session.score_projections(&projections, &body)?;
    if body.get("knn").is_some() || body.get("search_after").is_some() || body.get("pit").is_some()
    {
        return Err(
            "snapshot scans cannot combine knn, search_after, or caller-owned PIT pagination"
                .into(),
        );
    }
    body.as_object_mut()
        .ok_or("snapshot body must be object")?
        .remove("from");
    body["size"] = json!(session.options.page_size);
    let (_, skip) = request_bounds(&request)?;
    let empty = request
        .get("null_query_paths")
        .and_then(Value::as_array)
        .is_some_and(|paths| {
            paths.iter().any(|p| {
                p.as_str()
                    .and_then(|p| request.pointer(p))
                    .is_some_and(Value::is_null)
            })
        });
    let requirements = request
        .get("requirements")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let schema = crate::federation::transfer_schema(&columns)?;
    let state = Scroll {
        session,
        body,
        index,
        requirements,
        projections,
        columns,
        parameters,
        cursor: None,
        started: false,
        skip,
        total: 0,
        empty,
    };
    Ok(BatchStream::new(
        schema,
        futures::stream::try_unfold(state, |mut state| async move {
            Ok(state.next().await?.map(|batch| (batch, state)))
        }),
    ))
}
