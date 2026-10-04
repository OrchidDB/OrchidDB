//! Caller-owned SQL sessions for regions of the common execution DAG.
//! Inputs are bound as query-scoped CTEs; this interface never performs DDL.
use super::*;

pub trait RegionSession: std::fmt::Debug + Send {
    fn dialect(&self) -> SqlDialect;
    fn query(&mut self, sql: &str, schema: SchemaRef) -> SqlResult<RecordBatch>;
}

pub(crate) type SharedRegionSession = Arc<std::sync::Mutex<Box<dyn RegionSession>>>;

#[cfg(feature = "duckdb")]
pub(crate) fn bind_inputs(prepared: &mut PreparedSql) -> SqlResult<()> {
    // Very large generated scalar expressions exceed practical PostgreSQL
    // planning limits. Keep that operator in the residual DAG and partition
    // its inputs into smaller SQL regions before executing anything.
    if prepared.dialect == SqlDialect::Postgres && prepared.query.len() > 256 * 1024 {
        return Err(SqlError::Unsupported("SQL region exceeds PostgreSQL planning budget".into()));
    }
    if !prepared.setup.is_empty() {
        return Err(SqlError::Unsupported("SQL regions cannot execute setup statements".into()));
    }
    for field in prepared.schema.fields() {
        crate::federation::type_name(field.data_type()).map_err(SqlError::Unsupported)?;
    }
    for table in &prepared.tables {
        let columns = table.schema.fields().iter().map(|f| Ok(crate::federation::TransferColumn {
            name: f.name().clone(),
            data_type: crate::federation::type_name(f.data_type()).map_err(SqlError::Unsupported)?,
            nullable: f.is_nullable(),
        })).collect::<SqlResult<Vec<_>>>()?;
        let transfer = crate::federation::Transfer {
            source_engine: String::new(), source_dialect: String::new(),
            sql: String::new(), target_relation: table.name.clone(), columns, operation: None,
        };
        prepared.query = crate::federation::bind_batches(&prepared.query, prepared.dialect.name(), &transfer, &table.batches)
            .map_err(SqlError::Unsupported)?;
    }
    prepared.tables.clear();
    Ok(())
}

#[cfg(feature = "postgres")]
pub struct PostgresRegionSession {
    client: Option<postgres::Client>,
}
#[cfg(feature = "postgres")]
impl std::fmt::Debug for PostgresRegionSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresRegionSession").finish_non_exhaustive()
    }
}
#[cfg(feature = "postgres")]
impl PostgresRegionSession {
    pub fn new(client: postgres::Client) -> Self { Self { client: Some(client) } }
}
#[cfg(feature = "postgres")]
impl Drop for PostgresRegionSession {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            // postgres::Client owns a blocking runtime. Closing it on an async
            // caller's thread would attempt to nest Tokio runtimes.
            if tokio::runtime::Handle::try_current().is_ok() {
                let _ = std::thread::spawn(move || drop(client)).join();
            } else { drop(client); }
        }
    }
}
#[cfg(feature = "postgres")]
impl RegionSession for PostgresRegionSession {
    fn dialect(&self) -> SqlDialect { SqlDialect::Postgres }
    fn query(&mut self, sql: &str, schema: SchemaRef) -> SqlResult<RecordBatch> {
        // JSON preserves exact numbers and recursively typed arrays. Decode using
        // the planner's Arrow schema rather than inferring from the first row.
        // Transport JSON as JSON text inside the row envelope so JSON null
        // remains distinct from an SQL-null cell, recursively within lists.
        let projection = schema.fields().iter().map(|field| {
            let ident = format!("\"{}\"", field.name().replace('"', "\"\""));
            format!("{} AS {ident}", json_transport_expression(&format!("__orchiddb_values.{ident}"), field.data_type(), 0))
        }).collect::<Vec<_>>().join(", ");
        let query = format!("SELECT row_to_json(__orchiddb_result)::text FROM (SELECT {projection} FROM ({sql}) AS __orchiddb_values) AS __orchiddb_result");
        let rows = self.client.as_mut().expect("live PostgreSQL session").query(&query, &[]).map_err(|e| SqlError::Execution(format!("postgres region: {e}: {}", e.as_db_error().map(|e| e.message()).unwrap_or(""))))?;
        let mut columns = vec![Vec::new(); schema.fields().len()];
        for row in &rows {
            let text: String = row.try_get(0).map_err(|e| SqlError::Conversion(e.to_string()))?;
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| SqlError::Conversion(e.to_string()))?;
            for (i, field) in schema.fields().iter().enumerate() {
                columns[i].push(crate::federation::json_scalar(&value[field.name()], field.data_type()).map_err(SqlError::Conversion)?);
            }
        }
        let arrays = columns.into_iter().zip(schema.fields()).map(|(values, field)| {
            if values.is_empty() { Ok(arrow::array::new_empty_array(field.data_type())) }
            else { ScalarValue::iter_to_array(values).map_err(SqlError::from) }
        }).collect::<SqlResult<Vec<_>>>()?;
        Ok(RecordBatch::try_new_with_options(schema, arrays, &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(rows.len())))?)
    }
}

/// Keep logical domain values lossless in PostgreSQL's row-to-JSON transport.
#[cfg(feature = "postgres")]
fn json_transport_expression(expression: &str, ty: &DataType, depth: usize) -> String {
    if crate::ir::functions::domain::is_json(ty) {
        return format!("CAST({expression} AS TEXT)");
    }
    fn contains_json(ty: &DataType) -> bool {
        if crate::ir::functions::domain::is_json(ty) { return true; }
        match ty {
            DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => contains_json(f.data_type()),
            _ => false,
        }
    }
    match ty {
        // Nested lists use JSONB[] with the child-list exchange encoding,
        // including serialized JSON-domain cells. They are not PostgreSQL
        // multidimensional arrays and must not be unnested recursively.
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _)
            if matches!(f.data_type(), DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)) => expression.into(),
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) if contains_json(f.data_type()) => {
            let alias = format!("__domain_item_{depth}");
            let child = json_transport_expression(&format!("{alias}.value"), f.data_type(), depth + 1);
            format!("CASE WHEN {expression} IS NULL THEN NULL ELSE ARRAY(SELECT {child} FROM unnest({expression}) WITH ORDINALITY AS {alias}(value, ordinal) ORDER BY {alias}.ordinal) END")
        }
        _ => expression.into(),
    }
}
