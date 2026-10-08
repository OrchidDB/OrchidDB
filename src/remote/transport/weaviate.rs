use super::*;

fn identifier(value: &str) -> Result<&str, String> {
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err("invalid Weaviate identifier".into());
    }
    Ok(value)
}
fn graphql(value: &Value) -> Result<String, String> {
    Ok(match value {
        Value::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(key, value)| {
                    let key = identifier(key)?;
                    let value = if key == "operator" {
                        let name = value.as_str().ok_or("invalid Weaviate filter operator")?;
                        if !matches!(
                            name,
                            "And"
                                | "Or"
                                | "Equal"
                                | "NotEqual"
                                | "LessThan"
                                | "LessThanEqual"
                                | "GreaterThan"
                                | "GreaterThanEqual"
                                | "IsNull"
                        ) {
                            return Err("unsupported Weaviate filter operator".into());
                        }
                        name.to_owned()
                    } else {
                        graphql(value)?
                    };
                    Ok(format!("{key}:{value}"))
                })
                .collect::<Result<Vec<_>, String>>()?
                .join(",")
        ),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(graphql)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        other => other.to_string(),
    })
}
#[derive(Clone)]
enum Filter {
    All,
    None,
    Predicate(Value),
}
fn combine(values: Vec<Filter>, and: bool) -> Filter {
    let mut predicates = Vec::new();
    for value in values {
        match value {
            Filter::All if !and => return Filter::All,
            Filter::None if and => return Filter::None,
            Filter::Predicate(value) => predicates.push(value),
            _ => {}
        }
    }
    match predicates.len() {
        0 => {
            if and {
                Filter::All
            } else {
                Filter::None
            }
        }
        1 => Filter::Predicate(predicates.remove(0)),
        _ => {
            Filter::Predicate(json!({"operator":if and {"And"} else {"Or"},"operands":predicates}))
        }
    }
}
fn property<'a>(schema: &'a Value, name: &str) -> Result<&'a Value, String> {
    identifier(name)?;
    schema["properties"]
        .as_array()
        .and_then(|properties| properties.iter().find(|p| p["name"] == name))
        .ok_or("Weaviate collection is missing a required property".into())
}
fn comparison(schema: &Value, name: &str, op: &str, value: &Value) -> Result<Filter, String> {
    let property = property(schema, name)?;
    if property["indexFilterable"] == false {
        return Err("Weaviate property is not filterable".into());
    }
    if value.is_null() {
        return Ok(Filter::None);
    }
    let ty = property["dataType"][0]
        .as_str()
        .ok_or("Weaviate property type is missing")?;
    let field = match ty {
        "int" if value.as_i64().is_some() => "valueInt",
        "number" if value.as_f64().is_some_and(f64::is_finite) => "valueNumber",
        "boolean" if value.is_boolean() => "valueBoolean",
        "text" if value.is_string() && property["tokenization"] == "field" => "valueText",
        "uuid" if value.is_string() => "valueText",
        "date" if value.is_string() => "valueDate",
        _ => return Err("Weaviate filter cannot preserve the property type or text equality; text filters require field tokenization".into()),
    };
    Ok(Filter::Predicate(
        json!({"path":[name],"operator":op,field:value}),
    ))
}
fn filter(schema: &Value, value: &Value, negate: bool) -> Result<Filter, String> {
    if value.get("match_all").is_some() {
        return Ok(if negate { Filter::None } else { Filter::All });
    }
    if value.get("match_none").is_some() {
        return Ok(if negate { Filter::All } else { Filter::None });
    }
    if let Some(terms) = value.get("term").and_then(Value::as_object) {
        return Ok(combine(
            terms
                .iter()
                .map(|(name, value)| {
                    comparison(
                        schema,
                        name,
                        if negate { "NotEqual" } else { "Equal" },
                        value,
                    )
                })
                .collect::<Result<_, _>>()?,
            !negate,
        ));
    }
    if let Some(ranges) = value.get("range").and_then(Value::as_object) {
        let mut result = Vec::new();
        for (name, range) in ranges {
            for (op, value) in range.as_object().ok_or("invalid range filter")? {
                let op = match (op.as_str(), negate) {
                    ("lt", false) | ("gte", true) => "LessThan",
                    ("lte", false) | ("gt", true) => "LessThanEqual",
                    ("gt", false) | ("lte", true) => "GreaterThan",
                    ("gte", false) | ("lt", true) => "GreaterThanEqual",
                    _ => return Err("unsupported range filter".into()),
                };
                result.push(comparison(schema, name, op, value)?);
            }
        }
        return Ok(combine(result, !negate));
    }
    if let Some(name) = value
        .get("exists")
        .and_then(|value| value["field"].as_str())
    {
        property(schema, name)?;
        if schema["invertedIndexConfig"]["indexNullState"] != true {
            return Err("Weaviate null predicates require indexNullState".into());
        }
        return Ok(Filter::Predicate(
            json!({"path":[name],"operator":"IsNull","valueBoolean":negate}),
        ));
    }
    if let Some(boolean) = value.get("bool") {
        let mut result = Vec::new();
        for name in ["filter", "must", "must_not"] {
            if let Some(values) = boolean.get(name).and_then(Value::as_array) {
                for value in values {
                    result.push(filter(schema, value, negate != (name == "must_not"))?);
                }
            }
        }
        if let Some(values) = boolean.get("should").and_then(Value::as_array) {
            if boolean["minimum_should_match"] != 1 {
                return Err("unsupported boolean filter".into());
            }
            result.push(combine(
                values
                    .iter()
                    .map(|value| filter(schema, value, negate))
                    .collect::<Result<_, _>>()?,
                negate,
            ));
        }
        return Ok(combine(result, !negate));
    }
    Err("unsupported Weaviate filter".into())
}

impl HttpSession {
    pub(super) async fn execute_weaviate(
        &mut self,
        request: &Value,
        body: &Value,
        projections: &[Value],
        columns: &[TransferColumn],
        parameters: &[Value],
    ) -> Result<Vec<RecordBatch>, String> {
        if request["api"] != "weaviate" || request["operation"] != "search" {
            return Err("Weaviate supports typed ranked retrieval requests".into());
        }
        let collection = identifier(
            request["index"]
                .as_str()
                .ok_or("missing Weaviate collection")?,
        )?;
        let limit = request["limit"]
            .as_u64()
            .ok_or("Weaviate retrieval requires a limit")?;
        let metric = request["metric"].as_str().ok_or("missing vector metric")?;
        let expected_distance = match metric {
            "cosine" => "cosine",
            "dot" => "dot",
            "l2" => "l2-squared",
            _ => return Err("unsupported Weaviate vector metric".into()),
        };
        let key = format!("weaviate:{collection}");
        if !self.metadata.contains_key(&key) {
            let schema = self
                .request_json(Method::GET, &["v1", "schema", collection], None)
                .await?;
            self.metadata.insert(key.clone(), schema);
        }
        let schema = &self.metadata[&key];
        let vector_name = request["vector_name"].as_str();
        let config = if let Some(name) = vector_name {
            schema["vectorConfig"]
                .get(name)
                .ok_or("Weaviate named vector is missing")?
        } else {
            if schema["vectorConfig"]
                .as_object()
                .is_some_and(|config| !config.is_empty())
            {
                return Err(
                    "Weaviate collection requires an explicit target_vector binding".into(),
                );
            }
            schema
        };
        if config["vectorIndexConfig"]["distance"]
            .as_str()
            .unwrap_or("cosine")
            != expected_distance
        {
            return Err(
                "Weaviate vector distance does not match the logical scoring metric".into(),
            );
        }
        let vector = body["vector"]
            .as_array()
            .ok_or("Weaviate query vector must be an array")?;
        if vector.is_empty()
            || !vector
                .iter()
                .all(|value| value.as_f64().is_some_and(f64::is_finite))
        {
            return Err("Weaviate query vector must contain finite numbers".into());
        }
        let empty =
            || decode_rows(&[], projections, columns, parameters, false).map(|batch| vec![batch]);
        if metric == "cosine" && vector.iter().all(|value| value.as_f64() == Some(0.0)) {
            return empty();
        }
        let where_filter = filter(schema, &body["query"], false)?;
        if matches!(where_filter, Filter::None) {
            return empty();
        }
        let mut near = json!({"vector":vector});
        if let Some(name) = vector_name {
            near["targetVectors"] = json!([name]);
        }
        let thresholds = body["score_filters"]
            .as_array()
            .ok_or("missing score predicates")?;
        let mut distance: Option<f64> = None;
        for threshold in thresholds {
            if threshold["value"].is_null() {
                return empty();
            }
            let value = threshold["value"]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("score threshold must be finite")?;
            let bound = match metric {
                "cosine" => 1.0 - value,
                "dot" => -value,
                "l2" => {
                    if value < 0.0 {
                        return empty();
                    }
                    value * value
                }
                _ => unreachable!(),
            };
            if metric == "cosine" && bound < 0.0 {
                return empty();
            }
            if bound.is_finite() {
                distance = Some(distance.map_or(bound, |current| current.min(bound)));
            }
        }
        if let Some(distance) = distance {
            near["distance"] = json!(distance);
        }
        let mut arguments = vec![
            format!("nearVector:{}", graphql(&near)?),
            format!("limit:{limit}"),
        ];
        if let Filter::Predicate(predicate) = where_filter {
            arguments.push(format!("where:{}", graphql(&predicate)?));
        }
        if let Some(tenant) = request["tenant"].as_str() {
            arguments.push(format!("tenant:{}", json!(tenant)));
        }
        let mut fields = std::collections::BTreeSet::new();
        for projection in projections {
            if projection["source"] == "field" {
                let path = projection["path"]
                    .as_array()
                    .ok_or("missing projection path")?;
                if path.len() != 1 {
                    return Err("Weaviate retrieval supports scalar property projections".into());
                }
                let name = path[0].as_str().ok_or("invalid property projection")?;
                property(schema, name)?;
                fields.insert(identifier(name)?);
            }
        }
        let query = format!(
            "{{Get{{{collection}({}){{{} _additional{{id distance}}}}}}}}",
            arguments.join(","),
            fields.into_iter().collect::<Vec<_>>().join(" ")
        );
        let response = self
            .request_json(
                Method::POST,
                &["v1", "graphql"],
                Some(&json!({"query":query})),
            )
            .await?;
        if response
            .get("errors")
            .is_some_and(|errors| errors.as_array().is_none_or(|errors| !errors.is_empty()))
        {
            return Err("Weaviate GraphQL request failed".into());
        }
        let rows = response["data"]["Get"][collection]
            .as_array()
            .ok_or("invalid Weaviate retrieval response")?;
        if rows.len() as u64 > limit {
            return Err("Weaviate returned more rows than requested".into());
        }
        self.check_rows(rows.len())?;
        let mut hits = Vec::new();
        for row in rows {
            let distance = row["_additional"]["distance"]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("Weaviate omitted a finite vector distance")?;
            let score = match metric {
                "cosine" => 1.0 - distance,
                "dot" => -distance,
                "l2" => {
                    if distance < 0.0 {
                        return Err("invalid squared L2 distance".into());
                    }
                    distance.sqrt()
                }
                _ => unreachable!(),
            };
            let keep = thresholds.iter().all(|threshold| {
                let value = threshold["value"].as_f64().unwrap();
                match threshold["op"].as_str() {
                    Some("gt") => score > value,
                    Some("gte") => score >= value,
                    Some("lt") => score < value,
                    Some("lte") => score <= value,
                    _ => false,
                }
            });
            if keep {
                hits.push(json!({"_source":row,"_score":score,"_id":row["_additional"]["id"]}));
            }
        }
        Ok(vec![decode_rows(
            &hits,
            projections,
            columns,
            parameters,
            false,
        )?])
    }
}
