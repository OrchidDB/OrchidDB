//! Schema-directed structured transport. PostgreSQL uses JSONB with textual
//! leaves; DuckDB uses native values. Both restore the original Arrow schema.
use super::*;
use serde_json::{Value, json};
pub(super) fn is_structured(ty: &DataType) -> bool {
    crate::ir::functions::domain::descriptor(ty).is_none()
        && matches!(
            ty,
            DataType::Struct(_) | DataType::Map(_, _) | DataType::Union(_, _)
        )
}
pub(super) fn encode(value: &ScalarValue) -> SqlResult<Value> {
    // A union's selected child may be null while its tag is still meaningful.
    if !matches!(value, ScalarValue::Union(Some(_), _, _)) && value.is_null() {
        return Ok(Value::Null);
    }
    if crate::ir::functions::domain::is_json(&value.data_type()) {
        return Ok(
            crate::ir::functions::domain::json_text(value)?.map_or(Value::Null, Value::String)
        );
    }
    Ok(match value {
        ScalarValue::Struct(a) => Value::Object(
            a.fields()
                .iter()
                .zip(a.columns())
                .map(|(f, a)| {
                    Ok((
                        f.name().clone(),
                        encode(&ScalarValue::try_from_array(a, 0)?)?,
                    ))
                })
                .collect::<SqlResult<_>>()?,
        ),
        ScalarValue::Map(a) => {
            let entries = a.value(0);
            Value::Array((0..entries.len()).map(|i| Ok(json!({"key": encode(&ScalarValue::try_from_array(entries.column(0), i)?)?, "value": encode(&ScalarValue::try_from_array(entries.column(1), i)?)?}))).collect::<SqlResult<_>>()?)
        }
        ScalarValue::Union(Some((id, v)), fs, _) => {
            json!({"tag": fs.iter().find(|(i,_)| i == id).unwrap().1.name(), "value": encode(v)?})
        }
        ScalarValue::Union(None, _, _) => Value::Null,
        ScalarValue::List(a) => encode_items(a.value(0))?,
        ScalarValue::LargeList(a) => encode_items(a.value(0))?,
        ScalarValue::FixedSizeList(a) => encode_items(a.value(0))?,
        ScalarValue::Binary(Some(b))
        | ScalarValue::LargeBinary(Some(b))
        | ScalarValue::BinaryView(Some(b)) => Value::String(format!(
            "\\x{}",
            b.iter().map(|v| format!("{v:02x}")).collect::<String>()
        )),
        _ => {
            let a = value.to_array()?;
            Value::String(arrow::util::display::array_value_to_string(a.as_ref(), 0)?)
        }
    })
}
fn encode_items(items: ArrayRef) -> SqlResult<Value> {
    Ok(Value::Array(
        (0..items.len())
            .map(|i| encode(&ScalarValue::try_from_array(&items, i)?))
            .collect::<SqlResult<_>>()?,
    ))
}
pub(super) fn decode(value: &Value, ty: &DataType) -> SqlResult<ScalarValue> {
    if value.is_null() {
        return Ok(ScalarValue::try_from(ty)?);
    }
    if crate::ir::functions::domain::is_json(ty) {
        return crate::ir::functions::domain::json_scalar(
            value.as_str().ok_or_else(|| {
                SqlError::Conversion("expected serialized JSON domain leaf".into())
            })?,
        )
        .map_err(SqlError::from);
    }
    let invalid = || SqlError::Conversion(format!("invalid structured value {value} for {ty}"));
    Ok(match ty {
        DataType::Struct(fs) => {
            let object = value.as_object().ok_or_else(invalid)?;
            let columns = fs
                .iter()
                .map(|f| {
                    decode(object.get(f.name()).unwrap_or(&Value::Null), f.data_type())?
                        .to_array()
                        .map_err(SqlError::from)
                })
                .collect::<SqlResult<_>>()?;
            ScalarValue::Struct(Arc::new(arrow::array::StructArray::try_new(
                fs.clone(),
                columns,
                None,
            )?))
        }
        DataType::Map(f, sorted) => {
            let DataType::Struct(fs) = f.data_type() else {
                return Err(invalid());
            };
            let entries = value.as_array().ok_or_else(invalid)?;
            let keys = entries
                .iter()
                .map(|e| decode(&e["key"], fs[0].data_type()))
                .collect::<SqlResult<Vec<_>>>()?;
            let values = entries
                .iter()
                .map(|e| decode(&e["value"], fs[1].data_type()))
                .collect::<SqlResult<Vec<_>>>()?;
            let a = |v: Vec<ScalarValue>, t: &DataType| -> SqlResult<ArrayRef> {
                if v.is_empty() {
                    Ok(arrow::array::new_empty_array(t))
                } else {
                    Ok(ScalarValue::iter_to_array(v)?)
                }
            };
            let entries = arrow::array::StructArray::try_new(
                fs.clone(),
                vec![a(keys, fs[0].data_type())?, a(values, fs[1].data_type())?],
                None,
            )?;
            ScalarValue::Map(Arc::new(arrow::array::MapArray::try_new(
                f.clone(),
                OffsetBuffer::from_lengths([entries.len()]),
                entries,
                None,
                *sorted,
            )?))
        }
        DataType::Union(fs, mode) => {
            let tag = value["tag"].as_str().ok_or_else(invalid)?;
            let (id, f) = fs
                .iter()
                .find(|(_, f)| f.name() == tag)
                .ok_or_else(invalid)?;
            ScalarValue::Union(
                Some((id, Box::new(decode(&value["value"], f.data_type())?))),
                fs.clone(),
                *mode,
            )
        }
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
            let values = value
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|v| decode(v, f.data_type()))
                .collect::<SqlResult<Vec<_>>>()?;
            let a = if values.is_empty() {
                arrow::array::new_empty_array(f.data_type())
            } else {
                ScalarValue::iter_to_array(values)?
            };
            let list: ArrayRef = Arc::new(ListArray::try_new(
                f.clone(),
                OffsetBuffer::from_lengths([a.len()]),
                a,
                None,
            )?);
            ScalarValue::try_from_array(&arrow::compute::cast(&list, ty)?, 0)?
        }
        DataType::BinaryView => {
            let v = crate::federation::json_scalar(value, &DataType::Binary)
                .map_err(SqlError::Conversion)?;
            v.cast_to(ty)?
        }
        _ => crate::federation::json_scalar(value, ty).map_err(SqlError::Conversion)?,
    })
}
pub(super) fn literal(dialect: SqlDialect, value: &ScalarValue) -> SqlResult<String> {
    if value.is_null() && !matches!(value, ScalarValue::Union(Some(_), _, _)) {
        return Ok("NULL".into());
    }
    if dialect == SqlDialect::Postgres {
        return Ok(format!(
            "CAST({} AS JSONB)",
            super::literals::string_literal(dialect, &encode(value)?.to_string())?
        ));
    }
    if value.is_null() && !matches!(value, ScalarValue::Union(Some(_), _, _)) {
        return Ok("NULL".into());
    }
    Ok(match value {
        ScalarValue::Struct(a) => format!(
            "struct_pack({})",
            a.fields()
                .iter()
                .zip(a.columns())
                .map(|(f, a)| Ok(format!(
                    "{} := {}",
                    dialect.quote_ident(f.name()),
                    super::literals::sql_literal(dialect, &ScalarValue::try_from_array(a, 0)?)?
                )))
                .collect::<SqlResult<Vec<_>>>()?
                .join(", ")
        ),
        ScalarValue::Map(a) => {
            let e = a.value(0);
            let list = |a: ArrayRef| -> SqlResult<String> {
                let values = (0..a.len())
                    .map(|i| {
                        super::literals::sql_literal(dialect, &ScalarValue::try_from_array(&a, i)?)
                    })
                    .collect::<SqlResult<Vec<_>>>()?;
                Ok(format!(
                    "CAST([{}] AS {}[])",
                    values.join(", "),
                    crate::ir::functions::duckdb_type(a.data_type())?
                ))
            };
            format!(
                "map({}, {})",
                list(e.column(0).clone())?,
                list(e.column(1).clone())?
            )
        }
        ScalarValue::Union(Some((id, v)), fs, _) => format!(
            "CAST(union_value({} := {}) AS {})",
            dialect.quote_ident(fs.iter().find(|(i, _)| i == id).unwrap().1.name()),
            super::literals::sql_literal(dialect, v)?,
            crate::ir::functions::duckdb_type(&value.data_type())?
        ),
        _ => return Err(SqlError::Unsupported("structured literal".into())),
    })
}

/// Constant folding can produce structured literals before SQL placement.
/// Preserve their representation and schema just as for transported columns.
pub(super) fn literal_expr(value: &ScalarValue, dialect: SqlDialect) -> SqlResult<Expr> {
    use crate::ir::functions::logical::{LogicalFunction, SqlFunctionMapping};
    use std::hash::{Hash, Hasher};
    let sql = literal(dialect, value)?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    sql.hash(&mut hash);
    value.data_type().hash(&mut hash);
    let name = format!("__portable_structured_literal_{:x}", hash.finish());
    let captured = value.clone();
    let native = datafusion::logical_expr::create_udf(
        &name,
        vec![],
        value.data_type(),
        datafusion::logical_expr::Volatility::Immutable,
        Arc::new(move |_| {
            Ok(datafusion::logical_expr::ColumnarValue::Scalar(
                captured.clone(),
            ))
        }),
    );
    Ok(LogicalFunction::new(
        name,
        Arc::new(native),
        BTreeMap::from([(
            dialect.name().into(),
            SqlFunctionMapping {
                value: sql,
                ordering: None,
            },
        )]),
    )
    .into_udf()
    .call(vec![]))
}
