use super::*;
impl HttpSession {
    pub(super) async fn execute_many(
        &mut self,
        requests: &[Value],
        columns: &[TransferColumn],
    ) -> Result<Vec<Vec<RecordBatch>>, String> {
        if requests.is_empty() {
            return Ok(vec![]);
        }
        let bounded_elastic = requests.iter().all(|request| {
            request
                .get("api")
                .and_then(Value::as_str)
                .unwrap_or("elastic")
                == "elastic"
                && (request.get("operation").and_then(Value::as_str) == Some("query")
                    || request.get("limit").and_then(Value::as_u64).is_some())
        });
        if !bounded_elastic || requests.len() == 1 {
            let mut results = Vec::with_capacity(requests.len());
            for request in requests {
                results.push(self.execute(request, columns).await?);
            }
            return Ok(results);
        }
        let mut output = Vec::with_capacity(requests.len());
        for chunk in requests.chunks(self.options.batch_size) {
            let mut payload = String::new();
            let mut jobs = Vec::new();
            let mut results: Vec<Option<Vec<RecordBatch>>> = vec![None; chunk.len()];
            for (position, request) in chunk.iter().enumerate() {
                if request.get("version").and_then(Value::as_u64) != Some(1)
                    || request.get("engine").and_then(Value::as_str) != Some(self.engine.name())
                {
                    return Err("invalid batched request version or engine".into());
                }
                let operation = request
                    .get("operation")
                    .and_then(Value::as_str)
                    .ok_or("request operation missing")?;
                if !matches!(operation, "search" | "query") {
                    return Err("unsupported batched operation".into());
                }
                let index = request
                    .get("index")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or("batched request requires index")?;
                let projections = request
                    .get("columns")
                    .and_then(Value::as_array)
                    .ok_or("request projections missing")?;
                let projections =
                    parameter_null_projections(projections, request.get("parameter_nulls"))?;
                validate_projections(&projections, columns)?;
                let parameters = request
                    .get("parameters")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let (limit, offset) = request_bounds(request)?;
                let empty = operation == "search" && limit == Some(0)
                    || request
                        .get("null_query_paths")
                        .and_then(Value::as_array)
                        .is_some_and(|paths| {
                            paths.iter().any(|p| {
                                p.as_str()
                                    .and_then(|p| request.pointer(p))
                                    .is_some_and(Value::is_null)
                            })
                        });
                if empty {
                    results[position] = Some(vec![decode_rows(
                        &[],
                        &projections,
                        columns,
                        &parameters,
                        false,
                    )?]);
                    continue;
                }
                if let Some(requirements) = request.get("requirements").and_then(Value::as_array) {
                    self.validate_requirements(index, requirements).await?;
                }
                let mut body = bound_body(request, &parameters)?;
                let projections = self.score_projections(&projections, &body)?;
                if operation == "search" {
                    let limit = limit.ok_or("bounded batch search requires limit")?;
                    self.check_rows(limit)?;
                    body["size"] = json!(limit);
                    body["from"] = json!(offset);
                }
                let header = if self.engine == Engine::Quickwit {
                    json!({"index":index})
                } else {
                    json!({"index":index,"allow_partial_search_results":false})
                };
                payload.push_str(&header.to_string());
                payload.push('\n');
                payload.push_str(&body.to_string());
                payload.push('\n');
                jobs.push((position, operation, limit, projections, parameters));
            }
            if !jobs.is_empty() {
                let response = self
                    .send_payload(
                        Method::POST,
                        self.elastic_url(&["_msearch"])?,
                        Some(("application/x-ndjson", payload)),
                    )
                    .await?;
                let responses = response
                    .get("responses")
                    .and_then(Value::as_array)
                    .ok_or("_msearch response omitted responses array")?;
                if responses.len() != jobs.len() {
                    return Err("_msearch response count does not match request count".into());
                }
                for (response, (position, operation, limit, projections, parameters)) in
                    responses.iter().zip(jobs)
                {
                    reject_partial(response)?;
                    if response
                        .get("status")
                        .and_then(Value::as_u64)
                        .is_some_and(|status| status >= 400)
                    {
                        return Err(format!("_msearch item failed: {response}"));
                    }
                    let batch = if operation == "query" {
                        decode_rows(
                            std::slice::from_ref(response),
                            &projections,
                            columns,
                            &parameters,
                            true,
                        )?
                    } else {
                        let hits = elastic_hits(response)?;
                        if hits.len() > limit.unwrap() {
                            return Err("_msearch response exceeded requested limit".into());
                        }
                        decode_rows(hits, &projections, columns, &parameters, false)?
                    };
                    results[position] = Some(vec![batch]);
                }
            }
            output.extend(
                results
                    .into_iter()
                    .map(|r| r.expect("each request assigned one result")),
            );
        }
        Ok(output)
    }
}
