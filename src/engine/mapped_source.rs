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
    mapped_source::attach(Arc::new(ExecutorHost(executor)), mapping)
}
