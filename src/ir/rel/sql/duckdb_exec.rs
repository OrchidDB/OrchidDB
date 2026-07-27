//! DuckDB implementation of [`SqlExecutor`].
//!
//! An executor owns one in-memory database. Setup statements are applied once
//! per executor, allowing successive islands over the same graph to reuse
//! materialized tables and session state.

use std::collections::BTreeMap;

use duckdb::Connection;
use duckdb::types::{ListType, ValueRef};

use super::{SqlDialect, SqlError, SqlExecutor, SqlResult, SqlValue, sql_value_from_array};

#[derive(Debug, Default)]
pub struct DuckDbExecutor {
    connection: Option<Connection>,
    applied_setup: BTreeMap<String, Vec<String>>,
}

impl DuckDbExecutor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SqlExecutor for DuckDbExecutor {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }

    fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>> {
        if self.connection.is_none() {
            self.connection = Some(
                Connection::open_in_memory()
                    .map_err(|err| SqlError::Setup(format!("duckdb open: {err}")))?,
            );
        }
        let conn = self.connection.as_ref().expect("connection initialized");
        let mut blocks: Vec<Vec<String>> = Vec::new();
        for statement in setup {
            if statement.starts_with("CREATE") {
                blocks.push(Vec::new());
            }
            if blocks.is_empty() {
                blocks.push(Vec::new());
            }
            blocks
                .last_mut()
                .expect("setup block")
                .push(statement.clone());
        }
        for block in blocks {
            let key = setup_table_key(&block[0]);
            if self.applied_setup.get(&key) == Some(&block) {
                continue;
            }
            for statement in &block {
                conn.execute_batch(statement).map_err(|err| {
                    SqlError::Setup(format!("duckdb setup: {err}\nstatement: {statement}"))
                })?;
            }
            self.applied_setup.insert(key, block);
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

fn setup_table_key(statement: &str) -> String {
    let Some(start) = statement.find('"') else {
        return statement.to_string();
    };
    let rest = &statement[start + 1..];
    let Some(end) = rest.find('"') else {
        return statement.to_string();
    };
    rest[..end].replace("\"\"", "\"")
}

fn convert_value(value: ValueRef<'_>) -> SqlResult<SqlValue> {
    Ok(match value {
        ValueRef::Null => SqlValue::Null,
        ValueRef::Boolean(value) => SqlValue::Bool(value),
        ValueRef::TinyInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::SmallInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::Int(value) => SqlValue::Int(i64::from(value)),
        ValueRef::BigInt(value) => SqlValue::Int(value),
        ValueRef::HugeInt(value) => SqlValue::ExactNumber(value.to_string()),
        ValueRef::UTinyInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::USmallInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::UInt(value) => SqlValue::Int(i64::from(value)),
        ValueRef::UBigInt(value) => SqlValue::ExactNumber(value.to_string()),
        ValueRef::Float(value) => SqlValue::Float(f64::from(value)),
        ValueRef::Double(value) => SqlValue::Float(value),
        ValueRef::Decimal(value) => SqlValue::ExactNumber(value.to_string()),
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
