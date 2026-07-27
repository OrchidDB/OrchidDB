//! SQL text generation and real-database execution for lowered plans.
//!
//! `ir::rel` lowers Graph IR to a DataFusion `LogicalPlan` whose leaves are
//! in-memory table scans derived from the `PropertyGraph`. This module turns
//! that lowered plan into dialect-specific SQL text (via DataFusion's
//! unparser), materializes the scan tables into a real database as
//! `CREATE TABLE` + `INSERT` statements, executes the SQL there, and converts
//! the rows back into Arrow batches so results can be compared against the
//! in-process DataFusion execution.
//!
//! The pieces are deliberately separable:
//!
//! * [`SqlDialect`] — the dialect abstraction (identifier quoting, DDL types,
//!   literal rendering, unparser dialect selection, post-unparse fixups).
//! * [`unparse`] / [`plan_tables`] / [`table_setup_sql`] — pure SQL
//!   generation; anything the unparser cannot express surfaces as a typed
//!   [`SqlError::Unsupported`], never as wrong SQL.
//! * [`SqlExecutor`] — the engine abstraction. [`DuckDbExecutor`] (feature
//!   `duckdb`, on by default) runs everything in-memory; [`PostgresExecutor`]
//!   (feature `postgres`) connects to a live server via `GRAPH_PG_URL`.

#[cfg(feature = "duckdb")]
mod duckdb_exec;
#[cfg(feature = "postgres")]
mod postgres_exec;

#[cfg(feature = "duckdb")]
pub use duckdb_exec::DuckDbExecutor;
#[cfg(feature = "postgres")]
pub use postgres_exec::PostgresExecutor;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int32Builder, Int64Builder, RecordBatch,
    StringBuilder, new_null_array,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::common::tree_node::{Transformed, TreeNode, TreeNodeRecursion};
use datafusion::common::{Column, ScalarValue};
use datafusion::logical_expr::Expr;
use datafusion::error::DataFusionError;
use datafusion::logical_expr::{LogicalPlan, TableScan};
use datafusion::prelude::SessionContext;
use datafusion::sql::unparser::Unparser;
use datafusion::sql::unparser::dialect::{
    Dialect as UnparserDialect, DuckDBDialect, PostgreSqlDialect,
};

use crate::ir::catalog::PropertyGraph;
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::policy::ResultForm;

use super::LoweredPlan;

/// Rows per generated multi-row `INSERT` statement.
const INSERT_CHUNK_ROWS: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum SqlError {
    #[error("unsupported sql generation: {0}")]
    Unsupported(String),
    #[error("sql setup: {0}")]
    Setup(String),
    #[error("sql execution: {0}")]
    Execution(String),
    #[error("sql result conversion: {0}")]
    Conversion(String),
    #[error("datafusion: {0}")]
    DataFusion(#[from] DataFusionError),
    #[error("arrow: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
}

pub type SqlResult<T> = Result<T, SqlError>;

/// Target SQL dialect. All dialect-specific behavior — identifier quoting,
/// DDL type names, literal syntax, unparser selection, and post-unparse
/// fixups — lives here so executors and callers stay dialect-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    DuckDb,
    Postgres,
}

impl SqlDialect {
    pub fn name(self) -> &'static str {
        match self {
            Self::DuckDb => "duckdb",
            Self::Postgres => "postgres",
        }
    }

    pub fn quote_ident(self, ident: &str) -> String {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    fn unparser_dialect(self) -> Box<dyn UnparserDialect> {
        match self {
            Self::DuckDb => Box::new(DuckDBDialect::new()),
            Self::Postgres => Box::new(PostgreSqlDialect {}),
        }
    }

    fn create_table_keyword(self) -> &'static str {
        match self {
            // Fresh in-memory databases per run; OR REPLACE keeps re-running
            // setup scripts idempotent.
            Self::DuckDb => "CREATE OR REPLACE TABLE",
            // Executors run inside a rolled-back transaction, so temp tables
            // never leak into a shared server.
            Self::Postgres => "CREATE TEMPORARY TABLE",
        }
    }

    fn double_type(self) -> &'static str {
        match self {
            Self::DuckDb => "DOUBLE",
            Self::Postgres => "DOUBLE PRECISION",
        }
    }

    fn ddl_type(self, data_type: &DataType) -> SqlResult<&'static str> {
        Ok(match data_type {
            DataType::Boolean => "BOOLEAN",
            DataType::Int8 | DataType::Int16 | DataType::UInt8 => "SMALLINT",
            DataType::Int32 | DataType::UInt16 => "INTEGER",
            DataType::Int64 | DataType::UInt32 | DataType::UInt64 => "BIGINT",
            DataType::Float32 => "REAL",
            DataType::Float64 => self.double_type(),
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => match self {
                Self::DuckDb => "VARCHAR",
                Self::Postgres => "TEXT",
            },
            // All-null fixture columns still need a concrete SQL type.
            DataType::Null => match self {
                Self::DuckDb => "VARCHAR",
                Self::Postgres => "TEXT",
            },
            other => {
                return Err(SqlError::Unsupported(format!(
                    "no {} DDL type for arrow type {other}",
                    self.name()
                )));
            }
        })
    }

    /// Dialect-specific rewrites of unparsed SQL text. Fixups that cannot be
    /// expressed through the unparser dialect hooks belong here, not at call
    /// sites.
    fn fixup_query(self, sql: String) -> String {
        // The unparser renders unsigned Arrow casts MySQL-style
        // (`BIGINT UNSIGNED`), which neither DuckDB nor Postgres parse.
        let unsigned_casts: &[(&str, &str)] = match self {
            Self::DuckDb => &[
                ("BIGINT UNSIGNED", "UBIGINT"),
                ("INTEGER UNSIGNED", "UINTEGER"),
                ("SMALLINT UNSIGNED", "USMALLINT"),
                ("TINYINT UNSIGNED", "UTINYINT"),
            ],
            // Postgres has no unsigned types; widen to the next signed type
            // (NUMERIC for u64, which cannot fit in BIGINT).
            Self::Postgres => &[
                ("BIGINT UNSIGNED", "NUMERIC(20)"),
                ("INTEGER UNSIGNED", "BIGINT"),
                ("SMALLINT UNSIGNED", "INTEGER"),
                ("TINYINT UNSIGNED", "SMALLINT"),
            ],
        };
        let mut sql = sql;
        for (from, to) in unsigned_casts {
            if sql.contains(from) {
                sql = sql.replace(from, to);
            }
        }
        sql
    }
}

/// A leaf table referenced by a lowered plan: its name in the generated SQL
/// plus the full (unprojected) contents of the backing in-memory provider.
#[derive(Debug, Clone)]
pub struct TableData {
    pub name: String,
    pub schema: SchemaRef,
    pub batches: Vec<RecordBatch>,
}

/// Everything needed to run one lowered plan on a real database: setup DDL +
/// inserts, the query text, and the metadata to rebuild an Arrow batch from
/// the returned rows.
#[derive(Debug, Clone)]
pub struct PreparedSql {
    pub dialect: SqlDialect,
    pub setup: Vec<String>,
    pub query: String,
    pub fields: Vec<String>,
    pub result_form: ResultForm,
    schema: SchemaRef,
}

/// A SQL engine that can run a setup script and a query in one isolated
/// session, returning plain row values. Implementations should not leak state
/// between `run` calls.
pub trait SqlExecutor {
    fn dialect(&self) -> SqlDialect;
    fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>>;
}

/// Engine-neutral cell value returned by [`SqlExecutor::run`].
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

/// Unparse a lowered plan to dialect-specific SQL text. Constructs the
/// unparser cannot express surface as [`SqlError::Unsupported`].
pub fn unparse(lowered: &LoweredPlan, dialect: SqlDialect) -> SqlResult<String> {
    let plan = strip_identity_projections(lowered.plan.clone())?;
    let plan = strip_column_qualifiers(plan)
        .map_err(|err| SqlError::Unsupported(format!("qualifier strip: {err}")))?;
    let unparser_dialect = dialect.unparser_dialect();
    let unparser = Unparser::new(unparser_dialect.as_ref());
    let statement = unparser.plan_to_sql(&plan).map_err(|err| {
        SqlError::Unsupported(format!("unparser ({}): {err}", dialect.name()))
    })?;
    Ok(dialect.fixup_query(statement.to_string()))
}

/// Remove projections that select every input column unchanged. The unparser
/// otherwise collapses `Projection -> Sort -> Projection` stacks into a
/// single SELECT and loses the inner aliases, producing SQL that references
/// columns which no longer exist.
fn strip_identity_projections(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    let transformed = plan.transform_up(|node| {
        if let LogicalPlan::Projection(projection) = &node
            && is_identity_projection(projection)
        {
            return Ok(Transformed::yes(projection.input.as_ref().clone()));
        }
        Ok(Transformed::no(node))
    })?;
    Ok(transformed.data)
}

/// Rewrite column references to be unqualified wherever that stays
/// unambiguous. DataFusion plans keep base-table qualifiers alive across
/// projection boundaries, which the unparser turns into references like
/// `"table"."col"` outside the derived table that hides `"table"` — invalid
/// SQL. Lowered plans use globally binding-prefixed column names (`a__id`,
/// `e1__src_id`, ...), so unqualified references stay unambiguous — except
/// inside user-supplied subplans ("bring your own schema" views/queries),
/// where the same column name can appear on both sides of a join. There the
/// qualifier is required and is kept: it names a `SubqueryAlias`/table that
/// exists as a FROM alias in the emitted SQL.
fn strip_column_qualifiers(plan: LogicalPlan) -> Result<LogicalPlan, DataFusionError> {
    let transformed = plan.transform_up_with_subqueries(|node| {
        // Count how many input fields share each unqualified name in this
        // node's scope; only unique names can safely lose their qualifier.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        {
            let inputs = node.inputs();
            let scope_schemas: Vec<_> = if inputs.is_empty() {
                vec![node.schema()]
            } else {
                inputs.iter().map(|input| input.schema()).collect()
            };
            for schema in scope_schemas {
                for (_, field) in schema.iter() {
                    *counts.entry(field.name().clone()).or_default() += 1;
                }
            }
        }
        let unique = |name: &str| counts.get(name).copied().unwrap_or(0) <= 1;
        let rewritten = node.map_expressions(|expr| {
            expr.transform_up(|inner| {
                if let Expr::Column(column) = &inner
                    && column.relation.is_some()
                    && unique(&column.name)
                {
                    return Ok(Transformed::yes(Expr::Column(Column::new_unqualified(
                        column.name.clone(),
                    ))));
                }
                Ok(Transformed::no(inner))
            })
        })?;
        if rewritten.transformed {
            Ok(Transformed::yes(rewritten.data.recompute_schema()?))
        } else {
            Ok(rewritten)
        }
    })?;
    Ok(transformed.data)
}

fn is_identity_projection(projection: &datafusion::logical_expr::Projection) -> bool {
    let input_schema = projection.input.schema();
    if projection.expr.len() != input_schema.fields().len()
        || projection.schema.fields().len() != input_schema.fields().len()
    {
        return false;
    }
    let columns_only = projection
        .expr
        .iter()
        .zip(input_schema.iter())
        .all(|(expr, (qualifier, field))| match expr {
            Expr::Column(column) => {
                column.name == *field.name()
                    && (column.relation.is_none() || column.relation.as_ref() == qualifier)
            }
            _ => false,
        });
    // Removing the projection must not change the observable schema
    // (names and qualifiers) the parent plan sees.
    columns_only
        && projection
            .schema
            .iter()
            .zip(input_schema.iter())
            .all(|((out_qualifier, out_field), (in_qualifier, in_field))| {
                out_qualifier == in_qualifier && out_field.name() == in_field.name()
            })
}

/// Collect the leaf tables a lowered plan scans (including scans inside
/// subqueries) with their full contents, so they can be materialized in a
/// target database under the same names the unparsed SQL references.
pub async fn plan_tables(plan: &LogicalPlan) -> SqlResult<Vec<TableData>> {
    plan_tables_excluding(plan, &BTreeSet::new()).await
}

/// Like [`plan_tables`], but skips scan leaves whose table name is in
/// `external` — tables the caller knows already exist in the target database
/// (e.g. "bring your own schema" mappings), which must not be materialized.
pub async fn plan_tables_excluding(
    plan: &LogicalPlan,
    external: &BTreeSet<String>,
) -> SqlResult<Vec<TableData>> {
    let mut sources = BTreeMap::new();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            let name = scan.table_name.to_string();
            if !external.contains(&name) && !external.contains(scan.table_name.table()) {
                sources.entry(name).or_insert_with(|| Arc::clone(&scan.source));
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;

    let mut out = Vec::with_capacity(sources.len());
    for (name, source) in sources {
        let schema = source.schema();
        let scan = TableScan::try_new(name.clone(), source, None, Vec::new(), None)?;
        let ctx = SessionContext::new();
        let df = ctx
            .execute_logical_plan(LogicalPlan::TableScan(scan))
            .await?;
        let batches = df.collect().await?;
        out.push(TableData {
            name,
            schema,
            batches,
        });
    }
    Ok(out)
}

/// Render `CREATE TABLE` + `INSERT` statements that materialize one table.
pub fn table_setup_sql(dialect: SqlDialect, table: &TableData) -> SqlResult<Vec<String>> {
    let fields = table.schema.fields();
    if fields.is_empty() {
        return Err(SqlError::Unsupported(format!(
            "table `{}` has no columns",
            table.name
        )));
    }
    let mut columns = Vec::with_capacity(fields.len());
    for field in fields {
        columns.push(format!(
            "{} {}",
            dialect.quote_ident(field.name()),
            dialect.ddl_type(field.data_type())?
        ));
    }
    let quoted_name = dialect.quote_ident(&table.name);
    let mut statements = vec![format!(
        "{} {quoted_name} ({})",
        dialect.create_table_keyword(),
        columns.join(", ")
    )];

    let mut rows = Vec::new();
    for batch in &table.batches {
        for row in 0..batch.num_rows() {
            let mut cells = Vec::with_capacity(batch.num_columns());
            for column in batch.columns() {
                let scalar = ScalarValue::try_from_array(column, row)?;
                cells.push(sql_literal(dialect, &scalar)?);
            }
            rows.push(format!("({})", cells.join(", ")));
        }
    }
    for chunk in rows.chunks(INSERT_CHUNK_ROWS) {
        statements.push(format!(
            "INSERT INTO {quoted_name} VALUES {}",
            chunk.join(", ")
        ));
    }
    Ok(statements)
}

/// Materialize an entire `PropertyGraph` as `node_<label>` / `edge_<rel>`
/// tables. This is independent of any query; it lets callers stand up the
/// catalog itself in a target database.
pub fn graph_setup_sql(dialect: SqlDialect, graph: &PropertyGraph) -> SqlResult<Vec<String>> {
    let mut statements = Vec::new();
    let mut labels = graph.labels();
    labels.sort();
    for label in labels {
        let table = graph
            .node_table(&label)
            .map_err(|err| SqlError::Setup(format!("{err}")))?;
        statements.extend(table_setup_sql(
            dialect,
            &TableData {
                name: format!("node_{}", sanitize_table_name(&label)),
                schema: table.batch.schema(),
                batches: vec![table.batch.clone()],
            },
        )?);
    }
    let mut rel_types = graph.rel_types();
    rel_types.sort();
    for rel_type in rel_types {
        let tables = graph
            .edge_tables(&rel_type)
            .map_err(|err| SqlError::Setup(format!("{err}")))?;
        for (index, table) in tables.iter().enumerate() {
            let name = if index == 0 {
                format!("edge_{}", sanitize_table_name(&rel_type))
            } else {
                format!("edge_{}_{index}", sanitize_table_name(&rel_type))
            };
            statements.extend(table_setup_sql(
                dialect,
                &TableData {
                    name,
                    schema: table.batch.schema(),
                    batches: vec![table.batch.clone()],
                },
            )?);
        }
    }
    Ok(statements)
}

/// Generate SQL text and setup statements for a lowered plan.
pub async fn prepare(lowered: &LoweredPlan, dialect: SqlDialect) -> SqlResult<PreparedSql> {
    prepare_with_external(lowered, dialect, &BTreeSet::new()).await
}

/// Like [`prepare`], but treats the named tables as *external*: they already
/// exist in the target database (the user's own tables/views under a
/// [`GraphMapping`](crate::ir::rel::mapping::GraphMapping)), so no
/// `CREATE TABLE`/`INSERT` setup is generated for them. Pass
/// `GraphMapping::physical_table_names()` here.
pub async fn prepare_with_external(
    lowered: &LoweredPlan,
    dialect: SqlDialect,
    external: &BTreeSet<String>,
) -> SqlResult<PreparedSql> {
    let query = unparse(lowered, dialect)?;
    let tables = plan_tables_excluding(&lowered.plan, external).await?;
    let mut setup = Vec::new();
    for table in &tables {
        setup.extend(table_setup_sql(dialect, table)?);
    }
    Ok(PreparedSql {
        dialect,
        setup,
        query,
        fields: lowered.fields.clone(),
        result_form: lowered.result_form,
        schema: result_schema(&lowered.plan),
    })
}

/// Run prepared SQL on an executor and rebuild the result as Arrow batches
/// with the lowered plan's output schema (all fields nullable).
pub fn execute_prepared(
    executor: &mut dyn SqlExecutor,
    prepared: &PreparedSql,
) -> SqlResult<ReturnedBatches> {
    if executor.dialect() != prepared.dialect {
        return Err(SqlError::Setup(format!(
            "prepared for {} but executor speaks {}",
            prepared.dialect.name(),
            executor.dialect().name()
        )));
    }
    let rows = executor.run(&prepared.setup, &prepared.query)?;
    let batch = rows_to_batch(&prepared.schema, &rows)?;
    Ok(ReturnedBatches {
        fields: prepared.fields.clone(),
        result_form: prepared.result_form,
        batch,
    })
}

/// Convenience wrapper: prepare for the executor's dialect, then execute.
pub async fn execute_lowered_sql(
    executor: &mut dyn SqlExecutor,
    lowered: &LoweredPlan,
) -> SqlResult<ReturnedBatches> {
    let prepared = prepare(lowered, executor.dialect()).await?;
    execute_prepared(executor, &prepared)
}

fn result_schema(plan: &LogicalPlan) -> SchemaRef {
    let fields = plan
        .schema()
        .as_arrow()
        .fields()
        .iter()
        .map(|field| Field::new(field.name(), field.data_type().clone(), true))
        .collect::<Vec<_>>();
    Arc::new(Schema::new(fields))
}

fn sanitize_table_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn sql_literal(dialect: SqlDialect, value: &ScalarValue) -> SqlResult<String> {
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
        ScalarValue::Utf8(v) | ScalarValue::LargeUtf8(v) | ScalarValue::Utf8View(v) => {
            opt(v, |s| string_literal(s))
        }
        other => {
            return Err(SqlError::Unsupported(format!(
                "sql literal for {} value",
                other.data_type()
            )));
        }
    })
}

fn string_literal(input: &str) -> String {
    format!("'{}'", input.replace('\'', "''"))
}

fn float_literal(dialect: SqlDialect, value: f64) -> String {
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

fn rows_to_batch(schema: &SchemaRef, rows: &[Vec<SqlValue>]) -> SqlResult<RecordBatch> {
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

fn build_column(field: &Field, rows: &[Vec<SqlValue>], index: usize) -> SqlResult<ArrayRef> {
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
                    SqlValue::Int(value) => builder.append_value(
                        i32::try_from(*value).map_err(|_| mismatch(&row[index]))?,
                    ),
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
        other => {
            return Err(SqlError::Unsupported(format!(
                "result column `{}` of type {other}",
                field.name()
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_escapes_identifiers() {
        assert_eq!(SqlDialect::DuckDb.quote_ident("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn renders_literals() {
        assert_eq!(
            sql_literal(SqlDialect::DuckDb, &ScalarValue::Utf8(Some("o'hare".into()))).unwrap(),
            "'o''hare'"
        );
        assert_eq!(
            sql_literal(SqlDialect::DuckDb, &ScalarValue::Float64(Some(2.0))).unwrap(),
            "2.0"
        );
        assert_eq!(
            sql_literal(SqlDialect::Postgres, &ScalarValue::Float64(Some(f64::NAN))).unwrap(),
            "CAST('NaN' AS DOUBLE PRECISION)"
        );
        assert_eq!(
            sql_literal(SqlDialect::DuckDb, &ScalarValue::Int64(None)).unwrap(),
            "NULL"
        );
    }

    #[test]
    fn rejects_unsupported_ddl_types() {
        let err = SqlDialect::Postgres
            .ddl_type(&DataType::Binary)
            .unwrap_err();
        assert!(matches!(err, SqlError::Unsupported(_)));
    }
}
