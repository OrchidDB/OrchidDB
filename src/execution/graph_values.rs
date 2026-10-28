//! Shared graph descriptor decoding for foreign-language source cursors.
//! SQL has already selected the rows; this module only restores value encoding.
use arrow::array::ArrayRef;
use arrow::datatypes::DataType;
use base64::Engine;
use datafusion::common::ScalarValue;
use serde_json::{Value, json};

pub(crate) const ELEMENT_PREFIX: &str = "orchid.graph.e1:";
pub(crate) const PATH_PREFIX: &str = "orchid.graph.p1:";

/// Decode the adapter's lossless, typed transport at the result boundary.
pub(crate) fn decode_array(array: ArrayRef) -> Result<ArrayRef, String> {
    use crate::ir::{rel::native_values as native, value::Value as GraphValue};
    if !native::is_value(array.data_type()) {
        let rows = array.len();
        return native::typed::evaluate("element", &[array], rows);
    }
    let mut decoded = Vec::with_capacity(array.len());
    for i in 0..array.len() {
        let scalar = ScalarValue::try_from_array(&array, i).map_err(|e| e.to_string())?;
        if scalar.is_null() {
            decoded.push(scalar);
            continue;
        }
        let cell = crate::federation::scalar_json(&scalar).map_err(|e| e.to_string())?;
        let text = cell[native::FIELD]
            .as_str()
            .ok_or("invalid graph carrier")?;
        let (payload, path) = if let Some(text) = text.strip_prefix(ELEMENT_PREFIX) {
            (text, false)
        } else if let Some(text) = text.strip_prefix(PATH_PREFIX) {
            (text, true)
        } else {
            // Constants can already use the canonical carrier.
            native::json(&cell)?;
            decoded.push(scalar);
            continue;
        };
        let entries: Vec<Value> = serde_json::from_str(payload).map_err(|e| e.to_string())?;
        if !path && entries.len() != 1 {
            return Err("invalid element descriptor count".into());
        }
        let empty = base64::engine::general_purpose::STANDARD.encode(
            crate::ir::catalog::snapshot::binary::encode_value_bytes(&GraphValue::Path(vec![])),
        );
        let mut value =
            crate::federation::json_scalar(&json!({native::FIELD:empty}), &native::value_type())?;
        for entry in entries {
            let ty =
                crate::compiler::data_type(entry[0].as_str().ok_or("missing descriptor type")?)?;
            let descriptor = crate::federation::json_scalar(&normalize(&entry[1], &ty)?, &ty)?
                .to_array()
                .map_err(|e| e.to_string())?;
            let element = native::typed::evaluate("element", &[descriptor], 1)?;
            let output = if path {
                native::typed::evaluate(
                    "path_append",
                    &[value.to_array().map_err(|e| e.to_string())?, element],
                    1,
                )?
            } else {
                element
            };
            value = ScalarValue::try_from_array(&output, 0).map_err(|e| e.to_string())?;
        }
        decoded.push(value);
    }
    if decoded.is_empty() {
        return Ok(arrow::array::new_empty_array(&native::value_type()));
    }
    ScalarValue::iter_to_array(decoded).map_err(|e| e.to_string())
}

fn normalize(value: &Value, ty: &DataType) -> Result<Value, String> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    Ok(match ty {
        DataType::Struct(fields) => {
            if let Some(values) = value.as_array() {
                if values.len() != fields.len() {
                    return Err("graph descriptor row width mismatch".into());
                }
                fields
                    .iter()
                    .zip(values)
                    .map(|(field, value)| {
                        Ok((field.name().clone(), normalize(value, field.data_type())?))
                    })
                    .collect::<Result<serde_json::Map<_, _>, String>>()?
                    .into()
            } else {
                let object = value.as_object().ok_or("graph descriptor must be a row")?;
                fields
                    .iter()
                    .map(|field| {
                        let value = object.get(field.name()).ok_or_else(|| {
                            format!("missing graph descriptor field {}", field.name())
                        })?;
                        Ok((field.name().clone(), normalize(value, field.data_type())?))
                    })
                    .collect::<Result<serde_json::Map<_, _>, String>>()?
                    .into()
            }
        }
        DataType::List(field) | DataType::LargeList(field) => value
            .as_array()
            .ok_or("graph descriptor list expected")?
            .iter()
            .map(|value| normalize(value, field.data_type()))
            .collect::<Result<Vec<_>, _>>()?
            .into(),
        _ => value.clone(),
    })
}

pub(crate) fn decode(command: &Value) -> Result<Value, String> {
    let ty = crate::compiler::data_type(
        command["data_type"]
            .as_str()
            .ok_or("missing graph descriptor type")?,
    )?;
    let array = if let Some(ipc) = command.get("ipc") {
        let bytes: Vec<u8> = serde_json::from_value(ipc.clone()).map_err(|e| e.to_string())?;
        let mut reader =
            arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
                .map_err(|e| e.to_string())?;
        let batch = reader
            .next()
            .ok_or("missing graph descriptor batch")?
            .map_err(|e| e.to_string())?;
        if batch.num_columns() != 1 || batch.column(0).data_type() != &ty || reader.next().is_some()
        {
            return Err("graph descriptor batch schema mismatch".into());
        }
        batch.column(0).clone()
    } else {
        let rows = command["values"]
            .as_array()
            .ok_or("missing graph descriptor values")?;
        if rows.is_empty() {
            return Ok(json!([]));
        }
        let values = rows
            .iter()
            .map(|value| crate::federation::json_scalar(&normalize(value, &ty)?, &ty))
            .collect::<Result<Vec<_>, _>>()?;
        ScalarValue::iter_to_array(values).map_err(|e| e.to_string())?
    };
    let rows = array.len();
    if rows == 0 {
        return Ok(json!([]));
    }
    let result = decode_array(array)?;
    (0..rows)
        .map(|i| {
            let value = ScalarValue::try_from_array(&result, i).map_err(|e| e.to_string())?;
            crate::federation::scalar_json(&value).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed_transport_and_preserves_empty_path() {
        use crate::ir::rel::native_values as native;
        let ty = crate::federation::type_name(&native::value_type()).unwrap();
        let decode_value =
            |text: String| decode(&json!({"data_type":ty,"values":[{native::FIELD:text}]}));
        for text in [
            format!("{ELEMENT_PREFIX}[]"),
            format!("{ELEMENT_PREFIX}[[null,0]]"),
            format!("{PATH_PREFIX}invalid"),
        ] {
            assert!(decode_value(text).is_err());
        }
        let empty = decode_value(format!("{PATH_PREFIX}[]")).unwrap();
        assert!(native::json(&empty[0]).is_ok());
        assert!(
            !empty[0][native::FIELD]
                .as_str()
                .unwrap()
                .starts_with(PATH_PREFIX)
        );
    }

    #[test]
    fn positional_descriptor_checks_width() {
        let ty = crate::compiler::data_type("struct_fields:[[\"id\",\"int64\"]]").unwrap();
        assert_eq!(
            normalize(&json!(["9007199254740993"]), &ty).unwrap(),
            json!({"id":"9007199254740993"})
        );
        assert!(normalize(&json!([]), &ty).is_err());
        assert!(normalize(&json!({}), &ty).is_err());
    }
}
