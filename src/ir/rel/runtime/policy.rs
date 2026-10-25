//! Execution placement and injectable non-relational boundaries. SQL-required
//! execution is independent of the source language's semantic profile.
use super::{CompiledKernel, KernelState};
use crate::ir::{diagnostics::QueryExecutionError, runtime::Row};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoweringPolicy {
    Hybrid,
    /// SQL owns all relational processing; explicit execution boundaries remain.
    #[default]
    SqlRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryKind {
    ResultTransport,
    LiteralInput,
    /// Read the current mutable graph overlay as an Arrow source.
    GraphSource,
    Correlation,
    Barrier,
    ControlFlow,
    Mutation,
    TraversalState,
    ExternalCompute,
    Procedure,
}

/// Implementations receive an already compiled boundary, never Graph IR to
/// interpret. The default executable can be invoked explicitly for decoration.
pub trait BoundaryExecutable: fmt::Debug + Send + Sync {
    fn execute(
        &self,
        boundary: &CompiledKernel,
        inputs: Vec<Vec<Row>>,
        state: &mut KernelState,
    ) -> Result<Vec<Row>, QueryExecutionError>;
}

#[derive(Debug, Clone, Default)]
pub struct ExecutableSet {
    overrides: BTreeMap<BoundaryKind, Arc<dyn BoundaryExecutable>>,
    revision: u64,
}
impl ExecutableSet {
    pub fn with(mut self, kind: BoundaryKind, executable: Arc<dyn BoundaryExecutable>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        self.overrides.insert(kind, executable);
        self.revision = NEXT.fetch_add(1, Ordering::Relaxed);
        self
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn get(&self, kind: BoundaryKind) -> Option<Arc<dyn BoundaryExecutable>> {
        self.overrides.get(&kind).cloned()
    }
}

pub(super) fn boundary(node: &crate::ir::plan::Node) -> Option<BoundaryKind> {
    use crate::ir::plan::Node::*;
    Some(match node {
        GraphReturn { .. } => BoundaryKind::ResultTransport,
        GraphValues { .. } | GraphOneRow | GraphEmpty => BoundaryKind::LiteralInput,
        GraphCorrelate { .. } => BoundaryKind::Correlation,
        GraphBarrier { .. } => BoundaryKind::Barrier,
        GraphGroupMap { value: crate::ir::plan::GroupValue::Traversal { .. }, .. } => BoundaryKind::ControlFlow,
        GraphApply { .. } | GraphRepeat { .. } | GraphCoalesce { .. } | GraphChoose { .. } => {
            BoundaryKind::ControlFlow
        }
        GraphCreate { .. } | GraphMerge { .. } | GraphSetProperty { .. } | GraphDelete { .. } => {
            BoundaryKind::Mutation
        }
        GraphGroupSideEffect { .. }
        | GraphGroupCountSideEffect { .. }
        | GraphSideEffect { .. }
        | GraphReadSideEffect { .. }
        | GraphCap { .. }
        | GraphSample { .. } => BoundaryKind::TraversalState,
        GraphJvm { .. } => BoundaryKind::ExternalCompute,
        GraphProcedureCall { .. } => BoundaryKind::Procedure,
        _ => return None,
    })
}

pub(crate) fn validate_kernel(
    kernel: &CompiledKernel,
    policy: LoweringPolicy,
) -> Result<(), QueryExecutionError> {
    if policy == LoweringPolicy::SqlRequired && kernel.boundary_kind().is_none() {
        return Err(format!("SQL-required lowering rejected native relational kernel {}: provide SQL lowering; it is not an execution boundary",kernel.name()).into());
    }
    Ok(())
}
