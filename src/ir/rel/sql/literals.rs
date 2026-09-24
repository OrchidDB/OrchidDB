//! Literals for SQL execution.

use super::*;

pub(super) fn sql_literal(dialect: SqlDialect, value: &ScalarValue) -> SqlResult<String> {
    fn opt<T>(value: &Option<T>, render: impl Fn(&T) -> String) -> String {
        match value {
            Some(inner) => render(inner),
            None => "NULL".to_string(),
        }
    }
    Ok(match value {
        ScalarValue::Null => "NULL".to_string(),
        ScalarValue::Boolean(v) => opt(v, |b| if *b { "TRUE" } else { "FALSE" }.to_string()),
        ScalarValue::Int8(v) => opt(v, |i| i.to_string()),
        ScalarValue::Int16(v) => opt(v, |i| i.to_string()),
        ScalarValue::Int32(v) => opt(v, |i| i.to_string()),
        ScalarValue::Int64(v) => opt(v, |i| i.to_string()),
        ScalarValue::UInt8(v) => opt(v, |i| i.to_string()),
        ScalarValue::UInt16(v) => opt(v, |i| i.to_string()),
        ScalarValue::UInt32(v) => opt(v, |i| i.to_string()),
        ScalarValue::UInt64(v) => opt(v, |i| i.to_string()),
        ScalarValue::Float32(v) => opt(v, |f| float_literal(dialect, f64::from(*f))),
        ScalarValue::Float64(v) => opt(v, |f| float_literal(dialect, *f)),
        ScalarValue::Utf8(v) | ScalarValue::LargeUtf8(v) | ScalarValue::Utf8View(v) => match v {
            Some(text) => string_literal(dialect, text)?,
            None => "NULL".to_string(),
        },
        ScalarValue::List(array) => list_literal(dialect, array.as_ref(), array.value(0))?,
        ScalarValue::LargeList(array) => list_literal(dialect, array.as_ref(), array.value(0))?,
        ScalarValue::FixedSizeList(array) => list_literal(dialect, array.as_ref(), array.value(0))?,
        other => {
            return Err(SqlError::Unsupported(format!(
                "sql literal for {} value",
                other.data_type()
            )));
        }
    })
}

/// Render a list value. The result is always cast to the column's array type:
/// an empty list is otherwise untyped, and both engines reject that.
pub(super) fn list_literal(dialect: SqlDialect, outer: &dyn Array, items: ArrayRef) -> SqlResult<String> {
    if outer.is_null(0) {
        return Ok("NULL".to_string());
    }
    let mut cells = Vec::with_capacity(items.len());
    for index in 0..items.len() {
        let scalar = ScalarValue::try_from_array(&items, index)?;
        cells.push(sql_literal(dialect, &scalar)?);
    }
    let joined = cells.join(", ");
    let literal = match dialect {
        SqlDialect::DuckDb => format!("[{joined}]"),
        SqlDialect::Postgres => format!("ARRAY[{joined}]"),
    };
    Ok(format!(
        "CAST({literal} AS {})",
        dialect.ddl_type(outer.data_type())?
    ))
}

/// Render a string literal, including one containing a NUL character.
///
/// NUL is legal in graph string properties but cannot appear verbatim in
/// statement text: engine client APIs take NUL-terminated strings, so the
/// statement would be silently truncated or rejected.
pub(super) fn string_literal(dialect: SqlDialect, input: &str) -> SqlResult<String> {
    let quote = |part: &str| format!("'{}'", part.replace('\'', "''"));
    if !input.contains('\0') {
        return Ok(quote(input));
    }
    match dialect {
        SqlDialect::DuckDb => Ok(input
            .split('\0')
            .map(quote)
            .collect::<Vec<_>>()
            .join(" || chr(0) || ")),
        // Postgres `text` cannot represent the NUL code point at all, so
        // there is no encoding to fall back to.
        SqlDialect::Postgres => Err(SqlError::Unsupported(
            "postgres text cannot contain a NUL character".to_string(),
        )),
    }
}

pub(super) fn float_literal(dialect: SqlDialect, value: f64) -> String {
    if value.is_nan() {
        format!("CAST('NaN' AS {})", dialect.double_type())
    } else if value == f64::INFINITY {
        format!("CAST('Infinity' AS {})", dialect.double_type())
    } else if value == f64::NEG_INFINITY {
        format!("CAST('-Infinity' AS {})", dialect.double_type())
    } else if value == value.trunc() && value.abs() < 1e15 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}
