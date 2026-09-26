//! Test-only adapters to the production DataFusion DAG executor.
#![allow(dead_code)]
use orchiddb::ir::runtime::{ReturnedBatches, Row};
use orchiddb::ir::{GraphPlan, PropertyGraph};

pub async fn execute_async(
    plan: &GraphPlan,
    graph: &PropertyGraph,
) -> Result<ReturnedBatches, String> {
    orchiddb::ir::rel::runtime::execute(plan, graph, None)
        .await
        .map(|(returned, _)| returned)
}
pub async fn execute_rows_async(
    plan: &GraphPlan,
    graph: &PropertyGraph,
) -> Result<Vec<Row>, String> {
    orchiddb::ir::rel::runtime::execute_rows_with_jvm(plan, graph, Default::default())
        .await
        .map(|(rows, _)| rows)
}
pub fn execute(plan: &GraphPlan, graph: &PropertyGraph) -> Result<ReturnedBatches, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(execute_async(plan, graph))
}
pub fn execute_rows(plan: &GraphPlan, graph: &PropertyGraph) -> Result<Vec<Row>, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(execute_rows_async(plan, graph))
}
