//! Execute dependent relational operations through an engine-owned SQL session.
use super::DependentOperation;
use crate::ir::rel::{dag::coerce_sql_array, sql};
use arrow::array::RecordBatch;
use datafusion::{
    common::{DataFusionError, Result, ScalarValue},
    execution::TaskContext,
    physical_expr::EquivalenceProperties,
    physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
        SendableRecordBatchStream,
        execution_plan::{Boundedness, EmissionType},
        stream::RecordBatchStreamAdapter,
    },
};
use futures::stream;
use std::{
    any::Any,
    fmt,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub struct DependentOperationExec {
    source: Arc<dyn ExecutionPlan>,
    template: sql::lowering::SqlTemplate,
    session: sql::region::SharedRegionSession,
    cost: Arc<Mutex<crate::ir::QueryCost>>,
    properties: Arc<PlanProperties>,
}
impl DependentOperationExec {
    pub fn new(
        operation: &DependentOperation,
        source: Arc<dyn ExecutionPlan>,
        session: sql::region::SharedRegionSession,
        cost: Arc<Mutex<crate::ir::QueryCost>>,
    ) -> Self {
        Self {
            source,
            template: operation.template.clone(),
            session,
            cost,
            properties: Arc::new(PlanProperties::new(
                EquivalenceProperties::new(Arc::new(operation.schema.as_arrow().clone())),
                Partitioning::UnknownPartitioning(1),
                EmissionType::Final,
                Boundedness::Bounded,
            )),
        }
    }
}
impl DisplayAs for DependentOperationExec {
    fn fmt_as(&self, _: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DependentOperationExec: {}", self.template.sql)
    }
}
impl ExecutionPlan for DependentOperationExec {
    fn name(&self) -> &str {
        "DependentOperationExec"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.source]
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.len() != 1 {
            return Err(DataFusionError::Plan(
                "dependent operation requires one input".into(),
            ));
        }
        Ok(Arc::new(Self {
            source: children[0].clone(),
            template: self.template.clone(),
            session: self.session.clone(),
            cost: self.cost.clone(),
            properties: self.properties.clone(),
        }))
    }
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(
                "dependent operation has one partition".into(),
            ));
        }
        let source = self.source.clone();
        let session = self.session.clone();
        let template = self.template.clone();
        let cost = self.cost.clone();
        let schema = self.schema();
        let expected = schema.clone();
        let work = async move {
            let inputs = datafusion::physical_plan::collect(source, context).await?;
            tokio::task::spawn_blocking(move || {
                let dialect = session
                    .lock()
                    .map_err(|_| DataFusionError::Execution("SQL session poisoned".into()))?
                    .dialect();
                if dialect.name() != template.dialect {
                    return Err(DataFusionError::Execution(
                        "dependent operation session dialect mismatch".into(),
                    ));
                }
                let mut results = vec![];
                for batch in inputs {
                    for row in 0..batch.num_rows() {
                        let values = batch
                            .columns()
                            .iter()
                            .map(|a| ScalarValue::try_from_array(a.as_ref(), row))
                            .collect::<Result<Vec<_>>>()?;
                        let query = template
                            .bind(&values)
                            .map_err(|e| DataFusionError::Execution(e.to_string()))?;
                        let returned = session
                            .lock()
                            .map_err(|_| DataFusionError::Execution("SQL session poisoned".into()))?
                            .query(&query, expected.clone())
                            .map_err(|e| DataFusionError::Execution(e.to_string()))?;
                        if returned.num_columns() != expected.fields().len() {
                            return Err(DataFusionError::Execution(
                                "dependent operation output column count mismatch".into(),
                            ));
                        }
                        if returned
                            .columns()
                            .iter()
                            .zip(expected.fields())
                            .any(|(array, field)| !field.is_nullable() && array.null_count() != 0)
                        {
                            return Err(DataFusionError::Execution(
                                "NULL in non-nullable dependent operation output".into(),
                            ));
                        }
                        let arrays = returned
                            .columns()
                            .iter()
                            .zip(expected.fields())
                            .map(|(a, f)| coerce_sql_array(a, f.data_type()))
                            .collect::<std::result::Result<Vec<_>, _>>()?;
                        let returned = RecordBatch::try_new(expected.clone(), arrays)?;
                        {
                            let mut cost = cost.lock().map_err(|_| {
                                DataFusionError::Execution("query cost poisoned".into())
                            })?;
                            cost.sql_executions = cost.sql_executions.saturating_add(1);
                            cost.sql_output_rows = cost
                                .sql_output_rows
                                .saturating_add(returned.num_rows() as u64);
                            cost.sql_output_bytes = cost
                                .sql_output_bytes
                                .saturating_add(returned.get_array_memory_size() as u64);
                        }
                        results.push(returned);
                    }
                }
                arrow_select::concat::concat_batches(&expected, results.iter())
                    .map_err(DataFusionError::from)
            })
            .await
            .map_err(|e| DataFusionError::Execution(e.to_string()))?
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            schema,
            stream::once(work),
        )))
    }
}
