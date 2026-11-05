//! Literals for SQL execution.

use super::*;

/// Recover static text after the unparser's quote/NUL-safe literal encoding.
/// Only literal concatenation and character constructors are recognized;
/// column references and other calls remain dynamic.
pub(super) fn literal_text(expr: &datafusion::sql::sqlparser::ast::Expr) -> Option<String> {
    use datafusion::sql::sqlparser::ast;
    match expr {
        ast::Expr::Value(value) => match &value.value {
            ast::Value::SingleQuotedString(text) => Some(text.clone()),
            _ => None,
        },
        ast::Expr::Nested(inner) | ast::Expr::Cast { expr: inner, .. } => literal_text(inner),
        ast::Expr::BinaryOp { left, op: ast::BinaryOperator::StringConcat, right } => {
            Some(literal_text(left)? + &literal_text(right)?)
        }
        ast::Expr::Function(call) => {
            let name = call.name.to_string().to_ascii_lowercase();
            let ast::FunctionArguments::List(arguments) = &call.args else { return None };
            let args = arguments.args.iter().map(|arg| match arg {
                ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(expr)) => Some(expr),
                _ => None,
            }).collect::<Option<Vec<_>>>()?;
            match name.as_str() {
                "concat" => args.into_iter().map(literal_text).collect::<Option<Vec<_>>>().map(|parts| parts.concat()),
                "chr" if args.len() == 1 => {
                    let ast::Expr::Value(value) = args[0] else { return None };
                    let ast::Value::Number(number, _) = &value.value else { return None };
                    Some(char::from_u32(number.parse().ok()?)?.to_string())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn sql_literal(dialect: SqlDialect, value: &ScalarValue) -> SqlResult<String> {
    if let SqlDialect::Custom(adapter) = dialect { return adapter.exchange_literal(value, &value.data_type()); }
    // PostgreSQL multidimensional arrays cannot preserve ragged or null child
    // lists. Use the same JSONB[] carrier as expression and exchange literals.
    if dialect == SqlDialect::Postgres && super::postgres_lists::nested(&value.data_type()) {
        return super::exchange_literal(value.clone(), value.data_type(), dialect);
    }
    if crate::ir::functions::domain::descriptor(&value.data_type()).is_some() {
        return super::exchange_literal(value.clone(), value.data_type(), dialect);
    }
    if super::structured::is_structured(&value.data_type()) { return super::structured::literal(dialect, value); }
    fn opt<T>(value: &Option<T>, render: impl Fn(&T) -> String) -> String {
        match value {
            Some(inner) => render(inner),
            None => "NULL".to_string(),
        }
    }
    Ok(match value {
        ScalarValue::Dictionary(_, value) => sql_literal(dialect, value)?,
        ScalarValue::Float16(_) => super::exchange_literal(value.clone(), value.data_type(), dialect)?,
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
        ScalarValue::Date32(_) | ScalarValue::Date64(_) | ScalarValue::Decimal32(_,_,_) | ScalarValue::Decimal64(_,_,_) | ScalarValue::Decimal128(_,_,_) | ScalarValue::Decimal256(_,_,_) | ScalarValue::TimestampSecond(_, _) | ScalarValue::TimestampMillisecond(_, _) | ScalarValue::TimestampMicrosecond(_, _) | ScalarValue::TimestampNanosecond(_, None) | ScalarValue::Time32Second(_) | ScalarValue::Time32Millisecond(_) | ScalarValue::Time64Microsecond(_) | ScalarValue::Time64Nanosecond(_) => {
            if value.is_null() { "NULL".into() } else {
                let array = value.to_array()?;
                let text = arrow::util::display::array_value_to_string(array.as_ref(), 0)?;
                format!("CAST({} AS {})", string_literal(dialect, &text)?, dialect.ddl_type(&value.data_type())?)
            }
        }
        ScalarValue::Binary(v) | ScalarValue::LargeBinary(v) | ScalarValue::BinaryView(v) | ScalarValue::FixedSizeBinary(_, v) => opt(v, |bytes| {
            let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
            if dialect == SqlDialect::Postgres { format!("decode('{hex}', 'hex')") } else { format!("from_hex('{hex}')") }
        }),
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
        cells.push(if dialect == SqlDialect::DuckDb {
            list_cell(dialect, &scalar)?
        } else {
            sql_literal(dialect, &scalar)?
        });
    }
    let joined = cells.join(", ");
    let literal = match dialect {
        SqlDialect::DuckDb => format!("[{joined}]"),
        SqlDialect::Postgres => format!("ARRAY[{joined}]"),
        SqlDialect::Custom(_) => return Err(SqlError::Unsupported("custom list literals require an adapter codec".into())),
    };
    // Expression casts and literals must share the same physical type for
    // unresolved Null leaves. DuckDB's expression mapping uses INTEGER for
    // Null, whereas fixture DDL uses VARCHAR; using DDL here makes CASE over
    // nested empty lists disagree with its typed NULL arm (VARCHAR[][] versus
    // INTEGER[][]). No concrete payload is coerced by this choice.
    let data_type = if dialect == SqlDialect::DuckDb {
        dialect.sql_type(outer.data_type())?
    } else {
        dialect.ddl_type(outer.data_type())?
    };
    Ok(format!("CAST({literal} AS {data_type})"))
}

/// The outer list cast already fixes every recursive element type. Avoid
/// redundant casts at each depth: besides SQL size, deeply nested CAST/array
/// pairs trigger the parser's reserved-word backtracking recursion limit.
/// Retain null list validity independently from an empty list's zero children.
fn list_cell(dialect: SqlDialect, value: &ScalarValue) -> SqlResult<String> {
    if value.is_null() { return Ok("NULL".into()); }
    let items = match value {
        ScalarValue::List(array) => array.value(0),
        ScalarValue::LargeList(array) => array.value(0),
        ScalarValue::FixedSizeList(array) => array.value(0),
        _ => return sql_literal(dialect, value),
    };
    let cells = (0..items.len()).map(|index| {
        list_cell(dialect, &ScalarValue::try_from_array(&items, index)?)
    }).collect::<SqlResult<Vec<_>>>()?;
    Ok(format!("[{}]", cells.join(", ")))
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
        SqlDialect::Custom(adapter) => adapter.exchange_literal(&ScalarValue::Utf8(Some(input.into())), &DataType::Utf8),
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
    } else if value == 0.0 && value.is_sign_negative() {
        format!("CAST('-0' AS {})", dialect.double_type())
    } else if value == value.trunc() && value.abs() < 1e15 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__sql__literals__tests.rs"]
mod tests;
