//! Existing embedded-DuckDB transport. The extension does not compile this module.
use super::mapped_storage::GraphMapping;
use super::*;
use crate::ir::rel::sql::DuckDbExecutor;
use duckdb::{
    Connection,
    vtab::{arrow::ArrowVTab, arrow_recordbatch_to_query_params},
};
use std::sync::Mutex;
#[derive(Debug)]
pub(crate) struct ConnectionHost<'a>(pub &'a Connection);
#[derive(Debug)]
pub(crate) struct ExecutorHost(pub Arc<Mutex<DuckDbExecutor>>);
pub(crate) fn register(connection: &Connection) -> Result<(), String> {
    let exists: i64 = connection
        .query_row(
            "SELECT count(*) FROM duckdb_functions() WHERE function_name='__orchiddb_write_values'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists == 0 {
        connection
            .register_table_function::<ArrowVTab>("__orchiddb_write_values")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn request_sql(connection: &Connection, request: HostRequest) -> Result<String, String> {
    let mut sql = if request.parameters.is_empty() {
        request.sql
    } else {
        super::super::sql::lowering::SqlTemplate {
            sql: request.sql,
            parameters: request.parameters.len(),
            dialect: "duckdb".into(),
        }
        .bind(&request.parameters)
        .map_err(|e| e.to_string())?
    };
    if !request.relations.is_empty() {
        register(connection)?;
        let ctes = request
            .relations
            .into_iter()
            .map(|relation| {
                let pointers = arrow_recordbatch_to_query_params(relation.batch);
                format!(
                    "{} AS (SELECT * FROM __orchiddb_write_values({}::UBIGINT, {}::UBIGINT))",
                    super::mapped_storage::quote(&relation.name),
                    pointers[0],
                    pointers[1]
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let trimmed = sql.trim_start();
        sql = if trimmed
            .get(..15)
            .is_some_and(|s| s.eq_ignore_ascii_case("WITH RECURSIVE "))
        {
            format!("WITH RECURSIVE {ctes},{}", &trimmed[15..])
        } else if trimmed
            .get(..5)
            .is_some_and(|s| s.eq_ignore_ascii_case("WITH "))
        {
            format!("WITH {ctes},{}", &trimmed[5..])
        } else {
            format!("WITH {ctes} {sql}")
        };
    }
    Ok(sql)
}
impl HostRelational for ConnectionHost<'_> {
    fn query(&self, request: HostRequest) -> Result<RecordBatch, String> {
        let sql = request_sql(self.0, request)?;
        let mut statement = self.0.prepare(&sql).map_err(|e| e.to_string())?;
        let reader = statement.query_arrow([]).map_err(|e| e.to_string())?;
        let schema = reader.get_schema();
        arrow::compute::concat_batches(&schema, &reader.collect::<Vec<_>>())
            .map_err(|e| e.to_string())
    }
    fn execute(&self, request: HostRequest) -> Result<(), String> {
        let sql = request_sql(self.0, request)?;
        self.0
            .execute(&sql, [])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
impl HostRelational for ExecutorHost {
    fn query(&self, request: HostRequest) -> Result<RecordBatch, String> {
        let mut executor = self.0.lock().map_err(|e| e.to_string())?;
        ConnectionHost(executor.connection().map_err(|e| e.to_string())?).query(request)
    }
    fn execute(&self, request: HostRequest) -> Result<(), String> {
        let mut executor = self.0.lock().map_err(|e| e.to_string())?;
        ConnectionHost(executor.connection().map_err(|e| e.to_string())?).execute(request)
    }
    fn execute_plan(
        &self,
        mapping: &GraphMapping,
        plan: datafusion::logical_expr::LogicalPlan,
    ) -> Result<(RecordBatch, Vec<String>), String> {
        execute_plan(self.0.clone(), mapping, plan)
    }
    fn legacy_executor(&self) -> Option<Arc<Mutex<DuckDbExecutor>>> {
        Some(self.0.clone())
    }
}
/// Bridge synchronous graph-source requests into the existing executable DAG.
/// Placement happens before execution; backend failures are never retried.
fn execute_plan(
    executor: Arc<std::sync::Mutex<crate::ir::rel::sql::DuckDbExecutor>>,
    mapping: &GraphMapping,
    plan: datafusion::logical_expr::LogicalPlan,
) -> Result<(RecordBatch, Vec<String>), String> {
    let external = mapping.physical_table_names();
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?;
                runtime.block_on(async move {
                    let session = crate::ir::rel::dag::DagSession::with_shared(executor, external);
                    let lowered = crate::ir::rel::LoweredPlan {
                        fields: plan
                            .schema()
                            .fields()
                            .iter()
                            .map(|f| f.name().clone())
                            .collect(),
                        plan,
                        result_form: crate::ir::policy::ResultForm::RowSet,
                        islands: Default::default(),
                    };
                    let (result, stats) = crate::ir::rel::dag::execute_with_extensions(
                        lowered,
                        vec![],
                        None,
                        Some(&session),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                    Ok((result.batch, stats.sql_queries))
                })
            })
            .join()
            .map_err(|_| "computed relationship worker panicked".to_string())?
    })
}
