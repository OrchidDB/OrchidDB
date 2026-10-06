//! Legacy connection facade for the shared mapped storage implementation.
pub(super) use crate::ir::rel::host::mapped_storage::{keys, quote, resolved_source};
use crate::ir::{
    catalog::PropertyGraph,
    rel::{
        host::{legacy::ConnectionHost, mapped_storage},
        mapping::GraphMapping,
    },
};
use duckdb::Connection;
use std::sync::Arc;
pub(super) fn query(
    connection: &Connection,
    sql: &str,
) -> Result<arrow::array::RecordBatch, String> {
    mapped_storage::query(&ConnectionHost(connection), sql)
}
pub(super) fn register(connection: &Connection) -> Result<(), String> {
    crate::ir::rel::host::legacy::register(connection)
}
pub(super) fn metadata(
    connection: &Connection,
    mapping: Arc<GraphMapping>,
) -> Result<PropertyGraph, String> {
    mapped_storage::metadata(&ConnectionHost(connection), mapping)
}
pub(super) fn persist(
    connection: &Connection,
    graph: &PropertyGraph,
    mapping: &GraphMapping,
) -> Result<(), String> {
    mapped_storage::persist(&ConnectionHost(connection), graph, mapping)
}
