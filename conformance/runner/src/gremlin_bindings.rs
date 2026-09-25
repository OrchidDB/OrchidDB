//! Transport decoding only; execution is always GraphEngine::gremlin_with_bindings.
use orchiddb::language::gremlin::{GremlinBinding, semantics::GValue};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub fn bindings(value: &Value) -> Result<HashMap<String, GremlinBinding>, String> {
    let Some(values) = value.as_object() else {
        return Ok(HashMap::new());
    };
    values
        .iter()
        .map(|(key, v)| {
            let value = match v["type"].as_str() {
                Some("lambda") => GremlinBinding::Lambda(
                    v["script"]
                        .as_str()
                        .ok_or("Lambda script must be a string")?
                        .into(),
                ),
                Some("predicate") => GremlinBinding::Predicate {
                    operator: v["operator"]
                        .as_str()
                        .ok_or("Missing predicate operator")?
                        .into(),
                    value: literal(&v["value"])?,
                },
                _ => GremlinBinding::Value(literal(v)?),
            };
            Ok((key.clone(), value))
        })
        .collect()
}
fn literal(v: &Value) -> Result<GValue, String> {
    let value = &v["value"];
    let number = || {
        value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string())
    };
    Ok(
        match v["type"].as_str().ok_or("Missing native value type")? {
            "sack_callbacks" => GValue::SackCallbacks {
                supplier: v["supplier"].as_str().ok_or("Expected supplier callback")?.into(),
                split: v.get("split").map(|s| s.as_str().map(str::to_owned).ok_or("Expected split callback")).transpose()?,
            },
            "null" => GValue::Null,
            "string" => GValue::String(value.as_str().ok_or("Expected string")?.into()),
            "boolean" => GValue::Bool(value.as_bool().ok_or("Expected boolean")?),
            "byte" => GValue::Byte(number().parse().map_err(|_| "Invalid byte")?),
            "short" => GValue::Short(number().parse().map_err(|_| "Invalid short")?),
            "int" => GValue::Int(number().parse().map_err(|_| "Invalid integer")?),
            "long" => GValue::Long(number().parse().map_err(|_| "Invalid long")?),
            "float" => GValue::Float32(number().parse().map_err(|_| "Invalid float")?),
            "double" => GValue::Float(number().parse().map_err(|_| "Invalid double")?),
            "bigint" => GValue::BigInt(number().parse().map_err(|_| "Invalid bigint")?),
            "bigdecimal" => GValue::BigDecimal(number().parse().map_err(|_| "Invalid decimal")?),
            "datetime" => {
                GValue::DateTime(value.as_str().ok_or("Expected datetime string")?.into())
            }
            "token" => GValue::Token(value.as_str().ok_or("Expected token")?.into()),
            "direction" => {
                GValue::DirectionToken(value.as_str().ok_or("Expected direction")?.into())
            }
            "vertex" => GValue::VertexRef {
                id: Box::new(literal(&v["id"])?),
                label: v["label"].as_str().unwrap_or("vertex").into(),
            },
            "vertex_property" => GValue::VertexPropertyRef {
                id: Box::new(literal(&v["id"])?),
                owner: Box::new(literal(&v["owner"])?),
                key: v["key"].as_str().ok_or("Expected vertex property key")?.into(),
            },
            "edge" => GValue::EdgeRef {
                id: Box::new(literal(&v["id"])?),
            },
            "list" | "set" => {
                let items = value
                    .as_array()
                    .ok_or("Expected array")?
                    .iter()
                    .map(literal)
                    .collect::<Result<Vec<_>, _>>()?;
                if v["type"] == "set" {
                    GValue::Set(items)
                } else {
                    GValue::List(items)
                }
            }
            "map" => {
                let entries = value
                    .as_array()
                    .ok_or("Expected map entries")?
                    .iter()
                    .map(|pair| {
                        let pair = pair
                            .as_array()
                            .filter(|p| p.len() == 2)
                            .ok_or("Invalid map pair")?;
                        Ok((literal(&pair[0])?, literal(&pair[1])?))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                if entries.iter().all(|(k, _)| matches!(k, GValue::String(_))) {
                    GValue::Map(
                        entries
                            .into_iter()
                            .map(|(k, v)| {
                                let GValue::String(k) = k else { unreachable!() };
                                (k, v)
                            })
                            .collect::<BTreeMap<_, _>>(),
                    )
                } else {
                    GValue::TypedMap(entries)
                }
            }
            other => return Err(format!("Unsupported Gremlin binding type: {other}")),
        },
    )
}
