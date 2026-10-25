//! Legacy executor facade for the shared demand-driven mapped source.
use crate::ir::{
    catalog::PropertyGraph,
    rel::{
        host::{legacy::ExecutorHost, mapped_source},
        mapping::GraphMapping,
        sql::DuckDbExecutor,
    },
};
use std::sync::{Arc, Mutex};
pub(crate) fn attach(
    executor: Arc<Mutex<DuckDbExecutor>>,
    mapping: Arc<GraphMapping>,
) -> Result<PropertyGraph, String> {
    mapped_source::attach(Arc::new(ExecutorHost::new(executor)), mapping)
}

pub(crate) fn attach_for_query(
    executor: Arc<Mutex<DuckDbExecutor>>,
    mapping: Arc<GraphMapping>,
    plan: &crate::ir::plan::GraphPlan,
) -> Result<PropertyGraph, String> {
    mapped_source::attach_with_insert_defaults(Arc::new(ExecutorHost::new(executor)), mapping,
        crate::ir::analysis::validate_read_only(plan).is_err())
}
