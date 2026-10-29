//! Exact side-effect observations without planning five graph queries.
use super::*;

pub use crate::ir::rel::host::observation::CypherStateSnapshot;
use crate::ir::rel::host::observation::observe;
impl GraphEngine {
    /// Read one consistent state snapshot with Cypher identity/label/property
    /// semantics. Existing transactions see their own writes. For mapped graphs
    /// this explicitly reads source tables; normal query execution stays lazy.
    pub fn cypher_state_snapshot(&mut self) -> EngineResult<CypherStateSnapshot> {
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        let Some(mapping) = self.mapping.clone() else {
            self.refresh()?;
            return observe(&self.graph);
        };
        let placeholder = self.storage.try_clone().map_err(|e| e.to_string())?;
        let storage = std::mem::replace(&mut self.storage, placeholder);
        let mut connection_lease = MappedConnectionLease {
            target: &mut self.storage,
            executor: sql::DuckDbExecutor::from_connection(storage),
        };
        let automatic = !connection_lease.executor.in_transaction();
        if automatic {
            connection_lease
                .executor
                .begin()
                .map_err(|e| e.to_string())?;
        }
        let shared = Arc::new(std::sync::Mutex::new(std::mem::take(
            &mut connection_lease.executor,
        )));
        let mut executor_lease = MappedExecutorLease {
            target: &mut connection_lease.executor,
            shared: shared.clone(),
            automatic,
            finished: false,
        };
        // Snapshot identities follow id(): mapped keys retain their Arrow
        // type. Reuse the attached source instead of rescanning every table to
        // manufacture provider-prefixed display strings.
        let result = mapped_source::attach(shared.clone(), mapping.clone())
            .and_then(|graph| observe(&graph));
        executor_lease.finished = true;
        drop(executor_lease);
        if automatic {
            if result.is_ok() {
                if let Err(error) = connection_lease.executor.commit() {
                    let _ = connection_lease.executor.rollback();
                    return Err(error.to_string());
                }
            } else {
                let _ = connection_lease.executor.rollback();
            }
        }
        result
    }
}
