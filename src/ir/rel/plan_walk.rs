//! Plan walk.

use super::*;

/// Bindings consumed by the first `GraphCorrelate` leaf under `node`.
pub(super) fn first_correlate_bindings(node: &Node) -> Option<Vec<String>> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if let Node::GraphCorrelate { bindings, .. } = node {
            return Some(bindings.clone());
        }
        stack.extend(node_children(node));
    }
    None
}

pub(super) fn node_children(node: &Node) -> Vec<&Node> {
    use Node::*;
    match node {
        GraphSideEffect { value_input, input, .. } => vec![input, value_input],
        GraphGroupSideEffect { input, key_input, value, .. } => {
            let mut nodes = vec![input.as_ref(), key_input.as_ref()];
            if let crate::ir::plan::GroupValue::Traversal { traversal, .. } = value {
                nodes.push(traversal.as_ref());
            }
            nodes
        }
        GraphGroupMap { input, value, .. } => {
            let mut nodes = vec![input.as_ref()];
            if let crate::ir::plan::GroupValue::Traversal { traversal, .. } = value {
                nodes.push(traversal.as_ref());
            }
            nodes
        }
        GraphMerge {
            input,
            match_arm,
            create_arm,
            ..
        } => vec![input, match_arm, create_arm],
        GraphReturn { input, .. }
        | GraphConstructTriples { input, .. }
        | GraphDescribe { input, .. }
        | GraphAsk { input, .. }
        | GraphBind { input, .. }
        | GraphPathPattern { input, .. }
        | GraphPathFilter { input, .. }
        | GraphCreate { input, .. }
        | GraphSetProperty { input, .. }
        | GraphDelete { input, .. }
        | GraphFilter { input, .. }
        | GraphCurrentProject { input, .. }
        | GraphJvm { input, .. }
        | GraphAggregate { input, .. }
        | GraphGroupCountSideEffect { input, .. }
        | GraphReadSideEffect { input, .. }
        | GraphCap { input, .. }
        | GraphShortestPath { input, .. }
        | GraphDistinct { input, .. }
        | GraphSort { input, .. }
        | GraphSample { input, .. }
        | GraphSlice { input, .. }
        | GraphSliceExpr { input, .. }
        | GraphBarrier { input, .. }
        | GraphUnwind { input, .. }
        | GraphQuantifier { input, .. }
        | GraphCollect { input, .. }
        | GraphListComprehension { input, .. }
        | GraphSelect { input, .. }
        | GraphExpand { input, .. }
        | GraphProject { input, .. }
        | GraphService { input, .. } => vec![input],
        GraphJoin { left, right, .. }
        | GraphApply { left, right, .. }
        | GraphUnion { left, right, .. }
        | GraphSparqlMinus { left, right, .. } => vec![left, right],
        GraphRepeat {
            emit,
            seed,
            body,
            until_traversal,
            prefix_traversal,
            ..
        } => {
            let mut out = vec![seed.as_ref(), body.as_ref()];
            if let Some(node) = until_traversal {
                out.push(node);
            }
            if let Some(node) = prefix_traversal {
                out.push(node);
            }
            if let crate::ir::plan::EmitMode::AfterEachIfTraversal(traversal) = emit {
                out.push(traversal.as_ref());
            }
            out
        }
        GraphCoalesce { input, arms, .. } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter());
            out
        }
        GraphChoose {
            input,
            arms,
            default,
            ..
        } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter().map(|arm| &arm.body));
            if let Some(default) = default {
                out.push(default);
            }
            out
        }
        GraphProcedureCall { input, .. } => input.iter().map(|node| node.as_ref()).collect(),
        GraphExtension { inputs, .. } => inputs.iter().collect(),
        GraphNodeScan { .. }
        | GraphRelScan { .. }
        | GraphValues { .. }
        | GraphOneRow
        | GraphEmpty
        | GraphCorrelate { .. }
        | GraphSparqlTriplePattern { .. }
        | GraphSparqlGraphNames { .. }
        | GraphRdfPropertyPath { .. } => Vec::new(),
    }
}

pub(super) fn unsupported_node_name(node: &Node) -> &'static str {
    match node {
        Node::GraphCorrelate { .. } => "GraphCorrelate",
        Node::GraphSparqlTriplePattern { .. } => "GraphSparqlTriplePattern",
        Node::GraphSparqlGraphNames { .. } => "GraphSparqlGraphNames",
        Node::GraphPathPattern { .. } => "GraphPathPattern",
        Node::GraphRdfPropertyPath { .. } => "GraphRdfPropertyPath",
        Node::GraphRepeat { .. } => "GraphRepeat",
        Node::GraphPathFilter { .. } => "GraphPathFilter",
        Node::GraphCreate { .. } => "GraphCreate",
        Node::GraphMerge { .. } => "GraphMerge",
        Node::GraphSetProperty { .. } => "GraphSetProperty",
        Node::GraphDelete { .. } => "GraphDelete",
        Node::GraphGroupMap { .. } => "GraphGroupMap",
        Node::GraphGroupSideEffect { .. } => "GraphGroupSideEffect",
        Node::GraphGroupCountSideEffect { .. } => "GraphGroupCountSideEffect",
        Node::GraphSideEffect { .. } => "GraphSideEffect",
        Node::GraphReadSideEffect { .. } => "GraphReadSideEffect",
        Node::GraphCap { .. } => "GraphCap",
        Node::GraphShortestPath { .. } => "GraphShortestPath",
        Node::GraphSliceExpr { .. } => "GraphSliceExpr",
        Node::GraphSample { .. } => "GraphSample",
        Node::GraphBarrier { .. } => "GraphBarrier",
        Node::GraphApply { .. } => "GraphApply",
        Node::GraphUnwind { .. } => "GraphUnwind",
        Node::GraphQuantifier { .. } => "GraphQuantifier",
        Node::GraphCollect { .. } => "GraphCollect",
        Node::GraphListComprehension { .. } => "GraphListComprehension",
        Node::GraphCoalesce { .. } => "GraphCoalesce",
        Node::GraphChoose { .. } => "GraphChoose",
        Node::GraphSelect { .. } => "GraphSelect",
        Node::GraphSparqlMinus { .. } => "GraphSparqlMinus",
        Node::GraphService { .. } => "GraphService",
        Node::GraphProcedureCall { .. } => "GraphProcedureCall",
        Node::GraphExtension { .. } => "GraphExtension",
        Node::GraphConstructTriples { .. } => "GraphConstructTriples",
        Node::GraphDescribe { .. } => "GraphDescribe",
        Node::GraphAsk { .. } => "GraphAsk",
        _ => "Graph IR node",
    }
}
