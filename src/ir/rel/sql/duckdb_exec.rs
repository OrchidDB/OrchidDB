//! DuckDB implementation of [`SqlExecutor`].
//!
//! An executor owns one in-memory database. Setup statements are applied once
//! per executor, allowing successive islands over the same graph to reuse
//! materialized tables and session state.

use std::collections::BTreeMap;
use std::time::Duration;

use duckdb::Connection;
use duckdb::types::{FromSql, ListType, ValueRef};

use super::{SqlDialect, SqlError, SqlExecutor, SqlResult, SqlValue, sql_value_from_array};

#[derive(Debug, Default)]
pub struct DuckDbExecutor {
    connection: Option<Connection>,
    applied_setup: BTreeMap<String, Vec<String>>,
    timeout: Option<Duration>,
    setup_timeout: Option<Duration>,
}

impl DuckDbExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an executor that interrupts DuckDB when setup plus query
    /// execution exceeds the supplied wall-clock budget. DuckDB is
    /// synchronous, so the interrupt handle is driven from a supervisor.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            setup_timeout: Some(timeout),
            ..Self::default()
        }
    }

    /// Create an executor with independent query and fixture-setup budgets.
    /// Corpus fixtures can be expensive to materialize once, while generated
    /// queries should still be held to a much smaller execution budget.
    pub fn with_timeouts(query_timeout: Duration, setup_timeout: Duration) -> Self {
        Self {
            timeout: Some(query_timeout),
            setup_timeout: Some(setup_timeout),
            ..Self::default()
        }
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
        let mut pending = Vec::new();
        for block in blocks {
            let key = setup_table_key(&block[0]);
            if self.applied_setup.get(&key) == Some(&block) {
                continue;
            }
            pending.push((key, block));
        }
        let Some(query_timeout) = self.timeout else {
            for (key, block) in pending {
                execute_setup_block(conn, &block)?;
                self.applied_setup.insert(key, block);
            }
            return execute_query(conn, query);
        };
        let setup_timeout = self.setup_timeout.unwrap_or(query_timeout);
        let conn = self.connection.take().expect("connection initialized");
        let interrupt = conn.interrupt_handle();
        let query = query.to_string();
        let (sender, receiver) = std::sync::mpsc::channel();
        let (setup_sender, setup_receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut completed = Vec::new();
            for (key, block) in pending {
                if let Err(err) = execute_setup_block(&conn, &block) {
                    let _ = sender.send((conn, Err(err), completed));
                    return;
                }
                completed.push((key, block));
            }
            let _ = setup_sender.send(());
            let result = execute_query(&conn, &query);
            let _ = sender.send((conn, result, completed));
        });
        let received = match setup_receiver.recv_timeout(setup_timeout) {
            Ok(()) => match receiver.recv_timeout(query_timeout) {
                Ok(received) => received,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    interrupt.interrupt();
                    match receiver.recv_timeout(Duration::from_secs(2)) {
                        Ok((conn, _, completed)) => {
                            // Query interruption does not invalidate completed
                            // setup. Keep the session so the next case does not
                            // rematerialize the same fixture.
                            self.connection = Some(conn);
                            for (key, block) in completed {
                                self.applied_setup.insert(key, block);
                            }
                            let _ = worker.join();
                            return Err(SqlError::Execution(format!(
                                "duckdb query timed out after {}ms",
                                query_timeout.as_millis()
                            )));
                        }
                        Err(_) => {
                            self.applied_setup.clear();
                            return Err(SqlError::Execution(format!(
                                "duckdb query did not stop after interrupt at {}ms",
                                query_timeout.as_millis()
                            )));
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(SqlError::Execution(
                        "duckdb query worker disconnected".into(),
                    ));
                }
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                interrupt.interrupt();
                match receiver.recv_timeout(Duration::from_secs(2)) {
                    Ok((_conn, _, _)) => {
                        // Start clean after interruption. A setup statement
                        // may have been applied only partially, and retaining
                        // that connection would make cache state ambiguous.
                        self.applied_setup.clear();
                        let _ = worker.join();
                        return Err(SqlError::Execution(format!(
                            "duckdb setup timed out after {}ms",
                            setup_timeout.as_millis()
                        )));
                    }
                    Err(_) => {
                        self.applied_setup.clear();
                        return Err(SqlError::Execution(format!(
                            "duckdb setup did not stop after interrupt at {}ms",
                            setup_timeout.as_millis()
                        )));
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => receiver
                .recv()
                .map_err(|_| SqlError::Execution("duckdb setup worker disconnected".into()))?,
        };
        let (conn, result, completed) = received;
        self.connection = Some(conn);
        for (key, block) in completed {
            self.applied_setup.insert(key, block);
        }
        let _ = worker.join();
        result
    }
}

fn execute_setup_block(conn: &Connection, block: &[String]) -> SqlResult<()> {
    for statement in block {
        conn.execute_batch(statement).map_err(|err| {
            SqlError::Setup(format!("duckdb setup: {err}\nstatement: {statement}"))
        })?;
    }
    Ok(())
}

fn execute_query(conn: &Connection, query: &str) -> SqlResult<Vec<Vec<SqlValue>>> {
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
        temporal @ (ValueRef::Timestamp(..) | ValueRef::Date32(_) | ValueRef::Time64(..)) => {
            SqlValue::Text(
                String::column_result(temporal)
                    .map_err(|err| SqlError::Conversion(format!("duckdb temporal value: {err}")))?,
            )
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupts_a_long_running_query() {
        let started = std::time::Instant::now();
        let mut executor = DuckDbExecutor::with_timeout(Duration::from_millis(25));
        assert_eq!(
            executor
                .run(
                    &[
                        "CREATE TABLE retained_fixture(value INTEGER)".into(),
                        "INSERT INTO retained_fixture VALUES (42)".into()
                    ],
                    "SELECT value FROM retained_fixture",
                )
                .unwrap(),
            vec![vec![SqlValue::Int(42)]]
        );
        let error = executor
            .run(
                &[],
                "SELECT SUM(a.i * b.i) FROM range(1000000) a(i), range(1000000) b(i)",
            )
            .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            executor
                .run(&[], "SELECT value FROM retained_fixture")
                .unwrap(),
            vec![vec![SqlValue::Int(42)]]
        );
    }

    #[test]
    fn interrupts_long_running_setup_and_resets_the_session() {
        let started = std::time::Instant::now();
        let mut executor = DuckDbExecutor::with_timeout(Duration::from_millis(25));
        let error = executor
            .run(
                &[
                    "CREATE TABLE oversized_setup AS SELECT i FROM range(1000000000) values_(i)"
                        .into(),
                ],
                "SELECT COUNT(*) FROM oversized_setup",
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("setup timed out"),
            "unexpected timeout phase: {error}"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            executor.run(&[], "SELECT 1").unwrap(),
            vec![vec![SqlValue::Int(1)]]
        );
    }

    #[test]
    fn returns_temporal_values_as_canonical_text() {
        let mut executor = DuckDbExecutor::new();
        assert_eq!(
            executor
                .run(
                    &[],
                    "SELECT CAST('2024-06-15 12:34:56.123456' AS TIMESTAMP), CAST('2024-06-15' AS DATE)",
                )
                .unwrap(),
            vec![vec![
                SqlValue::Text("2024-06-15 12:34:56.123456".into()),
                SqlValue::Text("2024-06-15".into()),
            ]]
        );
    }
}
