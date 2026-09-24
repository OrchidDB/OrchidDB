//! Rows for SQL execution.

use super::*;

pub(super) fn rows_to_batch(schema: &SchemaRef, rows: &[Vec<SqlValue>]) -> SqlResult<RecordBatch> {
    let width = schema.fields().len();
    for (index, row) in rows.iter().enumerate() {
        if row.len() != width {
            return Err(SqlError::Conversion(format!(
                "row {index} has {} columns, expected {width}",
                row.len()
            )));
        }
    }
    let mut columns: Vec<ArrayRef> = Vec::with_capacity(width);
    for (index, field) in schema.fields().iter().enumerate() {
        columns.push(build_column(field, rows, index)?);
    }
    RecordBatch::try_new(Arc::clone(schema), columns).map_err(SqlError::from)
}

pub(super) fn build_column(field: &Field, rows: &[Vec<SqlValue>], index: usize) -> SqlResult<ArrayRef> {
    let mismatch = |value: &SqlValue| {
        SqlError::Conversion(format!(
            "column `{}` expected {}, engine returned {value:?}",
            field.name(),
            field.data_type()
        ))
    };
    Ok(match field.data_type() {
        DataType::Null => new_null_array(&DataType::Null, rows.len()),
        DataType::Boolean => {
            let mut builder = BooleanBuilder::with_capacity(rows.len());
            for row in rows {
                match &row[index] {
                    SqlValue::Null => builder.append_null(),
                    SqlValue::Bool(value) => builder.append_value(*value),
                    other => return Err(mismatch(other)),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Int32 => {
            let mut builder = Int32Builder::with_capacity(rows.len());
            for row in rows {
                match &row[index] {
                    SqlValue::Null => builder.append_null(),
                    SqlValue::Int(value) => builder
                        .append_value(i32::try_from(*value).map_err(|_| mismatch(&row[index]))?),
                    SqlValue::ExactNumber(value) => builder
                        .append_value(value.parse::<i32>().map_err(|_| mismatch(&row[index]))?),
                    other => return Err(mismatch(other)),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Int64 => {
            let mut builder = Int64Builder::with_capacity(rows.len());
            for row in rows {
                match &row[index] {
                    SqlValue::Null => builder.append_null(),
                    SqlValue::Int(value) => builder.append_value(*value),
                    SqlValue::ExactNumber(value) => builder
                        .append_value(value.parse::<i64>().map_err(|_| mismatch(&row[index]))?),
                    // Engines widen some integer aggregates to floating point;
                    // accept exact integral values back.
                    SqlValue::Float(value)
                        if value.fract() == 0.0 && value.abs() < i64::MAX as f64 =>
                    {
                        builder.append_value(*value as i64)
                    }
                    other => return Err(mismatch(other)),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Float64 => {
            let mut builder = Float64Builder::with_capacity(rows.len());
            for row in rows {
                match &row[index] {
                    SqlValue::Null => builder.append_null(),
                    SqlValue::Int(value) => builder.append_value(*value as f64),
                    SqlValue::ExactNumber(value) => builder
                        .append_value(value.parse::<f64>().map_err(|_| mismatch(&row[index]))?),
                    SqlValue::Float(value) => builder.append_value(*value),
                    other => return Err(mismatch(other)),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Utf8 => {
            let mut builder = StringBuilder::new();
            for row in rows {
                match &row[index] {
                    SqlValue::Null => builder.append_null(),
                    SqlValue::Text(value) => builder.append_value(value),
                    other => return Err(mismatch(other)),
                }
            }
            Arc::new(builder.finish())
        }
        // Narrow and unsigned integers come back from the engine widened to
        // i64 (or, above `i64::MAX`, as exact text). Rebuild through the i64
        // column and a checked cast, so a value outside the declared width is
        // a conversion error instead of a wrapped or nulled value.
        DataType::Int8
        | DataType::Int16
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32 => {
            let wide = build_column(
                &Field::new(field.name(), DataType::Int64, true),
                rows,
                index,
            )?;
            checked_cast(&wide, field)?
        }
        DataType::UInt64 => {
            let mut builder = arrow::array::UInt64Builder::with_capacity(rows.len());
            for row in rows {
                match &row[index] {
                    SqlValue::Null => builder.append_null(),
                    SqlValue::Int(value) => builder
                        .append_value(u64::try_from(*value).map_err(|_| mismatch(&row[index]))?),
                    SqlValue::ExactNumber(value) => builder
                        .append_value(value.parse::<u64>().map_err(|_| mismatch(&row[index]))?),
                    other => return Err(mismatch(other)),
                }
            }
            Arc::new(builder.finish())
        }
        // Engines hand REAL values back widened to f64; narrowing them again
        // is exact.
        DataType::Float32 => {
            let wide = build_column(
                &Field::new(field.name(), DataType::Float64, true),
                rows,
                index,
            )?;
            checked_cast(&wide, field)?
        }
        DataType::Decimal128(precision, scale) => {
            let mut builder = Decimal128Builder::with_capacity(rows.len())
                .with_precision_and_scale(*precision, *scale)?;
            for row in rows {
                match &row[index] {
                    SqlValue::Null => builder.append_null(),
                    SqlValue::Int(value) => {
                        let scaled = i128::from(*value)
                            .checked_mul(10_i128.pow((*scale).max(0) as u32))
                            .ok_or_else(|| mismatch(&row[index]))?;
                        builder.append_value(scaled);
                    }
                    SqlValue::ExactNumber(value) => {
                        builder.append_value(
                            parse_decimal_i128(value, *scale).map_err(|_| mismatch(&row[index]))?,
                        );
                    }
                    other => return Err(mismatch(other)),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::List(inner) => {
            // Rebuild the list column from the engine's per-row lists: flatten
            // the elements into a single child column (recursing through the
            // same conversion, so nested lists work), then re-slice it with
            // offsets.
            let mut offsets: Vec<i32> = vec![0];
            let mut flat: Vec<Vec<SqlValue>> = Vec::new();
            let mut present: Vec<bool> = Vec::with_capacity(rows.len());
            for row in rows {
                match &row[index] {
                    SqlValue::Null => present.push(false),
                    SqlValue::List(items) => {
                        present.push(true);
                        flat.extend(items.iter().map(|item| vec![item.clone()]));
                    }
                    other => return Err(mismatch(other)),
                }
                offsets.push(i32::try_from(flat.len()).map_err(|_| {
                    SqlError::Conversion(format!("list column `{}` is too long", field.name()))
                })?);
            }
            let values = build_column(inner, &flat, 0)?;
            Arc::new(ListArray::try_new(
                Arc::clone(inner),
                OffsetBuffer::new(offsets.into()),
                values,
                Some(NullBuffer::from(present)),
            )?)
        }
        other => {
            return Err(SqlError::Unsupported(format!(
                "result column `{}` of type {other}",
                field.name()
            )));
        }
    })
}

pub(super) fn checked_cast(array: &ArrayRef, field: &Field) -> SqlResult<ArrayRef> {
    let options = arrow::compute::CastOptions {
        safe: false,
        ..Default::default()
    };
    arrow::compute::cast_with_options(array.as_ref(), field.data_type(), &options).map_err(|err| {
        SqlError::Conversion(format!(
            "column `{}` does not fit {}: {err}",
            field.name(),
            field.data_type()
        ))
    })
}

pub(super) fn parse_decimal_i128(value: &str, scale: i8) -> Result<i128, ()> {
    if scale < 0 {
        return Err(());
    }
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |rest| (true, rest));
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let scale = scale as usize;
    if fraction.len() > scale && fraction[scale..].bytes().any(|byte| byte != b'0') {
        return Err(());
    }
    let mut digits = String::with_capacity(whole.len() + scale);
    digits.push_str(if whole.is_empty() { "0" } else { whole });
    digits.push_str(&fraction[..fraction.len().min(scale)]);
    digits.push_str(&"0".repeat(scale.saturating_sub(fraction.len())));
    let magnitude = digits.parse::<i128>().map_err(|_| ())?;
    if negative {
        magnitude.checked_neg().ok_or(())
    } else {
        Ok(magnitude)
    }
}
