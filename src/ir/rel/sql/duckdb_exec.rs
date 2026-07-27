//! DuckDB implementation of [`SqlExecutor`].
//!
//! Every `run` opens a fresh in-memory database, applies the setup script,
//! and executes the query, so cases never observe each other's tables.

use duckdb::Connection;
use duckdb::types::{ListType, ValueRef};

use super::{SqlDialect, SqlError, SqlExecutor, SqlResult, SqlValue, sql_value_from_array};

#[derive(Debug, Clone, Copy, Default)]
pub struct DuckDbExecutor;

impl DuckDbExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl SqlExecutor for DuckDbExecutor {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }

    fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>> {
        let conn = Connection::open_in_memory()
            .map_err(|err| SqlError::Setup(format!("duckdb open: {err}")))?;
        for statement in setup {
            conn.execute_batch(statement).map_err(|err| {
                SqlError::Setup(format!("duckdb setup: {err}\nstatement: {statement}"))
            })?;
        }
        let mut statement = conn
            .prepare(query)
            .map_err(|err| SqlError::Execution(format!("duckdb prepare: {err}")))?;
        let mut rows = statement
            .query([])
            .map_err(|err| SqlError::Execution(format!("duckdb query: {err}")))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|err| SqlError::Execution(format!("duckdb row: {err}")))?
        {
            let stmt: &duckdb::Statement<'_> = row.as_ref();
            let width = stmt.column_count();
            let mut cells = Vec::with_capacity(width);
            for index in 0..width {
                let value = row
                    .get_ref(index)
                    .map_err(|err| SqlError::Execution(format!("duckdb cell {index}: {err}")))?;
                cells.push(convert_value(value)?);
            }
            out.push(cells);
        }
        Ok(out)
    }
}

fn convert_value(value: ValueRef<'_>) -> SqlResult<SqlValue> {
    Ok(match value {
        ValueRef::Null => SqlValue::Null,
        ValueRef::Boolean(value) => SqlValue::Bool(value),
        ValueRef::TinyInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::SmallInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::Int(value) => SqlValue::Int(i64::from(value)),
        ValueRef::BigInt(value) => SqlValue::Int(value),
        ValueRef::HugeInt(value) => SqlValue::Int(i64::try_from(value).map_err(|_| {
            SqlError::Conversion(format!("duckdb HUGEINT {value} overflows i64"))
        })?),
        ValueRef::UTinyInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::USmallInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::UInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::UBigInt(value) => SqlValue::Int(i64::try_from(value).map_err(|_| {
            SqlError::Conversion(format!("duckdb UBIGINT {value} overflows i64"))
        })?),
        ValueRef::Float(value) => SqlValue::Float(f64::from(value)),
        ValueRef::Double(value) => SqlValue::Float(value),
        ValueRef::Decimal(value) => SqlValue::Float(
            value
                .to_string()
                .parse::<f64>()
                .map_err(|err| SqlError::Conversion(format!("duckdb decimal {value}: {err}")))?,
        ),
        ValueRef::Text(bytes) => SqlValue::Text(String::from_utf8_lossy(bytes).into_owned()),
        // List-valued graph properties come back as one Arrow list per row;
        // slice out this row's elements and convert them the same way.
        ValueRef::List(list, row) => {
            let (values, offset, length) = match list {
                ListType::Regular(array) => (
                    array.values(),
                    array.value_offsets()[row] as usize,
                    array.value_length(row) as usize,
                ),
                ListType::Large(array) => (
                    array.values(),
                    array.value_offsets()[row] as usize,
                    array.value_length(row) as usize,
                ),
            };
            let mut items = Vec::with_capacity(length);
            for index in offset..offset + length {
                items.push(sql_value_from_array(values.as_ref(), index)?);
            }
            SqlValue::List(items)
        }
        other => {
            return Err(SqlError::Unsupported(format!(
                "duckdb value type {other:?}"
            )));
        }
    })
}
