//! Barrier analysis for relational subplans.

use crate::ir::plan::Node;

pub(crate) fn contains_barrier(node: &Node) -> bool {
    matches!(node,
        Node::GraphAggregate { .. } | Node::GraphGroupMap { .. }
        | Node::GraphDistinct { .. } | Node::GraphSort { .. }
        | Node::GraphSlice { .. } | Node::GraphSliceExpr { .. }
        | Node::GraphBarrier { .. } | Node::GraphCap { .. }
        | Node::GraphSideEffect { eager: true, .. }
        | Node::GraphSample { kind: crate::ir::plan::SampleKind::Global(_), .. }
    ) || crate::ir::analysis::children(node).into_iter().any(contains_barrier)
}
