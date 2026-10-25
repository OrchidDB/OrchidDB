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

pub(super) fn persist_observed(
    connection: &Connection,
    graph: &PropertyGraph,
    mapping: &GraphMapping,
    profiling: bool,
) -> Result<(Vec<crate::ir::rel::host::WriteStatement>, crate::ir::query_cost::EngineCost), String> {
    use crate::ir::rel::host::{HostRelational, HostRequest, WriteStatement};
    #[derive(Debug)]
    struct Observed<'a> {
        host: ConnectionHost<'a>,
        profiling: bool,
        cost: Arc<std::sync::Mutex<crate::ir::query_cost::EngineCost>>,
        writes: std::cell::RefCell<Vec<WriteStatement>>,
    }
    impl HostRelational for Observed<'_> {
        fn query(&self, request: HostRequest) -> Result<arrow::array::RecordBatch, String> {
            let profile=self.host.profile(self.profiling,self.cost.clone(),&request.sql)?;
            let result=self.host.query(request)?;
            if let Some(profile)=profile {profile.finish();}
            Ok(result)
        }
        fn execute(&self, request: HostRequest) -> Result<(), String> {
            let evidence = WriteStatement { sql: request.sql.clone(), inputs: request.relations.iter().map(|input| (input.name.clone(), input.batch.num_rows())).collect() };
            let profile=self.host.profile(self.profiling,self.cost.clone(),&request.sql)?;
            self.host.execute(request)?;
            if let Some(profile)=profile {profile.finish();}
            self.writes.borrow_mut().push(evidence);
            Ok(())
        }
    }
    let host = Observed { host: ConnectionHost(connection), profiling, cost: Default::default(), writes: Default::default() };
    mapped_storage::persist(&host, graph, mapping)?;
    Ok((host.writes.into_inner(), host.cost.lock().unwrap().clone()))
}
