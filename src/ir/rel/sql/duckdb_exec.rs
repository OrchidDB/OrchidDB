//! DuckDB implementation of [`SqlExecutor`].
//!
//! An executor owns one in-memory or file-backed database session. Setup statements are applied once
//! per executor, allowing successive islands over the same graph to reuse
//! materialized tables and session state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::RecordBatch;
use arrow::datatypes::{Field, Schema};
use duckdb::Connection;
use duckdb::types::{FromSql, ListType, ValueRef};
use duckdb::vtab::arrow::ArrowVTab;
use duckdb::vtab::arrow_recordbatch_to_query_params;

use super::{
    SqlDialect, SqlError, SqlExecutor, SqlResult, SqlValue, TableData, create_table_sql,
    sql_value_from_array, table_setup_sql,
};

/// Name under which the Arrow table function is registered on a session.
const ARROW_SCAN_FUNCTION: &str = "__graph_arrow_scan";

/// Rows per Arrow hand-off. duckdb-rs's Arrow table function emits a record
/// batch as a single data chunk, so a batch must fit DuckDB's vector size.
const ARROW_SLICE_ROWS: usize = 2048;

/// Default budget for cached scan tables before least-recently-used ones are
/// dropped. Override with `GRAPH_DUCKDB_SCAN_CACHE_BYTES`.
const DEFAULT_SCAN_CACHE_BYTES: usize = 1 << 30;

#[derive(Debug, Clone, Copy)]
struct ScanTableEntry {
    bytes: usize,
    last_used: u64,
}

#[derive(Debug, Default)]
pub struct DuckDbExecutor {
    connection: Option<Connection>,
    database: Option<super::SharedDatabase>,
    applied_setup: BTreeMap<String, Vec<String>>,
    /// Content-addressed scan tables loaded into the session, by
    /// [`TableData::content_key`].
    scan_tables: BTreeMap<String, ScanTableEntry>,
    /// Per-query scan names currently defined as views, and the content
    /// table each one aliases plus the view's column names. Two queries can
    /// reuse one scan name over identical content with different binding
    /// column names, so the column list is part of the view identity.
    scan_views: BTreeMap<String, (String, Vec<String>)>,
    scan_clock: u64,
    arrow_registered: bool,
    timeout: Option<Duration>,
    setup_timeout: Option<Duration>,
    lost_connection: bool,
    transaction_active: bool,
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

    /// Configure time budgets without replacing an existing database session.
    pub fn set_timeouts(&mut self, query_timeout: Duration, setup_timeout: Duration) {
        self.timeout = Some(query_timeout);
        self.setup_timeout = Some(setup_timeout);
    }

    /// Open a file-backed DuckDB database. The connection persists across
    /// executor calls and, because it is backed by a file, across executor
    /// instances that reopen the same path.
    pub fn open(path: impl AsRef<Path>) -> SqlResult<Self> {
        let (connection, database) = super::open_shared(path.as_ref())
            .map_err(|err| SqlError::Setup(format!("duckdb open: {err}")))?;
        let mut executor = Self::from_connection(connection);
        executor.database = database;
        Ok(executor)
    }

    /// Wrap an already-open DuckDB connection.
    pub fn from_connection(connection: Connection) -> Self {
        let transaction_active = detect_transaction(&connection).unwrap_or(true);
        Self {
            connection: Some(connection),
            database: None,
            applied_setup: BTreeMap::new(),
            scan_tables: BTreeMap::new(),
            scan_views: BTreeMap::new(),
            scan_clock: 0,
            arrow_registered: false,
            timeout: None,
            setup_timeout: None,
            lost_connection: false,
            transaction_active,
        }
    }

    /// Return the underlying connection, opening an in-memory database on
    /// first use.
    pub fn connection(&mut self) -> SqlResult<&Connection> {
        self.ensure_connection()?;
        // Connection has interior mutability: callers can execute arbitrary
        // SQL, including rollback or replacing materialized tables.
        self.invalidate_setup();
        Ok(self.connection.as_ref().expect("connection initialized"))
    }

    fn ensure_connection(&mut self) -> SqlResult<()> {
        if self.connection.is_none() {
            if self.lost_connection {
                return Err(SqlError::Execution(
                    "DuckDB session was lost after interruption; open a new executor".into(),
                ));
            }
            self.arrow_registered = false;
            self.connection = Some(
                Connection::open_in_memory()
                    .map_err(|err| SqlError::Setup(format!("duckdb open: {err}")))?,
            );
        }
        Ok(())
    }

    /// Execute a raw SQL batch. Arbitrary SQL can rewrite materialized tables,
    /// so this invalidates the applied-setup cache.
    pub fn execute_batch(&mut self, sql: &str) -> SqlResult<()> {
        self.invalidate_setup();
        let conn = self.connection()?;
        let result = conn
            .execute_batch(sql)
            .map_err(|err| SqlError::Execution(format!("duckdb execute_batch: {err}")));
        if let Some(active) = detect_transaction(conn) {
            self.transaction_active = active;
        }
        result
    }

    /// Begin an explicit transaction.
    pub fn begin(&mut self) -> SqlResult<()> {
        if self.in_transaction() {
            return Err(SqlError::Setup(
                "duckdb begin: a transaction is already active".into(),
            ));
        }
        let conn = self.connection()?;
        conn.execute_batch("BEGIN TRANSACTION")
            .map_err(|err| SqlError::Setup(format!("duckdb begin: {err}")))?;
        self.transaction_active = true;
        Ok(())
    }

    /// Commit the active transaction.
    pub fn commit(&mut self) -> SqlResult<()> {
        if !self.in_transaction() {
            return Err(SqlError::Setup(
                "duckdb commit: no active transaction".into(),
            ));
        }
        let conn = self.connection()?;
        conn.execute_batch("COMMIT")
            .map_err(|err| SqlError::Setup(format!("duckdb commit: {err}")))?;
        self.transaction_active = false;
        Ok(())
    }

    /// Roll back the active transaction. Setup applied inside the rolled-back
    /// transaction is undone, so its cache entries are discarded as well.
    pub fn rollback(&mut self) -> SqlResult<()> {
        if !self.in_transaction() {
            return Err(SqlError::Setup(
                "duckdb rollback: no active transaction".into(),
            ));
        }
        let conn = self.connection()?;
        conn.execute_batch("ROLLBACK")
            .map_err(|err| SqlError::Setup(format!("duckdb rollback: {err}")))?;
        self.transaction_active = false;
        self.invalidate_setup();
        Ok(())
    }

    /// Forget everything this executor believes it has materialized. Called
    /// whenever the session may have changed underneath the caches.
    fn invalidate_setup(&mut self) {
        self.applied_setup.clear();
        self.scan_tables.clear();
        self.scan_views.clear();
    }

    /// Load `table` once under its content key and alias it under the name
    /// the query uses. Returns the content key.
    fn ensure_scan_table(&mut self, table: &TableData, cache: bool) -> SqlResult<String> {
        let key = table.content_key();
        self.scan_clock += 1;
        let conn = self.connection.as_ref().expect("connection initialized");
        match self.scan_tables.get_mut(&key) {
            Some(entry) if cache => entry.last_used = self.scan_clock,
            _ => {
                if !self.arrow_registered {
                    conn.register_table_function::<ArrowVTab>(ARROW_SCAN_FUNCTION)
                        .map_err(|err| SqlError::Setup(format!("duckdb arrow scan: {err}")))?;
                    self.arrow_registered = true;
                }
                load_scan_table(conn, &key, table, cache)?;
                if cache {
                    self.scan_tables.insert(
                        key.clone(),
                        ScanTableEntry {
                            bytes: table.byte_size(),
                            last_used: self.scan_clock,
                        },
                    );
                }
            }
        }
        let view_columns = table
            .schema
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect::<Vec<_>>();
        let view_identity = (key.clone(), view_columns);
        if !cache || self.scan_views.get(&table.name) != Some(&view_identity) {
            let dialect = SqlDialect::DuckDb;
            let columns = table
                .schema
                .fields()
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    format!(
                        "{} AS {}",
                        dialect.quote_ident(&scan_column(index)),
                        dialect.quote_ident(field.name())
                    )
                })
                .collect::<Vec<_>>();
            conn.execute_batch(&format!(
                "CREATE OR REPLACE TEMP VIEW {} AS SELECT {} FROM {}",
                dialect.quote_ident(&table.name),
                columns.join(", "),
                dialect.quote_ident(&key)
            ))
            .map_err(|err| SqlError::Setup(format!("duckdb scan view: {err}")))?;
            if cache {
                self.scan_views.insert(table.name.clone(), view_identity);
            }
        }
        Ok(key)
    }

    /// Drop least-recently-used scan tables beyond the byte budget, never
    /// touching the ones the current query uses.
    fn evict_scan_tables(&mut self, in_use: &BTreeSet<String>) {
        let budget = std::env::var("GRAPH_DUCKDB_SCAN_CACHE_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_SCAN_CACHE_BYTES);
        let mut total: usize = self.scan_tables.values().map(|entry| entry.bytes).sum();
        if total <= budget {
            return;
        }
        let mut candidates = self
            .scan_tables
            .iter()
            .filter(|(key, _)| !in_use.contains(*key))
            .map(|(key, entry)| (entry.last_used, key.clone(), entry.bytes))
            .collect::<Vec<_>>();
        candidates.sort();
        let Some(conn) = self.connection.as_ref() else {
            return;
        };
        for (_, key, bytes) in candidates {
            if total <= budget {
                break;
            }
            let dropped = conn
                .execute_batch(&format!(
                    "DROP TABLE IF EXISTS {}",
                    SqlDialect::DuckDb.quote_ident(&key)
                ))
                .is_ok();
            if !dropped {
                continue;
            }
            self.scan_tables.remove(&key);
            self.scan_views.retain(|_, (target, _)| *target != key);
            total -= bytes;
        }
    }

    pub fn in_transaction(&self) -> bool {
        self.connection
            .as_ref()
            .and_then(detect_transaction)
            .unwrap_or(self.transaction_active)
    }
}

impl SqlExecutor for DuckDbExecutor {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }

    fn run_with_tables(
        &mut self,
        tables: &[TableData],
        setup: &[String],
        query: &str,
    ) -> SqlResult<Vec<Vec<SqlValue>>> {
        if tables.is_empty() {
            return self.run(setup, query);
        }
        self.ensure_connection()?;
        // Tables created inside a caller's transaction vanish on rollback,
        // so they are only remembered when they commit immediately.
        let cache = !self.in_transaction();
        let mut in_use = BTreeSet::new();
        for table in tables {
            in_use.insert(self.ensure_scan_table(table, cache)?);
        }
        let result = self.run(setup, query);
        if cache {
            self.evict_scan_tables(&in_use);
        }
        result
    }

    fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>> {
        self.ensure_connection()?;
        let conn = self.connection.as_ref().expect("connection initialized");
        let wrap_setup = !self.in_transaction();
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
                execute_setup_block(conn, &block, wrap_setup)?;
                self.applied_setup.insert(key, block);
            }
            return execute_query(conn, query);
        };
        let setup_timeout = self.setup_timeout.unwrap_or(query_timeout);
        let conn = self.connection.take().expect("connection initialized");
        self.lost_connection = true;
        let interrupt = conn.interrupt_handle();
        let query = query.to_string();
        let (sender, receiver) = std::sync::mpsc::channel();
        let (setup_sender, setup_receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut completed = Vec::new();
            for (key, block) in pending {
                if let Err(err) = execute_setup_block(&conn, &block, wrap_setup) {
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
                            self.lost_connection = false;
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
                            self.invalidate_setup();
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
                    Ok((conn, _, _)) => {
                        // Keep the user's session and transaction available
                        // for recovery. Never replace a file-backed database
                        // with an empty in-memory connection after a timeout.
                        self.connection = Some(conn);
                        self.lost_connection = false;
                        self.invalidate_setup();
                        let _ = worker.join();
                        return Err(SqlError::Execution(format!(
                            "duckdb setup timed out after {}ms",
                            setup_timeout.as_millis()
                        )));
                    }
                    Err(_) => {
                        self.invalidate_setup();
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
        self.lost_connection = false;
        for (key, block) in completed {
            self.applied_setup.insert(key, block);
        }
        let _ = worker.join();
        result
    }
}

/// Apply one setup block, wrapping it in its own transaction so a statement
/// that fails partway through does not leave a half-materialized table behind.
/// When the executor is already inside an explicit transaction (`atomic` is
/// false) the statements join that transaction instead and are undone or
/// committed with it.
fn execute_setup_block(conn: &Connection, block: &[String], atomic: bool) -> SqlResult<()> {
    if atomic {
        conn.execute_batch("BEGIN TRANSACTION")
            .map_err(|err| SqlError::Setup(format!("duckdb setup begin: {err}")))?;
    }
    for statement in block {
        if let Err(err) = conn.execute_batch(statement) {
            if atomic {
                let _ = conn.execute_batch("ROLLBACK");
            }
            return Err(SqlError::Setup(format!(
                "duckdb setup: {err}\nstatement: {statement}"
            )));
        }
    }
    if atomic {
        conn.execute_batch("COMMIT")
            .map_err(|err| SqlError::Setup(format!("duckdb setup commit: {err}")))?;
    }
    Ok(())
}

fn scan_column(index: usize) -> String {
    format!("c{index}")
}

/// Create `key` with the table's DDL types and fill it from Arrow. Rows go
/// through DuckDB's Arrow table function and an `INSERT ... SELECT`, so values
/// are cast to the declared column types exactly as literal inserts were.
/// A table whose Arrow types the scan cannot carry falls back to literal
/// inserts. Either way the load is atomic unless it joins a caller's
/// transaction.
fn load_scan_table(conn: &Connection, key: &str, table: &TableData, atomic: bool) -> SqlResult<()> {
    let fields = table
        .schema
        .fields()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            Arc::new(Field::new(
                scan_column(index),
                field.data_type().clone(),
                true,
            ))
        })
        .collect::<Vec<_>>();
    let schema = Arc::new(Schema::new(fields));
    let batches = table
        .batches
        .iter()
        .map(|batch| RecordBatch::try_new(schema.clone(), batch.columns().to_vec()))
        .collect::<Result<Vec<_>, _>>()?;
    let renamed = TableData {
        name: key.to_string(),
        schema,
        batches,
    };
    let quoted = SqlDialect::DuckDb.quote_ident(key);

    let arrow_load = |conn: &Connection| -> Result<(), String> {
        conn.execute_batch(
            &create_table_sql(SqlDialect::DuckDb, &renamed).map_err(|e| e.to_string())?,
        )
        .map_err(|err| err.to_string())?;
        let insert = format!("INSERT INTO {quoted} SELECT * FROM {ARROW_SCAN_FUNCTION}(?, ?)");
        for batch in &renamed.batches {
            let mut offset = 0;
            while offset < batch.num_rows() {
                let len = ARROW_SLICE_ROWS.min(batch.num_rows() - offset);
                let params = arrow_recordbatch_to_query_params(batch.slice(offset, len));
                conn.execute(&insert, params)
                    .map_err(|err| err.to_string())?;
                offset += len;
            }
        }
        Ok(())
    };

    if atomic {
        conn.execute_batch("BEGIN TRANSACTION")
            .map_err(|err| SqlError::Setup(format!("duckdb scan load begin: {err}")))?;
    }
    let loaded = match arrow_load(conn) {
        Ok(()) => Ok(()),
        Err(arrow_err) => {
            // Undo the partial Arrow load before retrying with literal rows.
            let retry = if atomic {
                conn.execute_batch("ROLLBACK; BEGIN TRANSACTION")
                    .map_err(|err| format!("{arrow_err}; rollback: {err}"))
            } else {
                Ok(())
            };
            retry.and_then(|()| {
                let statements =
                    table_setup_sql(SqlDialect::DuckDb, &renamed).map_err(|e| e.to_string())?;
                for statement in statements {
                    conn.execute_batch(&statement)
                        .map_err(|err| format!("{err} (arrow load: {arrow_err})"))?;
                }
                Ok(())
            })
        }
    };
    match loaded {
        Ok(()) => {
            if atomic {
                conn.execute_batch("COMMIT")
                    .map_err(|err| SqlError::Setup(format!("duckdb scan load commit: {err}")))?;
            }
            Ok(())
        }
        Err(err) => {
            if atomic {
                let _ = conn.execute_batch("ROLLBACK");
            }
            Err(SqlError::Setup(format!(
                "duckdb scan load `{}`: {err}",
                table.name
            )))
        }
    }
}

/// duckdb-rs 1.10502's `Connection::is_autocommit` is an unconditional `true`
/// stub. Two queries have the same transaction start ID only inside an
/// explicit transaction. Failed/aborted sessions retain the last known state.
fn detect_transaction(conn: &Connection) -> Option<bool> {
    let first: u64 = conn
        .query_row("SELECT txid_current()", [], |row| row.get(0))
        .ok()?;
    let second: u64 = conn
        .query_row("SELECT txid_current()", [], |row| row.get(0))
        .ok()?;
    Some(first == second)
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
            // List offsets are also present for null rows. Reading only the
            // offsets turns SQL NULL into [], losing Cypher's null semantics.
            use arrow::array::Array;
            if match &list {
                ListType::Regular(array) => array.is_null(row),
                ListType::Large(array) => array.is_null(row),
            } {
                return Ok(SqlValue::Null);
            }
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
