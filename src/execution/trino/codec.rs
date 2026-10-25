//! Trino JSON result pages to typed Arrow batches (one bounded page at a time).
use arrow::{
    array::{ArrayRef, ListArray, MapArray, StructArray, new_empty_array},
    buffer::OffsetBuffer,
    datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit},
    record_batch::{RecordBatch, RecordBatchOptions},
};
use base64::Engine;
use datafusion::common::ScalarValue;
use num_traits::ToPrimitive;
use serde_json::Value;
use std::{str::FromStr, sync::Arc};

pub(super) fn schema(columns: &Value) -> Result<SchemaRef, String> {
    let columns = columns.as_array().ok_or("Trino columns must be an array")?;
    let fields = columns
        .iter()
        .map(|column| {
            Ok(Field::new(
                column["name"].as_str().ok_or("Trino column missing name")?,
                data_type(&column["typeSignature"], 0)?,
                true,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Arc::new(Schema::new(fields)))
}
fn data_type(signature: &Value, depth: usize) -> Result<DataType, String> {
    if depth > 64 {
        return Err("Trino type nesting limit".into());
    }
    let name = signature["rawType"]
        .as_str()
        .ok_or("Trino column missing type signature")?;
    let args = signature["arguments"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let number = |i: usize| {
        args.get(i)
            .and_then(|a| a["value"].as_u64())
            .ok_or("missing numeric Trino type argument".to_string())
    };
    let child = |i: usize| {
        data_type(
            &args.get(i).ok_or("missing Trino type argument")?["value"],
            depth + 1,
        )
    };
    Ok(match name {
        "unknown" => DataType::Null,
        "boolean" => DataType::Boolean,
        "tinyint" => DataType::Int8,
        "smallint" => DataType::Int16,
        "integer" => DataType::Int32,
        "bigint" => DataType::Int64,
        "real" => DataType::Float32,
        "double" => DataType::Float64,
        "varchar" | "char" | "json" | "uuid" | "ipaddress" => DataType::Utf8,
        "varbinary" => DataType::Binary,
        "date" => DataType::Date32,
        "decimal" => {
            let p = number(0)?;
            let s = number(1)?;
            if p > 38 || s > p {
                return Err("unsupported Trino decimal precision".into());
            }
            DataType::Decimal128(p as u8, s as i8)
        }
        "time" => {
            let p = number(0).unwrap_or(3);
            if p > 9 {
                return Err(
                    "Trino picosecond time cannot be represented losslessly in Arrow".into(),
                );
            }
            DataType::Time64(TimeUnit::Nanosecond)
        }
        "timestamp" | "timestamp with time zone" => {
            let p = number(0).unwrap_or(3);
            let unit = match p {
                0..=3 => TimeUnit::Millisecond,
                4..=6 => TimeUnit::Microsecond,
                7..=9 => TimeUnit::Nanosecond,
                _ => {
                    return Err(
                        "Trino picosecond timestamp cannot be represented losslessly in Arrow"
                            .into(),
                    );
                }
            };
            DataType::Timestamp(
                unit,
                if name.ends_with("time zone") {
                    Some("UTC".into())
                } else {
                    None
                },
            )
        }
        "array" => DataType::List(Arc::new(Field::new("item", child(0)?, true))),
        "map" => DataType::Map(
            Arc::new(Field::new(
                "entries",
                DataType::Struct(
                    vec![
                        Field::new("keys", child(0)?, false),
                        Field::new("values", child(1)?, true),
                    ]
                    .into(),
                ),
                false,
            )),
            false,
        ),
        "row" => DataType::Struct(
            args.iter()
                .enumerate()
                .map(|(index, arg)| {
                    let value = &arg["value"];
                    let name = value["fieldName"]["name"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("field{index}"));
                    Ok(Field::new(
                        name,
                        data_type(&value["typeSignature"], depth + 1)?,
                        true,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?
                .into(),
        ),
        other => {
            return Err(format!(
                "unsupported Trino result type {other}; cast explicitly to an Arrow-compatible type"
            ));
        }
    })
}
pub(super) fn batch(schema: SchemaRef, rows: &[Value]) -> Result<RecordBatch, String> {
    let mut values: Vec<Vec<ScalarValue>> = schema
        .fields()
        .iter()
        .map(|_| Vec::with_capacity(rows.len()))
        .collect();
    for row in rows {
        let row = row.as_array().ok_or("Trino result row is not an array")?;
        if row.len() != schema.fields().len() {
            return Err("Trino result row width mismatch".into());
        }
        for ((value, field), column) in row.iter().zip(schema.fields()).zip(&mut values) {
            column.push(scalar(value, field.data_type())?);
        }
    }
    let arrays = values
        .into_iter()
        .zip(schema.fields())
        .map(|(values, field)| {
            if values.is_empty() {
                Ok(new_empty_array(field.data_type()))
            } else {
                ScalarValue::iter_to_array(values).map_err(|e| e.to_string())
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    RecordBatch::try_new_with_options(
        schema,
        arrays,
        &RecordBatchOptions::new().with_row_count(Some(rows.len())),
    )
    .map_err(|e| e.to_string())
}
fn text(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or("Trino value must be a string".into())
}
fn number<T: FromStr>(value: &Value) -> Result<T, String> {
    let s = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Err("Trino value must be numeric".into()),
    };
    s.parse()
        .map_err(|_| format!("Trino numeric value out of range: {s}"))
}
fn values_array(values: Vec<ScalarValue>, ty: &DataType) -> Result<ArrayRef, String> {
    if values.is_empty() {
        Ok(new_empty_array(ty))
    } else {
        ScalarValue::iter_to_array(values).map_err(|e| e.to_string())
    }
}
fn scalar(value: &Value, ty: &DataType) -> Result<ScalarValue, String> {
    if value.is_null() {
        return ScalarValue::try_from(ty).map_err(|e| e.to_string());
    }
    Ok(match ty {
        DataType::Null => return Err("non-null value for Trino unknown type".into()),
        DataType::Boolean => {
            ScalarValue::Boolean(Some(value.as_bool().ok_or("invalid Trino boolean")?))
        }
        DataType::Int8 => ScalarValue::Int8(Some(number(value)?)),
        DataType::Int16 => ScalarValue::Int16(Some(number(value)?)),
        DataType::Int32 => ScalarValue::Int32(Some(number(value)?)),
        DataType::Int64 => ScalarValue::Int64(Some(number(value)?)),
        DataType::Float32 => ScalarValue::Float32(Some(number(value)?)),
        DataType::Float64 => ScalarValue::Float64(Some(number(value)?)),
        DataType::Utf8 => ScalarValue::Utf8(Some(text(value)?.into())),
        DataType::Binary => ScalarValue::Binary(Some(
            base64::engine::general_purpose::STANDARD
                .decode(text(value)?)
                .map_err(|e| e.to_string())?,
        )),
        DataType::Date32 => {
            let date = chrono::NaiveDate::parse_from_str(text(value)?, "%Y-%m-%d")
                .map_err(|e| e.to_string())?;
            ScalarValue::Date32(Some(
                date.signed_duration_since(chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
                    .num_days()
                    .try_into()
                    .map_err(|_| "date out of range")?,
            ))
        }
        DataType::Time64(TimeUnit::Nanosecond) => {
            use chrono::Timelike;
            let time = chrono::NaiveTime::parse_from_str(text(value)?, "%H:%M:%S%.f")
                .map_err(|e| e.to_string())?;
            ScalarValue::Time64Nanosecond(Some(
                time.num_seconds_from_midnight() as i64 * 1_000_000_000 + time.nanosecond() as i64,
            ))
        }
        DataType::Timestamp(unit, zone) => {
            let raw = text(value)?;
            let datetime = if zone.is_some() {
                let (date, zone) = raw.rsplit_once(' ').ok_or("zoned timestamp missing zone")?;
                let naive = chrono::NaiveDateTime::parse_from_str(date, "%Y-%m-%d %H:%M:%S%.f")
                    .map_err(|e| e.to_string())?;
                use chrono::TimeZone;
                if let Ok(zone) = zone.parse::<chrono_tz::Tz>() {
                    zone.from_local_datetime(&naive)
                        .single()
                        .ok_or("ambiguous Trino timestamp")?
                        .with_timezone(&chrono::Utc)
                } else {
                    let zone = chrono::FixedOffset::from_str(zone).map_err(|e| e.to_string())?;
                    zone.from_local_datetime(&naive)
                        .single()
                        .ok_or("invalid Trino timestamp")?
                        .with_timezone(&chrono::Utc)
                }
            } else {
                chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f")
                    .map_err(|e| e.to_string())?
                    .and_utc()
            };
            match unit {
                TimeUnit::Second => {
                    ScalarValue::TimestampSecond(Some(datetime.timestamp()), zone.clone())
                }
                TimeUnit::Millisecond => ScalarValue::TimestampMillisecond(
                    Some(datetime.timestamp_millis()),
                    zone.clone(),
                ),
                TimeUnit::Microsecond => ScalarValue::TimestampMicrosecond(
                    Some(datetime.timestamp_micros()),
                    zone.clone(),
                ),
                TimeUnit::Nanosecond => ScalarValue::TimestampNanosecond(
                    Some(
                        datetime
                            .timestamp_nanos_opt()
                            .ok_or("timestamp out of range")?,
                    ),
                    zone.clone(),
                ),
            }
        }
        DataType::Decimal128(p, s) => {
            let decimal =
                bigdecimal::BigDecimal::from_str(text(value)?).map_err(|e| e.to_string())?;
            let (coefficient, scale) = decimal.as_bigint_and_exponent();
            let difference = *s as i64 - scale;
            let coefficient = if difference >= 0 {
                coefficient
                    * num_bigint::BigInt::from(10u8).pow(
                        difference
                            .try_into()
                            .map_err(|_| "decimal scale out of range")?,
                    )
            } else {
                let divisor = num_bigint::BigInt::from(10u8).pow(
                    (-difference)
                        .try_into()
                        .map_err(|_| "decimal scale out of range")?,
                );
                if &coefficient % &divisor != num_bigint::BigInt::from(0) {
                    return Err("decimal precision loss".into());
                }
                coefficient / divisor
            };
            ScalarValue::Decimal128(
                Some(coefficient.to_i128().ok_or("decimal overflow")?),
                *p,
                *s,
            )
        }
        DataType::List(field) => {
            let items = value.as_array().ok_or("Trino array must be an array")?;
            let values = items
                .iter()
                .map(|v| scalar(v, field.data_type()))
                .collect::<Result<Vec<_>, _>>()?;
            let array = ListArray::try_new(
                field.clone(),
                OffsetBuffer::new(
                    vec![0, i32::try_from(items.len()).map_err(|_| "array too long")?].into(),
                ),
                values_array(values, field.data_type())?,
                None,
            )
            .map_err(|e| e.to_string())?;
            ScalarValue::List(Arc::new(array))
        }
        DataType::Struct(fields) => {
            let items = value.as_array().ok_or("Trino row must be an array")?;
            if items.len() != fields.len() {
                return Err("Trino row field count mismatch".into());
            }
            let arrays = items
                .iter()
                .zip(fields)
                .map(|(v, f)| {
                    scalar(v, f.data_type())?
                        .to_array_of_size(1)
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?;
            ScalarValue::Struct(Arc::new(
                StructArray::try_new(fields.clone(), arrays, None).map_err(|e| e.to_string())?,
            ))
        }
        DataType::Map(field, sorted) => {
            let values = value.as_object().ok_or("Trino map must be an object")?;
            let DataType::Struct(fields) = field.data_type() else {
                return Err("invalid Arrow map".into());
            };
            let keys = values
                .keys()
                .map(|k| scalar(&Value::String(k.clone()), fields[0].data_type()))
                .collect::<Result<Vec<_>, _>>()?;
            let values_array = values_array(
                values
                    .values()
                    .map(|v| scalar(v, fields[1].data_type()))
                    .collect::<Result<Vec<_>, _>>()?,
                fields[1].data_type(),
            )?;
            let keys = super::codec::values_array(keys, fields[0].data_type())?;
            let entries = StructArray::try_new(fields.clone(), vec![keys, values_array], None)
                .map_err(|e| e.to_string())?;
            ScalarValue::Map(Arc::new(
                MapArray::try_new(
                    field.clone(),
                    OffsetBuffer::new(
                        vec![0, i32::try_from(values.len()).map_err(|_| "map too large")?].into(),
                    ),
                    entries,
                    None,
                    *sorted,
                )
                .map_err(|e| e.to_string())?,
            ))
        }
        other => return Err(format!("unsupported Arrow result type {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn nested_and_binary_values_preserve_types() {
        let fields = vec![
            Field::new(
                "items",
                DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
                true,
            ),
            Field::new("bytes", DataType::Binary, true),
            Field::new("amount", DataType::Decimal128(12, 2), true),
        ];
        let batch = batch(
            Arc::new(Schema::new(fields)),
            &[json!([[1, null, 3], "AAEC", "123.40"])],
        )
        .unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(
            ScalarValue::try_from_array(batch.column(2), 0).unwrap(),
            ScalarValue::Decimal128(Some(12340), 12, 2)
        );
    }
    #[test]
    fn rejects_lossy_precision() {
        assert!(
            data_type(
                &json!({"rawType":"timestamp","arguments":[{"kind":"LONG","value":12}]}),
                0
            )
            .is_err()
        );
        assert!(scalar(&json!("1.234"), &DataType::Decimal128(10, 2)).is_err());
    }
}
