//! Postgres implementation of [`SqlExecutor`].
//!
//! Connects to a live server (connection string usually supplied through the
//! `GRAPH_PG_URL` environment variable). Every `run` executes inside a
//! transaction that is rolled back afterwards, and the setup script creates
//! temporary tables, so nothing persists on the server.

use postgres::types::Type;
use postgres::{Client, NoTls};

use super::{SqlDialect, SqlError, SqlExecutor, SqlResult, SqlValue};

pub struct PostgresExecutor {
    client: Client,
}

impl PostgresExecutor {
    /// Environment variable holding the connection string, e.g.
    /// `postgres://user:pass@localhost:5432/dbname`.
    pub const ENV_URL: &'static str = "GRAPH_PG_URL";

    pub fn connect(url: &str) -> SqlResult<Self> {
        let client = Client::connect(url, NoTls)
            .map_err(|err| SqlError::Setup(format!("postgres connect: {err}")))?;
        Ok(Self { client })
    }

    /// Connect using [`Self::ENV_URL`]; returns `Ok(None)` when unset so
    /// callers can skip Postgres coverage without failing.
    pub fn from_env() -> SqlResult<Option<Self>> {
        match std::env::var(Self::ENV_URL) {
            Ok(url) => Self::connect(&url).map(Some),
            Err(_) => Ok(None),
        }
    }
}

impl SqlExecutor for PostgresExecutor {
    fn dialect(&self) -> SqlDialect {
        SqlDialect::Postgres
    }

    fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>> {
        let mut tx = self
            .client
            .transaction()
            .map_err(|err| SqlError::Setup(format!("postgres begin: {err}")))?;
        for statement in setup {
            tx.batch_execute(statement).map_err(|err| {
                SqlError::Setup(format!("postgres setup: {err}\nstatement: {statement}"))
            })?;
        }
        let rows = tx
            .query(query, &[])
            .map_err(|err| SqlError::Execution(format!("postgres query: {err}")))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut cells = Vec::with_capacity(row.columns().len());
            for (index, column) in row.columns().iter().enumerate() {
                cells.push(convert_cell(row, index, column.type_())?);
            }
            out.push(cells);
        }
        tx.rollback()
            .map_err(|err| SqlError::Execution(format!("postgres rollback: {err}")))?;
        Ok(out)
    }
}

fn convert_cell(row: &postgres::Row, index: usize, ty: &Type) -> SqlResult<SqlValue> {
    fn get<'a, T: postgres::types::FromSql<'a>>(
        row: &'a postgres::Row,
        index: usize,
    ) -> SqlResult<Option<T>> {
        row.try_get(index)
            .map_err(|err| SqlError::Conversion(format!("postgres cell {index}: {err}")))
    }
    let value = if *ty == Type::BOOL {
        get::<bool>(row, index)?.map(SqlValue::Bool)
    } else if *ty == Type::INT2 {
        get::<i16>(row, index)?.map(|v| SqlValue::Int(i64::from(v)))
    } else if *ty == Type::INT4 {
        get::<i32>(row, index)?.map(|v| SqlValue::Int(i64::from(v)))
    } else if *ty == Type::INT8 {
        get::<i64>(row, index)?.map(SqlValue::Int)
    } else if *ty == Type::FLOAT4 {
        get::<f32>(row, index)?.map(|v| SqlValue::Float(f64::from(v)))
    } else if *ty == Type::FLOAT8 {
        get::<f64>(row, index)?.map(SqlValue::Float)
    } else if *ty == Type::TEXT || *ty == Type::VARCHAR || *ty == Type::BPCHAR || *ty == Type::NAME
    {
        get::<String>(row, index)?.map(SqlValue::Text)
    } else {
        return Err(SqlError::Unsupported(format!(
            "postgres value type {ty} in column {index}"
        )));
    };
    Ok(value.unwrap_or(SqlValue::Null))
}
