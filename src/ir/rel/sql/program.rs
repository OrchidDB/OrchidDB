//! Query-local, staged DuckDB execution.
//!
//! A program owns temporary work tables and may insert, clear, and inspect
//! them while DuckDB performs the relational work. The host only schedules
//! stages and reads an empty/nonempty termination flag. Programs run in their
//! own transaction so a failed stage rolls back every work table. A caller's
//! existing transaction is rejected until savepoint ownership is defined.
//!
//! Stage SELECT text must come from a trusted read-only SQL compiler. This
//! runner does not parse SQL or prove source-table purity. Each stage uses the
//! executor's query timeout, if configured. The optional program deadline is
//! checked between executor calls; it cannot interrupt an in-flight DuckDB
//! call. If DuckDB cannot return a connection after an interrupt, rollback and
//! cleanup cannot be guaranteed on that lost session.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::{DuckDbExecutor, SqlError, SqlExecutor, SqlResult, SqlValue};

static NEXT_PROGRAM: AtomicU64 = AtomicU64::new(1);

/// DuckDB type of a program work column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkType {
    Boolean,
    BigInt,
    Double,
    Varchar,
}

impl WorkType {
    fn sql(self) -> &'static str {
        match self {
            Self::Boolean => "BOOLEAN",
            Self::BigInt => "BIGINT",
            Self::Double => "DOUBLE",
            Self::Varchar => "VARCHAR",
        }
    }
}

/// Column contract for a DuckDB temporary work relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkColumn {
    pub name: String,
    pub data_type: WorkType,
    pub nullable: bool,
}

impl WorkColumn {
    pub fn new(name: impl Into<String>, data_type: WorkType, nullable: bool) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
        }
    }
}

/// Opaque identity and schema for one temporary table. Its SQL name is unique
/// to this program instance, even when several programs share one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkTable {
    sql_name: String,
    columns: Vec<WorkColumn>,
}

impl WorkTable {
    pub fn sql_name(&self) -> &str {
        &self.sql_name
    }

    pub fn columns(&self) -> &[WorkColumn] {
        &self.columns
    }
}

/// One DuckDB work-table write. `select_sql` is a trusted, generated SELECT
/// query; the runner controls the target table and write operation.
#[derive(Debug, Clone)]
pub enum WorkStage {
    Clear(WorkTable),
    InsertSelect { into: WorkTable, select_sql: String },
}

impl WorkStage {
    pub fn insert(into: &WorkTable, select_sql: impl Into<String>) -> Self {
        Self::InsertSelect {
            into: into.clone(),
            select_sql: select_sql.into(),
        }
    }

    pub fn clear(table: &WorkTable) -> Self {
        Self::Clear(table.clone())
    }
}

/// A sequence of writes, or a bounded loop that stops once `until_empty`
/// produces no rows. Exhausting the bound is an error, never partial success.
#[derive(Debug, Clone)]
pub enum ProgramStage {
    Write(WorkStage),
    RepeatUntilEmpty {
        steps: Vec<WorkStage>,
        until_empty: String,
        max_iterations: usize,
    },
}

/// One read-only graph query implemented as a DuckDB program.
#[derive(Debug, Clone)]
pub struct DuckDbProgram {
    id: u64,
    tables: Vec<WorkTable>,
    stages: Vec<ProgramStage>,
    result_sql: String,
    timeout: Option<Duration>,
}

impl DuckDbProgram {
    pub fn new(result_sql: impl Into<String>) -> Self {
        Self {
            id: NEXT_PROGRAM.fetch_add(1, Ordering::Relaxed),
            tables: Vec::new(),
            stages: Vec::new(),
            result_sql: result_sql.into(),
            timeout: None,
        }
    }

    pub fn add_work_table(&mut self, columns: Vec<WorkColumn>) -> SqlResult<WorkTable> {
        if columns.is_empty() {
            return Err(SqlError::Unsupported(
                "program work table has no columns".into(),
            ));
        }
        let mut names = BTreeSet::new();
        for column in &columns {
            if column.name.is_empty() || !names.insert(column.name.clone()) {
                return Err(SqlError::Unsupported(format!(
                    "invalid or duplicate program work column `{}`",
                    column.name
                )));
            }
        }
        let table = WorkTable {
            sql_name: format!("\"__graph_program_{}_{}\"", self.id, self.tables.len()),
            columns,
        };
        self.tables.push(table.clone());
        Ok(table)
    }

    pub fn push(&mut self, stage: ProgramStage) {
        self.stages.push(stage);
    }

    pub fn set_result_sql(&mut self, sql: impl Into<String>) {
        self.result_sql = sql.into();
    }

    /// Set a whole-program wall-clock budget. The budget covers setup, every
    /// stage and loop iteration, result retrieval, and work-table cleanup.
    /// Expiration is observed between DuckDB calls; an in-flight call relies
    /// on the executor's per-query timeout and is not interrupted by this.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    /// Execute on the executor's persistent DuckDB connection. All work
    /// relations are dropped before commit; failure rolls back their creation.
    pub fn run(&self, executor: &mut DuckDbExecutor) -> SqlResult<Vec<Vec<SqlValue>>> {
        if executor.in_transaction() {
            return Err(SqlError::Unsupported(
                "DuckDB programs cannot run inside a caller transaction".into(),
            ));
        }
        self.validate()?;
        let started = Instant::now();
        check_deadline(started, self.timeout)?;
        executor.begin()?;
        let result = check_deadline(started, self.timeout)
            .and_then(|()| self.run_in_transaction(executor, started))
            .and_then(|rows| check_deadline(started, self.timeout).map(|()| rows));
        match result {
            Ok(rows) => {
                if let Err(error) = executor.commit() {
                    return match executor.rollback() {
                        Ok(()) => Err(error),
                        Err(cleanup) => Err(SqlError::Execution(format!(
                            "{error}; program rollback also failed: {cleanup}"
                        ))),
                    };
                }
                Ok(rows)
            }
            Err(error) => match executor.rollback() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(SqlError::Execution(format!(
                    "{error}; program rollback also failed: {cleanup}"
                ))),
            },
        }
    }

    fn validate(&self) -> SqlResult<()> {
        for stage in &self.stages {
            match stage {
                ProgramStage::Write(write) => self.validate_write(write)?,
                ProgramStage::RepeatUntilEmpty {
                    steps,
                    max_iterations,
                    ..
                } => {
                    if *max_iterations == 0 {
                        return Err(SqlError::Unsupported(
                            "program repeat requires a positive iteration budget".into(),
                        ));
                    }
                    for write in steps {
                        self.validate_write(write)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_write(&self, write: &WorkStage) -> SqlResult<()> {
        let table = match write {
            WorkStage::Clear(table) => table,
            WorkStage::InsertSelect { into, .. } => into,
        };
        if self.tables.contains(table) {
            Ok(())
        } else {
            Err(SqlError::Unsupported(format!(
                "program stage references foreign work table {}",
                table.sql_name
            )))
        }
    }

    fn run_in_transaction(
        &self,
        executor: &mut DuckDbExecutor,
        started: Instant,
    ) -> SqlResult<Vec<Vec<SqlValue>>> {
        for table in &self.tables {
            check_deadline(started, self.timeout)?;
            let columns = table
                .columns
                .iter()
                .map(|column| {
                    format!(
                        "{} {}{}",
                        quote_ident(&column.name),
                        column.data_type.sql(),
                        if column.nullable { "" } else { " NOT NULL" }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            executor.execute_batch(&format!("CREATE TEMP TABLE {} ({columns})", table.sql_name))?;
            check_deadline(started, self.timeout)?;
        }
        for stage in &self.stages {
            check_deadline(started, self.timeout)?;
            match stage {
                ProgramStage::Write(write) => run_write(executor, write, started, self.timeout)?,
                ProgramStage::RepeatUntilEmpty {
                    steps,
                    until_empty,
                    max_iterations,
                } => {
                    let mut terminated = false;
                    for _ in 0..*max_iterations {
                        check_deadline(started, self.timeout)?;
                        for write in steps {
                            run_write(executor, write, started, self.timeout)?;
                        }
                        check_deadline(started, self.timeout)?;
                        let rows = executor.run(&[], until_empty)?;
                        check_deadline(started, self.timeout)?;
                        if rows.is_empty() {
                            terminated = true;
                            break;
                        }
                    }
                    if !terminated {
                        return Err(SqlError::Execution(format!(
                            "DuckDB program exceeded {max_iterations} iterations"
                        )));
                    }
                }
            }
        }
        check_deadline(started, self.timeout)?;
        let rows = executor.run(&[], &self.result_sql)?;
        check_deadline(started, self.timeout)?;
        for table in self.tables.iter().rev() {
            check_deadline(started, self.timeout)?;
            executor.execute_batch(&format!("DROP TABLE {}", table.sql_name))?;
            check_deadline(started, self.timeout)?;
        }
        Ok(rows)
    }
}

fn run_write(
    executor: &mut DuckDbExecutor,
    write: &WorkStage,
    started: Instant,
    timeout: Option<Duration>,
) -> SqlResult<()> {
    check_deadline(started, timeout)?;
    let sql = match write {
        WorkStage::Clear(table) => format!("DELETE FROM {}", table.sql_name),
        WorkStage::InsertSelect { into, select_sql } => {
            format!("INSERT INTO {} {select_sql}", into.sql_name)
        }
    };
    executor.run(&[], &sql)?;
    check_deadline(started, timeout)?;
    Ok(())
}

fn check_deadline(started: Instant, timeout: Option<Duration>) -> SqlResult<()> {
    if let Some(timeout) = timeout {
        if started.elapsed() >= timeout {
            return Err(SqlError::Execution(format!(
                "DuckDB program exceeded its {timeout:?} wall-clock deadline"
            )));
        }
    }
    Ok(())
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
