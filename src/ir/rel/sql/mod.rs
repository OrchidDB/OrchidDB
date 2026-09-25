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

mod functions;
pub(crate) use functions::expression_sql;
mod rows;
use rows::*;
mod literals;
use literals::*;
mod unparse;
pub use unparse::unparse;
use unparse::*;

#[cfg(feature = "duckdb")]
mod database;
#[cfg(feature = "duckdb")]
mod duckdb_exec;
#[cfg(feature = "duckdb")]
pub mod program;
#[cfg(feature = "duckdb")]
pub mod mutation;
#[cfg(feature = "duckdb")]
pub(crate) mod source_program;
#[cfg(feature = "duckdb")]
pub(crate) use database::{SharedDatabase, open_shared};
#[cfg(feature = "postgres")]
mod postgres_exec;
mod recursive;

#[cfg(feature = "duckdb")]
pub use duckdb_exec::DuckDbExecutor;
#[cfg(feature = "postgres")]
pub use postgres_exec::PostgresExecutor;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanBuilder, Decimal128Builder, Float64Builder, Int32Builder, Int64Builder,
    ListArray, RecordBatch, StringBuilder, new_null_array,
};
use arrow::buffer::{NullBuffer, OffsetBuffer};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::common::tree_node::{Transformed, TreeNode, TreeNodeRecursion};
use datafusion::common::{Column, ScalarValue};
use datafusion::error::DataFusionError;
use datafusion::functions::string::expr_fn as df_string;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::{LogicalPlan, TableScan};
use datafusion::prelude::SessionContext;
use datafusion::prelude::lit;
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

    fn ddl_type(self, data_type: &DataType) -> SqlResult<String> {
        // List-valued properties are ordinary graph data (a `tags` array on a
        // node), so they have to survive the round trip through the engine.
        if let DataType::List(inner)
        | DataType::LargeList(inner)
        | DataType::FixedSizeList(inner, _) = data_type
        {
            return Ok(format!("{}[]", self.ddl_type(inner.data_type())?));
        }
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
        }
        .to_string())
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
        if self == Self::DuckDb {
            sql = alias_duckdb_unnest_outputs(sql);
        }
        sql
    }
}

/// DataFusion tracks an unnested column under its input name, but its SQL
/// unparser omits that alias. DuckDB instead names the result
/// `unnest(column)`, which breaks outer projections and correlated filters.
fn alias_duckdb_unnest_outputs(mut sql: String) -> String {
    let mut search_from = 0;
    while let Some(relative_start) = sql[search_from..].find("UNNEST(\"") {
        let start = search_from + relative_start;
        let name_start = start + "UNNEST(\"".len();
        let Some(relative_end) = sql[name_start..].find("\")") else {
            break;
        };
        let end = name_start + relative_end + 2;
        if sql[end..].starts_with(" AS ") {
            search_from = end;
            continue;
        }
        let name = sql[name_start..end - 2].to_string();
        let alias = format!(" AS \"{name}\"");
        sql.insert_str(end, &alias);
        search_from = end + alias.len();
    }
    sql
}

/// A leaf table referenced by a lowered plan: its name in the generated SQL
/// plus the full (unprojected) contents of the backing in-memory provider.
#[derive(Debug, Clone)]
pub struct TableData {
    pub name: String,
    pub schema: SchemaRef,
    pub batches: Vec<RecordBatch>,
}

impl TableData {
    /// A name derived from the table's column types and contents, not from
    /// the scan that produced it. Scans of the same labels under different
    /// bindings, and repeated queries over the same graph, share one key, so
    /// an engine can materialize the data once and alias it per query.
    pub fn content_key(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for field in self.schema.fields() {
            field.data_type().hash(&mut hasher);
        }
        for batch in &self.batches {
            batch.num_rows().hash(&mut hasher);
            for column in batch.columns() {
                hash_array_data(&column.to_data(), &mut hasher);
            }
        }
        format!("__graph_scan_{:016x}", hasher.finish())
    }

    /// Approximate in-engine footprint, used for cache eviction.
    pub fn byte_size(&self) -> usize {
        self.batches
            .iter()
            .map(RecordBatch::get_array_memory_size)
            .sum()
    }

    /// Render this table's `CREATE TABLE` + `INSERT` statements.
    pub fn setup_sql(&self, dialect: SqlDialect) -> SqlResult<Vec<String>> {
        table_setup_sql(dialect, self)
    }
}

fn hash_array_data(data: &arrow::array::ArrayData, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    data.data_type().hash(hasher);
    data.len().hash(hasher);
    data.offset().hash(hasher);
    match data.nulls() {
        Some(nulls) => {
            1u8.hash(hasher);
            nulls.offset().hash(hasher);
            nulls.buffer().as_slice().hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
    for buffer in data.buffers() {
        buffer.as_slice().hash(hasher);
    }
    for child in data.child_data() {
        hash_array_data(child, hasher);
    }
}

/// Everything needed to run one lowered plan on a real database: setup DDL +
/// inserts, the query text, and the metadata to rebuild an Arrow batch from
/// the returned rows.
#[derive(Debug, Clone)]
pub struct PreparedSql {
    pub dialect: SqlDialect,
    /// Tables the query reads that the engine must materialize first. Kept
    /// as Arrow data so an engine can load it natively and cache it; see
    /// [`SqlExecutor::run_with_tables`].
    pub tables: Vec<TableData>,
    /// Additional setup statements, applied after `tables`.
    pub setup: Vec<String>,
    pub query: String,
    pub fields: Vec<String>,
    pub result_form: ResultForm,
    schema: SchemaRef,
}

/// A SQL engine that can apply setup statements and run a query, returning
/// plain row values. Implementations may retain their session between calls;
/// setup statements must therefore be safe to present repeatedly.
pub trait SqlExecutor {
    fn dialect(&self) -> SqlDialect;
    fn run(&mut self, setup: &[String], query: &str) -> SqlResult<Vec<Vec<SqlValue>>>;

    /// Materialize `tables`, apply `setup`, then run `query`.
    ///
    /// The default renders every table as `CREATE TABLE` + `INSERT` text.
    /// That is proportional to the data on every call, so engines that can
    /// ingest Arrow directly and keep tables between calls should override it.
    fn run_with_tables(
        &mut self,
        tables: &[TableData],
        setup: &[String],
        query: &str,
    ) -> SqlResult<Vec<Vec<SqlValue>>> {
        let mut statements = Vec::new();
        for table in tables {
            statements.extend(table.setup_sql(self.dialect())?);
        }
        statements.extend_from_slice(setup);
        self.run(&statements, query)
    }
}

/// Engine-neutral cell value returned by [`SqlExecutor::run`].
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Bool(bool),
    Int(i64),
    /// An exact integral or fixed-point value that cannot safely pass through
    /// either i64 or f64.
    ExactNumber(String),
    Float(f64),
    Text(String),
    List(Vec<SqlValue>),
}

/// Convert one Arrow array element into an engine-neutral [`SqlValue`].
///
/// DuckDB hands nested values back as Arrow arrays rather than scalars, so its
/// executor reuses this instead of re-deriving the mapping.
pub(super) fn sql_value_from_array(array: &dyn Array, index: usize) -> SqlResult<SqlValue> {
    scalar_to_sql_value(&ScalarValue::try_from_array(array, index)?)
}

fn scalar_to_sql_value(scalar: &ScalarValue) -> SqlResult<SqlValue> {
    fn opt<T>(value: &Option<T>, render: impl Fn(&T) -> SqlValue) -> SqlValue {
        value.as_ref().map_or(SqlValue::Null, render)
    }
    Ok(match scalar {
        ScalarValue::Null => SqlValue::Null,
        ScalarValue::Boolean(v) => opt(v, |b| SqlValue::Bool(*b)),
        ScalarValue::Int8(v) => opt(v, |i| SqlValue::Int(i64::from(*i))),
        ScalarValue::Int16(v) => opt(v, |i| SqlValue::Int(i64::from(*i))),
        ScalarValue::Int32(v) => opt(v, |i| SqlValue::Int(i64::from(*i))),
        ScalarValue::Int64(v) => opt(v, |i| SqlValue::Int(*i)),
        ScalarValue::UInt8(v) => opt(v, |i| SqlValue::Int(i64::from(*i))),
        ScalarValue::UInt16(v) => opt(v, |i| SqlValue::Int(i64::from(*i))),
        ScalarValue::UInt32(v) => opt(v, |i| SqlValue::Int(i64::from(*i))),
        ScalarValue::UInt64(v) => match v {
            Some(value) => SqlValue::Int(
                i64::try_from(*value)
                    .map_err(|_| SqlError::Conversion(format!("unsigned {value} overflows i64")))?,
            ),
            None => SqlValue::Null,
        },
        ScalarValue::Float32(v) => opt(v, |f| SqlValue::Float(f64::from(*f))),
        ScalarValue::Float64(v) => opt(v, |f| SqlValue::Float(*f)),
        ScalarValue::Decimal128(v, _precision, scale) => match v {
            Some(value) => SqlValue::ExactNumber(render_scaled_i128(*value, *scale)),
            None => SqlValue::Null,
        },
        ScalarValue::Utf8(v) | ScalarValue::LargeUtf8(v) | ScalarValue::Utf8View(v) => {
            opt(v, |s| SqlValue::Text(s.clone()))
        }
        ScalarValue::List(array) => nested_sql_values(array.as_ref(), array.value(0))?,
        ScalarValue::LargeList(array) => nested_sql_values(array.as_ref(), array.value(0))?,
        ScalarValue::FixedSizeList(array) => nested_sql_values(array.as_ref(), array.value(0))?,
        other => {
            return Err(SqlError::Unsupported(format!(
                "engine value of type {}",
                other.data_type()
            )));
        }
    })
}

fn nested_sql_values(outer: &dyn Array, items: ArrayRef) -> SqlResult<SqlValue> {
    if outer.is_null(0) {
        return Ok(SqlValue::Null);
    }
    let mut out = Vec::with_capacity(items.len());
    for index in 0..items.len() {
        out.push(sql_value_from_array(items.as_ref(), index)?);
    }
    Ok(SqlValue::List(out))
}

fn render_scaled_i128(value: i128, scale: i8) -> String {
    if scale <= 0 {
        return format!("{value}{}", "0".repeat((-scale) as usize));
    }
    let negative = value.is_negative();
    let mut digits = value.unsigned_abs().to_string();
    let scale = scale as usize;
    if digits.len() <= scale {
        digits.insert_str(0, &"0".repeat(scale + 1 - digits.len()));
    }
    let split = digits.len() - scale;
    digits.insert(split, '.');
    if negative {
        digits.insert(0, '-');
    }
    digits
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
    let mut cte_names = BTreeSet::new();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::RecursiveQuery(recursive) = node {
            cte_names.insert(recursive.name.clone());
        }
        Ok(TreeNodeRecursion::Continue)
    })?;

    let mut sources = BTreeMap::new();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            let name = scan.table_name.to_string();
            let bare_name = scan.table_name.table();
            if !cte_names.contains(&name)
                && !cte_names.contains(bare_name)
                && !external.contains(&name)
                && !external.contains(bare_name)
            {
                sources
                    .entry(name)
                    .or_insert_with(|| Arc::clone(&scan.source));
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;

    let mut out = Vec::with_capacity(sources.len());
    let mut fallback_context = None;
    for (name, source) in sources {
        let schema = source.schema();
        // Lowered graph scans already own immutable Arrow batches. Reading
        // their complete contents needs neither planning nor execution. Scan
        // filters and projections remain in the generated SQL.
        let provider = datafusion::datasource::source_as_provider(&source).ok();
        let batches = if let Some(table) = provider.as_ref()
            .and_then(|provider| provider.as_any().downcast_ref::<datafusion::datasource::MemTable>())
        {
            let mut batches = Vec::new();
            for partition in &table.batches {
                batches.extend(partition.read().await.iter().cloned());
            }
            batches
        } else {
            let scan = TableScan::try_new(name.clone(), source, None, Vec::new(), None)?;
            let ctx = fallback_context.get_or_insert_with(SessionContext::new);
            ctx.execute_logical_plan(LogicalPlan::TableScan(scan)).await?.collect().await?
        };
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
    let quoted_name = dialect.quote_ident(&table.name);
    let mut statements = vec![create_table_sql(dialect, table)?];

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

/// Render the `CREATE TABLE` statement for one table, without its rows.
pub(crate) fn create_table_sql(dialect: SqlDialect, table: &TableData) -> SqlResult<String> {
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
    Ok(format!(
        "{} {} ({})",
        dialect.create_table_keyword(),
        dialect.quote_ident(&table.name),
        columns.join(", ")
    ))
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
    Ok(PreparedSql {
        dialect,
        tables,
        setup: Vec::new(),
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
    let rows = executor.run_with_tables(&prepared.tables, &prepared.setup, &prepared.query)?;
    let batch = if prepared.schema.fields().is_empty() {
        // SQL represents the empty tuple with a physical unit projection.
        // Its value is not part of the logical schema, but every row is.
        RecordBatch::try_new_with_options(
            Arc::clone(&prepared.schema), Vec::new(),
            &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(rows.len())),
        )?
    } else {
        rows_to_batch(&prepared.schema, &rows)?
    };
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
        .map(|field| {
            // DuckDB exposes concatenated strings as Utf8View. The SQL
            // boundary owns the returned buffers, so normalize every string
            // representation to ordinary Utf8 rather than requiring callers
            // to understand an engine-specific Arrow layout.
            let data_type = match field.data_type() {
                DataType::LargeUtf8 | DataType::Utf8View => DataType::Utf8,
                other => other.clone(),
            };
            Field::new(field.name(), data_type, true)
        })
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
            sql_literal(
                SqlDialect::DuckDb,
                &ScalarValue::Utf8(Some("o'hare".into()))
            )
            .unwrap(),
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
        let nul = sql_literal(SqlDialect::DuckDb, &ScalarValue::Utf8(Some("a\0b".into()))).unwrap();
        assert_eq!(nul, "'a' || chr(0) || 'b'");
        assert!(!nul.contains('\0'));
    }

    #[test]
    fn rejects_unsupported_ddl_types() {
        let err = SqlDialect::Postgres
            .ddl_type(&DataType::Binary)
            .unwrap_err();
        assert!(matches!(err, SqlError::Unsupported(_)));
    }

    #[test]
    fn duckdb_fixups_alias_unnest_outputs() {
        let sql = SqlDialect::DuckDb.fixup_query(
            "SELECT UNNEST(\"items\"), UNNEST(\"items\") AS \"copy\", array_min(\"items\"), array_intersect(\"items\", \"other\")".into(),
        );
        assert_eq!(
            sql,
            "SELECT UNNEST(\"items\") AS \"items\", UNNEST(\"items\") AS \"copy\", array_min(\"items\"), array_intersect(\"items\", \"other\")"
        );
    }
}
