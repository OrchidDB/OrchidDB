use super::*;
fn require_bm25(mapping: &Value, settings: &Value) -> Result<(), String> {
    let name = mapping
        .get("similarity")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let configured = settings
        .pointer("/settings/index/similarity")
        .and_then(|s| s.get(name))
        .and_then(|s| s.get("type"))
        .and_then(Value::as_str);
    let kind = configured.unwrap_or(if matches!(name, "default" | "BM25") {
        "BM25"
    } else {
        name
    });
    if kind != "BM25" {
        return Err(format!(
            "text.bm25 requires BM25 similarity; Elasticsearch selected {kind}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bm25_checks_field_and_index_default_similarity() {
        let settings = json!({"settings":{"index":{"similarity":{"default":{"type":"boolean"},"tuned":{"type":"BM25","k1":"1.5"}}}}});
        assert!(require_bm25(&json!({}), &settings).is_err());
        assert!(require_bm25(&json!({"similarity":"tuned"}), &settings).is_ok());
        assert!(require_bm25(&json!({"similarity":"BM25"}), &settings).is_ok());
        assert!(require_bm25(&json!({"similarity":"boolean"}), &json!({})).is_err());
        assert!(require_bm25(&json!({}), &json!({"settings":{"index":{}}})).is_ok());
    }
}
impl HttpSession {
    pub(super) async fn validate_requirements(
        &mut self,
        index: &str,
        requirements: &[Value],
    ) -> Result<(), String> {
        if requirements.is_empty() {
            return Ok(());
        }
        if !self.metadata.contains_key(index) {
            let url = if self.engine == Engine::Quickwit {
                self.url(&["api", "v1", "indexes", index])?
            } else {
                self.elastic_url(&[index, "_mapping"])?
            };
            let metadata = self.send(Method::GET, url, None).await?;
            self.metadata.insert(index.into(), metadata);
        }
        if self.engine == Engine::Elasticsearch
            && requirements
                .iter()
                .any(|r| r.get("usage").and_then(Value::as_str) == Some("match"))
            && !self.settings.contains_key(index)
        {
            let settings = self
                .send(Method::GET, self.elastic_url(&[index, "_settings"])?, None)
                .await?;
            self.settings.insert(index.into(), settings);
        }
        let metadata = &self.metadata[index];
        for requirement in requirements {
            let field = requirement
                .get("field")
                .and_then(Value::as_str)
                .ok_or("mapping requirement needs field")?;
            let usage = requirement
                .get("usage")
                .and_then(Value::as_str)
                .ok_or("mapping requirement needs usage")?;
            let ty = requirement
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("");
            if self.engine == Engine::Quickwit {
                let mappings = metadata
                    .pointer("/index_config/doc_mapping/field_mappings")
                    .and_then(Value::as_array)
                    .ok_or("Quickwit metadata omitted field_mappings")?;
                let mapping = quickwit_field(mappings, field)
                    .ok_or_else(|| format!("Quickwit field {field} has no explicit mapping"))?;
                let physical = mapping
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or("Quickwit mapping omitted type")?;
                if physical.starts_with("array<") && usage != "stored" {
                    return Err(format!(
                        "Quickwit field {field} is multivalued; scalar predicate semantics require a scalar mapping"
                    ));
                }
                let fast = mapping
                    .get("fast")
                    .is_some_and(|fast| fast.as_bool() == Some(true) || fast.is_object());
                match usage {
                    "stored" => {
                        if mapping.get("stored").and_then(Value::as_bool) == Some(false) {
                            return Err(format!(
                                "Quickwit field {field} must be stored to return it"
                            ));
                        }
                    }
                    "match" => {
                        if physical != "text"
                            || mapping
                                .get("record")
                                .and_then(Value::as_str)
                                .is_none_or(|r| !matches!(r, "freq" | "position"))
                            || mapping.get("fieldnorms").and_then(Value::as_bool) != Some(true)
                        {
                            return Err(format!(
                                "Quickwit BM25 field {field} requires text frequencies and fieldnorms"
                            ));
                        }
                    }
                    "exact" => {
                        if (matches!(ty, "string" | "text")
                            || ty.is_empty()
                                && matches!(physical, "text" | "keyword" | "match_only_text"))
                            && (physical != "text"
                                || mapping.get("tokenizer").and_then(Value::as_str) != Some("raw"))
                        {
                            return Err(format!(
                                "Quickwit exact string field {field} requires raw tokenizer"
                            ));
                        }
                    }
                    "range" => {
                        if !matches!(physical, "i64" | "u64" | "f64" | "datetime") {
                            return Err(format!(
                                "Quickwit range field {field} requires numeric or datetime mapping"
                            ));
                        }
                    }
                    "exists" => {
                        if !fast
                            && metadata
                                .pointer("/index_config/doc_mapping/index_field_presence")
                                .and_then(Value::as_bool)
                                != Some(true)
                        {
                            return Err(format!(
                                "Quickwit exists field {field} requires a fast field"
                            ));
                        }
                    }
                    "sort" => {
                        if !fast {
                            return Err(format!("Quickwit sort field {field} must be fast"));
                        }
                    }
                    _ => return Err(format!("unknown remote mapping usage {usage}")),
                }
                let compatible = match ty {
                    "string" | "text" => physical == "text",
                    "boolean" => physical == "bool",
                    ty if ty.starts_with("int") => physical == "i64",
                    ty if ty.starts_with("uint") => physical == "u64",
                    ty if ty.starts_with("float") => physical == "f64",
                    "timestamp" => physical == "datetime",
                    "" => true,
                    _ => false,
                };
                if !compatible {
                    return Err(format!(
                        "Quickwit field {field} type {physical} cannot implement declared {ty}"
                    ));
                }
            } else {
                let indexes = metadata
                    .as_object()
                    .ok_or("Elasticsearch mapping response is not an object")?;
                if indexes.is_empty() {
                    return Err("Elasticsearch mapping response has no indices".into());
                }
                for (name, index_mapping) in indexes {
                    let mapping = elastic_field(
                        index_mapping
                            .pointer("/mappings/properties")
                            .ok_or("Elasticsearch metadata omitted properties")?,
                        field,
                    )
                    .ok_or_else(|| {
                        format!("Elasticsearch index {name} field {field} is unmapped")
                    })?;
                    let physical = mapping
                        .get("type")
                        .and_then(Value::as_str)
                        .or_else(|| mapping.get("properties").is_some().then_some("object"))
                        .ok_or("Elasticsearch field mapping omitted type")?;
                    if mapping.get("null_value").is_some() {
                        return Err(format!(
                            "Elasticsearch field {field} substitutes null_value and cannot preserve SQL null predicates"
                        ));
                    }
                    match usage {
                        "stored" => {
                            let source = index_mapping.pointer("/mappings/_source");
                            if source
                                .and_then(|s| s.get("enabled"))
                                .and_then(Value::as_bool)
                                == Some(false)
                                || source.and_then(|s| s.get("includes")).is_some()
                                || source.and_then(|s| s.get("excludes")).is_some()
                            {
                                return Err(format!(
                                    "Elasticsearch index {name} must preserve unfiltered _source for projected field {field}"
                                ));
                            }
                        }
                        "match" => {
                            if physical != "text"
                                || mapping.get("norms").and_then(Value::as_bool) == Some(false)
                                || mapping.get("index_options").and_then(Value::as_str)
                                    == Some("docs")
                            {
                                return Err(format!(
                                    "Elasticsearch BM25 field {field} requires text mapping"
                                ));
                            }
                            let settings = self
                                .settings
                                .get(index)
                                .and_then(|s| s.get(name))
                                .ok_or("Elasticsearch omitted index similarity settings")?;
                            require_bm25(mapping, settings)?;
                        }
                        "exact" => {
                            if (matches!(ty, "string" | "text")
                                || ty.is_empty()
                                    && matches!(physical, "text" | "keyword" | "match_only_text"))
                                && (physical != "keyword"
                                    || mapping.get("normalizer").is_some()
                                    || mapping.get("ignore_above").is_some())
                            {
                                return Err(format!(
                                    "Elasticsearch exact field {field} requires keyword without normalizer or ignore_above"
                                ));
                            }
                        }
                        "range" => {
                            if !matches!(
                                physical,
                                "byte"
                                    | "short"
                                    | "integer"
                                    | "long"
                                    | "unsigned_long"
                                    | "float"
                                    | "double"
                                    | "half_float"
                                    | "date"
                                    | "date_nanos"
                                    | "keyword"
                            ) {
                                return Err(format!(
                                    "Elasticsearch range field {field} is not scalar/range-indexed"
                                ));
                            }
                        }
                        "exists" => {
                            if mapping.get("index").and_then(Value::as_bool) == Some(false)
                                && mapping.get("doc_values").and_then(Value::as_bool) == Some(false)
                            {
                                return Err(format!(
                                    "Elasticsearch exists field {field} has neither index nor doc_values"
                                ));
                            }
                        }
                        "sort" => {
                            if matches!(physical, "text" | "match_only_text")
                                || mapping.get("doc_values").and_then(Value::as_bool) == Some(false)
                            {
                                return Err(format!(
                                    "Elasticsearch sort field {field} requires sortable doc_values"
                                ));
                            }
                        }
                        _ => return Err(format!("unknown remote mapping usage {usage}")),
                    }
                    let compatible = match ty {
                        "string" | "text" => {
                            matches!(physical, "keyword" | "text" | "match_only_text")
                        }
                        "boolean" => physical == "boolean",
                        ty if ty.starts_with("int") => {
                            matches!(physical, "byte" | "short" | "integer" | "long")
                        }
                        ty if ty.starts_with("uint") => matches!(
                            physical,
                            "byte" | "short" | "integer" | "long" | "unsigned_long"
                        ),
                        ty if ty.starts_with("float") => {
                            matches!(physical, "float" | "double" | "half_float")
                        }
                        "timestamp" => matches!(physical, "date" | "date_nanos"),
                        "" => true,
                        _ => false,
                    };
                    if !compatible {
                        return Err(format!(
                            "Elasticsearch field {field} type {physical} cannot implement declared {ty}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
fn quickwit_field<'a>(fields: &'a [Value], path: &str) -> Option<&'a Value> {
    if let Some(field) = fields
        .iter()
        .find(|field| field.get("name").and_then(Value::as_str) == Some(path))
    {
        return Some(field);
    }
    let (head, tail) = path.split_once('.')?;
    let field = fields
        .iter()
        .find(|field| field.get("name").and_then(Value::as_str) == Some(head))?;
    quickwit_field(field.get("field_mappings")?.as_array()?, tail)
}
fn elastic_field<'a>(properties: &'a Value, path: &str) -> Option<&'a Value> {
    if let Some(field) = properties.get(path) {
        return Some(field);
    }
    let (head, tail) = path.split_once('.')?;
    let field = properties.get(head)?;
    elastic_field(
        field.get("properties").or_else(|| field.get("fields"))?,
        tail,
    )
}
