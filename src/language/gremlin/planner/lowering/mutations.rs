//! Real graph writes; mutation IR executes through the transactional graph runtime.
use super::context::CURRENT;
use super::literals::gvalue_to_expr;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{CreateEdge, CreateNode, Node, SetMode, SetPropertyItem};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_add_vertex(input: Node, label: &str) -> Node {
    Node::GraphCreate {
        graph: "default".into(),
        nodes: vec![CreateNode {
            bind: Some(CURRENT.into()),
            label: label.into(),
            properties: None,
        }],
        edges: vec![],
        input: input.boxed(),
    }
}

pub(super) fn lower_property(input: Node, key: &str, value: &GValue) -> GremlinPlanResult<Node> {
    Ok(Node::GraphSetProperty {
        items: vec![SetPropertyItem {
            target: IrExpr::Binding(CURRENT.into()),
            key: key.into(),
            mode: SetMode::Property,
            value: gvalue_to_expr(value)?,
        }],
        input: input.boxed(),
    })
}

pub(super) fn lower_add_edge(
    input: Node,
    label: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Node {
    Node::GraphCreate {
        graph: "default".into(),
        nodes: vec![],
        edges: vec![CreateEdge {
            bind: Some(CURRENT.into()),
            rel_type: label.into(),
            src: from.unwrap_or(CURRENT).into(),
            dst: to.unwrap_or(CURRENT).into(),
            properties: None,
        }],
        input: input.boxed(),
    }
}
