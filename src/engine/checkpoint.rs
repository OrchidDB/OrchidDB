//! Reusable managed graph imports with a single durable encoding.
use super::{EngineResult, GraphEngine};
use crate::ir::PropertyGraph;
use std::sync::Arc;

/// An immutable managed graph and its preencoded durable representation.
///
/// Build once when repeatedly restoring the same fixture or application seed.
/// Restores still write a real SQL checkpoint and advance its revision; only
/// serialization is reused. Both fields are private so graph data and payload
/// cannot diverge. Graph clones share immutable Arrow data and copy mutations
/// on write, while the encoded payload is shared without copying its bytes.
#[derive(Clone)]
pub struct ManagedGraphCheckpoint {
    graph: PropertyGraph,
    payload: Arc<[u8]>,
}

impl ManagedGraphCheckpoint {
    pub fn new(graph: PropertyGraph) -> EngineResult<Self> {
        if graph.source.is_some() || graph.mapping.is_some() {
            return Err("a managed checkpoint cannot capture a mapped source".into());
        }
        graph.clear_pending_changes();
        let payload = crate::storage::encode_graph(&graph)?.into();
        Ok(Self { graph, payload })
    }
}

impl GraphEngine {
    /// Restore a reusable checkpoint using the same atomic replacement and
    /// transaction semantics as [`Self::replace_graph`]. It may be restored into
    /// different managed engines; later mutations never change the checkpoint.
    pub fn restore_graph_checkpoint(
        &mut self,
        checkpoint: &ManagedGraphCheckpoint,
    ) -> EngineResult<()> {
        self.replace_graph_with_payload(checkpoint.graph.clone(), Some(&checkpoint.payload))
    }
}
