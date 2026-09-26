//! Stream stage analysis for relational bounded execution.

use crate::ir::analysis::{children, node_effect, Effect};
use crate::ir::plan::Node;

pub(crate) fn range_movable(node: &Node) -> bool {
    matches!(
        node,
        Node::GraphProject { .. }
            | Node::GraphBind { .. }
            | Node::GraphSelect { .. }
            | Node::GraphReadSideEffect { .. }
            | Node::GraphSideEffect { eager: false, .. }
            | Node::GraphSetProperty { .. }
            | Node::GraphCreate { .. }
    ) || matches!(node, Node::GraphCurrentProject { expr: crate::ir::expr::IrExpr::Call { name, .. }, .. }
        if matches!(name.as_str(), "make_project_map_productive" | "tree_value"))
        || matches!(node, Node::GraphProcedureCall { name, .. }
        if name == "gremlin.mutation.add_vertex")
}

fn pure(node: &Node) -> bool {
    node_effect(node) == Effect::Pure && children(node).into_iter().all(pure)
}

pub(crate) fn take_stream_input(node: &mut Node) -> Option<Box<Node>> {
    let input = match node {
        Node::GraphFilter { input, .. }
        | Node::GraphProject { input, .. }
        | Node::GraphCurrentProject { input, .. }
        | Node::GraphBind { input, .. }
        | Node::GraphExpand { input, .. }
        | Node::GraphUnwind { input, .. }
        | Node::GraphSelect { input, .. }
        | Node::GraphReadSideEffect { input, .. }
        | Node::GraphSetProperty { input, .. }
        | Node::GraphCreate { input, .. }
        | Node::GraphDelete { input, .. } => input,
        Node::GraphProcedureCall {
            name,
            input: Some(input),
            ..
        } if name.starts_with("gremlin.mutation.") => input,
        // Apply children already have per-traverser scope. Pure children can
        // materialize their own results without executing unconsumed effects.
        Node::GraphApply { left, right, .. } if pure(right) => left,
        Node::GraphChoose {
            input,
            arms,
            default,
            ..
        } if arms
            .iter()
            .all(|arm| pure(&arm.body) && !super::choose::contains_barrier(&arm.body))
            && default
                .as_deref()
                .is_none_or(|body| pure(body) && !super::choose::contains_barrier(body)) =>
        {
            input
        }
        Node::GraphSideEffect {
            input,
            eager: false,
            ..
        } => input,
        _ => return None,
    };
    Some(std::mem::replace(
        input,
        Node::GraphCorrelate {
            bindings: vec!["__gremlin_group_members".into()],
        }
        .boxed(),
    ))
}
