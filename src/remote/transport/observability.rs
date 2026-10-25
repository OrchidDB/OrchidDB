//! Bounded Prometheus/Loki requests through the shared HTTP session. Return
//! typed Arrow batches, never JSON wrappers or silently truncated observations.
use super::*;
impl HttpSession {
    pub(super) async fn execute_observability(
        &self,
        request: &Value,
        columns: &[TransferColumn],
    ) -> Result<Vec<RecordBatch>, String> {
        if request["version"] != 1
            || request["engine"] != self.engine.name()
            || request["operation"] != "range"
        {
            return Err("invalid observability request".into());
        }
        let query = request["query"]
            .as_str()
            .filter(|q| !q.is_empty())
            .ok_or("missing source query")?;
        let start = request["start_ns"].as_i64().ok_or("missing start_ns")?;
        let end = request["end_ns"].as_i64().ok_or("missing end_ns")?;
        let logs = self.engine == SourceKind::Loki;
        let span = end.checked_sub(start).ok_or("invalid time bounds")?;
        if start < 0
            || span <= 0
            || span
                > if logs {
                    900_000_000_000
                } else {
                    86_400_000_000_000
                }
        {
            return Err("invalid observability time window".into());
        }
        let step = request["step_seconds"].as_u64().ok_or("missing step")?;
        if !(15..=3600).contains(&step)
            || (!logs && (span as u64).div_ceil(step * 1_000_000_000) > 1440)
        {
            return Err("invalid metric step or sample count".into());
        }
        let expected = request["columns"]
            .as_array()
            .ok_or("missing output columns")?;
        if expected.len() != columns.len() || expected.iter().any(|a| a.as_str().is_none()) {
            return Err("observability output schema mismatch".into());
        }
        let mut url = self.url(if logs {
            &["loki", "api", "v1", "query_range"]
        } else {
            &["api", "v1", "query_range"]
        })?;
        let limit = self
            .options
            .max_rows
            .unwrap_or(if logs { 4999 } else { 50000 });
        if limit == 0 {
            return Err("observability row limit must be positive".into());
        }
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("query", query);
            if logs {
                pairs
                    .append_pair("start", &start.to_string())
                    .append_pair("end", &end.to_string())
                    .append_pair("direction", "forward")
                    .append_pair("limit", &(limit.saturating_add(1).min(5000)).to_string());
            } else {
                pairs
                    .append_pair("start", &seconds(evaluation_start(start, step)))
                    .append_pair("end", &seconds(end - 1))
                    .append_pair("step", &step.to_string())
                    .append_pair("limit", "201")
                    .append_pair("timeout", "5s");
            }
        }
        if url.as_str().len() > self.options.max_request_bytes {
            return Err("observability request exceeds byte limit".into());
        }
        // The relation has a fixed epoch-aligned evaluation grid. Predicate
        // pushdown may narrow it, but must never shift or invent samples.
        let response = if !logs && evaluation_start(start, step) >= end {
            json!({"status":"success","data":{"resultType":"matrix","result":[]}})
        } else {
            self.send(Method::GET, url, None).await?
        };
        if response["status"] != "success"
            || ["warnings", "infos"].iter().any(|key| {
                response
                    .get(*key)
                    .is_some_and(|w| !w.as_array().is_some_and(Vec::is_empty))
            })
        {
            return Err("observability source failed or returned annotations".into());
        }
        let data = &response["data"];
        if data["resultType"] != if logs { "streams" } else { "matrix" } {
            return Err("unexpected observability result type".into());
        }
        let series = data["result"].as_array().ok_or("missing result series")?;
        if !logs && series.len() > 200 {
            return Err("Prometheus response exceeds 200 series".into());
        }
        let mut rows = Vec::new();
        for series in series {
            let labels = series[if logs { "stream" } else { "metric" }]
                .as_object()
                .ok_or("missing labels")?;
            if series
                .get("histograms")
                .is_some_and(|h| !h.as_array().is_some_and(Vec::is_empty))
            {
                return Err("native histogram samples are not supported".into());
            }
            for sample in series["values"].as_array().ok_or("missing samples")? {
                if rows.len() >= limit || (logs && rows.len() >= 4999) {
                    return Err("observability response reached row cap; narrow query".into());
                }
                let ns = if logs {
                    sample[0]
                        .as_str()
                        .ok_or("invalid log timestamp")?
                        .parse::<i64>()
                        .map_err(|_| "invalid log timestamp")?
                } else {
                    let n = sample[0]
                        .as_f64()
                        .filter(|v| v.is_finite() && *v >= 0.0 && *v < 9_223_372_036.0)
                        .ok_or("invalid metric timestamp")?;
                    (n * 1e9).round() as i64
                };
                let stamp = chrono::DateTime::from_timestamp(
                    ns.div_euclid(1_000_000_000),
                    ns.rem_euclid(1_000_000_000) as u32,
                )
                .ok_or("invalid timestamp")?
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
                let mut row = labels.clone();
                // Lossless source key: retain Loki nanoseconds and the complete label set.
                row.insert("sample_id".into(), json!(json!([ns.to_string(), labels, if logs { sample.get(1) } else { None }]).to_string()));
                row.insert("timestamp".into(), json!(stamp));
                row.insert("labels".into(), json!(labels));
                if logs {
                    row.insert(
                        "line".into(),
                        json!(sample[1].as_str().ok_or("invalid log line")?),
                    );
                    row.insert(
                        "structured_metadata".into(),
                        sample.get(2).cloned().unwrap_or(json!({})),
                    );
                } else {
                    let n = sample[1]
                        .as_str()
                        .ok_or("invalid metric value")?
                        .parse::<f64>()
                        .map_err(|_| "invalid metric number")?;
                    // Preserve non-finite values in Arrow; JSON cannot encode them.
                    row.insert("value".into(), json!(n.to_string()));
                }
                rows.push(row);
            }
        }
        let types = columns
            .iter()
            .map(|c| crate::compiler::data_type(&c.data_type))
            .collect::<Result<Vec<_>, _>>()?;
        let arrays = columns
            .iter()
            .zip(&types)
            .zip(expected)
            .map(|((c, ty), source)| {
                let name = source.as_str().ok_or("invalid source column")?;
                let values = rows
                    .iter()
                    .map(|row| {
                        let value = if !logs && name == "value" {
                            if *ty != DataType::Float64 {
                                return Err("Prometheus value must be float64".into());
                            }
                            ScalarValue::Float64(Some(
                                row["value"]
                                    .as_str()
                                    .ok_or("invalid metric value")?
                                    .parse()
                                    .map_err(|_| "invalid metric value")?,
                            ))
                        } else {
                            decode_value(row.get(name), ty)?
                        };
                        if value.is_null() && !c.nullable {
                            return Err(format!(
                                "missing required observability column {}",
                                c.name
                            ));
                        }
                        Ok(value)
                    })
                    .collect::<Result<Vec<_>, String>>()?;
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
                .map(|(c, t)| Field::new(&c.name, t, c.nullable))
                .collect::<Vec<_>>(),
        ));
        Ok(vec![
            RecordBatch::try_new_with_options(
                schema,
                arrays,
                &RecordBatchOptions::new().with_row_count(Some(rows.len())),
            )
            .map_err(|e| e.to_string())?,
        ])
    }
}
fn seconds(ns: i64) -> String {
    format!("{}.{:09}", ns / 1_000_000_000, ns % 1_000_000_000)
}

fn evaluation_start(start: i64, step: u64) -> i64 {
    let step = step as i64 * 1_000_000_000;
    start
        .checked_add(step - 1)
        .map(|n| n / step * step)
        .unwrap_or(i64::MAX)
}
