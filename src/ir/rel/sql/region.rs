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
            sql: String::new(), target_relation: table.name.clone(), columns, operation: None, request: None,
        };
        prepared.query = crate::federation::bind_batches(&prepared.query, prepared.dialect.name(), &transfer, &table.batches)
            .map_err(SqlError::Unsupported)?;
    }
    prepared.tables.clear();
    Ok(())
}
