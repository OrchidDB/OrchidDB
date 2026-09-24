//! DuckDB type spelling for typed, unevaluated column placeholders.
use arrow::datatypes::{DataType, TimeUnit};
use datafusion::common::{DataFusionError, Result};

pub(super) fn sql_type(data_type: &DataType) -> Result<String> {
    Ok(match data_type {
        DataType::Null => "INTEGER".into(),
        DataType::Boolean => "BOOLEAN".into(),
        DataType::Int8 => "TINYINT".into(),
        DataType::Int16 => "SMALLINT".into(),
        DataType::Int32 => "INTEGER".into(),
        DataType::Int64 => "BIGINT".into(),
        DataType::UInt8 => "UTINYINT".into(),
        DataType::UInt16 => "USMALLINT".into(),
        DataType::UInt32 => "UINTEGER".into(),
        DataType::UInt64 => "UBIGINT".into(),
        DataType::Float16 | DataType::Float32 => "FLOAT".into(),
        DataType::Float64 => "DOUBLE".into(),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "VARCHAR".into(),
        DataType::Binary
        | DataType::LargeBinary
        | DataType::BinaryView
        | DataType::FixedSizeBinary(_) => "BLOB".into(),
        DataType::Date32 | DataType::Date64 => "DATE".into(),
        DataType::Time32(_) | DataType::Time64(_) => "TIME".into(),
        DataType::Timestamp(_, Some(_)) => "TIMESTAMPTZ".into(),
        DataType::Timestamp(unit, None) => match unit {
            TimeUnit::Second => "TIMESTAMP_S",
            TimeUnit::Millisecond => "TIMESTAMP_MS",
            TimeUnit::Microsecond => "TIMESTAMP",
            TimeUnit::Nanosecond => "TIMESTAMP_NS",
        }
        .into(),
        DataType::Interval(_) => "INTERVAL".into(),
        DataType::Decimal128(precision, scale) | DataType::Decimal256(precision, scale)
            if *precision <= 38 && *scale >= 0 =>
        {
            format!("DECIMAL({precision},{scale})")
        }
        DataType::List(field)
        | DataType::LargeList(field)
        | DataType::ListView(field)
        | DataType::LargeListView(field) => format!("{}[]", sql_type(field.data_type())?),
        DataType::FixedSizeList(field, len) => format!("{}[{len}]", sql_type(field.data_type())?),
        DataType::Struct(fields) => format!(
            "STRUCT({})",
            fields
                .iter()
                .map(|field| Ok(format!(
                    "\"{}\" {}",
                    field.name().replace('"', "\"\""),
                    sql_type(field.data_type())?
                )))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        DataType::Map(field, _) => {
            let DataType::Struct(fields) = field.data_type() else {
                return Err(unsupported(data_type));
            };
            if fields.len() != 2 {
                return Err(unsupported(data_type));
            }
            format!(
                "MAP({}, {})",
                sql_type(fields[0].data_type())?,
                sql_type(fields[1].data_type())?
            )
        }
        DataType::Dictionary(_, value) => sql_type(value)?,
        _ => return Err(unsupported(data_type)),
    })
}
fn unsupported(data_type: &DataType) -> DataFusionError {
    DataFusionError::Plan(format!("no DuckDB function argument type for {data_type}"))
}

/// A NULL expression with the original nested type. Unnamed DuckDB STRUCTs
/// have no round-trippable CAST type spelling, so build a type reference from
/// NULL-only constructors. cast_to_type drops that reference during binding.
pub(super) fn typed_null(data_type: &DataType) -> Result<String> {
    let reference = match data_type {
        DataType::List(field)
        | DataType::LargeList(field)
        | DataType::ListView(field)
        | DataType::LargeListView(field) => {
            format!("list_value({})", typed_null(field.data_type())?)
        }
        DataType::Struct(fields) if fields.iter().all(|f| f.name().is_empty()) => {
            format!(
                "row({})",
                fields
                    .iter()
                    .map(|f| typed_null(f.data_type()))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            )
        }
        DataType::Struct(fields) => {
            format!(
                "struct_pack({})",
                fields
                    .iter()
                    .map(|f| Ok(format!(
                        "\"{}\" := {}",
                        f.name().replace('"', "\"\""),
                        typed_null(f.data_type())?
                    )))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            )
        }
        _ => return Ok(format!("CAST(NULL AS {})", sql_type(data_type)?)),
    };
    Ok(format!("cast_to_type(NULL, {reference})"))
}
