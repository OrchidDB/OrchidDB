//! Caller-owned SQL sessions for regions of the common execution DAG.
//! Inputs arrive as typed Arrow relations; adapters own their engine binding.
use super::*;

pub trait RegionSession: std::fmt::Debug + Send {
    #[doc(hidden)]
    fn validate_float_helper(
        &self,
        required: &super::super::language_functions::FloatHelperCapability,
    ) -> SqlResult<()> {
        Err(SqlError::Unsupported(format!(
            "SQL region cannot verify float helper registration {}",
            required.registration_identity()
        )))
    }

    /// Whether a downstream statement may consume an unfinished statement on
    /// this session. Single-connection engines require an explicit replay spool.
    fn supports_concurrent_statements(&self) -> bool {
        false
    }
    fn dialect(&self) -> SqlDialect;
    fn query(&mut self, sql: &str, schema: SchemaRef) -> SqlResult<RecordBatch>;
    fn query_stream(
        &mut self,
        sql: &str,
        schema: SchemaRef,
        inputs: Vec<crate::execution::stream::InputRelation>,
    ) -> SqlResult<crate::execution::stream::BatchStream> {
        if !inputs.is_empty() {
            return Err(SqlError::Unsupported(
                "SQL session must implement typed streaming input relations".into(),
            ));
        }
        let batch = self.query(sql, schema.clone())?;
        if batch.num_columns() != schema.fields().len() {
            return Err(SqlError::Conversion(
                "dependent operation output column count mismatch".into(),
            ));
        }
        if batch
            .columns()
            .iter()
            .zip(schema.fields())
            .any(|(a, f)| !f.is_nullable() && a.null_count() != 0)
        {
            return Err(SqlError::Conversion(
                "NULL in non-nullable dependent operation output".into(),
            ));
        }
        let arrays = batch
            .columns()
            .iter()
            .zip(schema.fields())
            .map(|(a, f)| super::super::dag::coerce_sql_array(a, f.data_type()))
            .collect::<Result<Vec<_>, _>>()?;
        let batch = RecordBatch::try_new_with_options(
            schema.clone(),
            arrays,
            &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
        )?;
        Ok(crate::execution::stream::BatchStream::new(
            schema,
            futures::stream::iter([Ok(batch)]),
        ))
    }
}

pub(crate) type SharedRegionSession = Arc<std::sync::Mutex<Box<dyn RegionSession>>>;
