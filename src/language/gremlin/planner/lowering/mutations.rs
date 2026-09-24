//! Real graph writes; mutation IR executes through the transactional graph runtime.
use super::context::{CURRENT, Lowerer};
use super::literals::gvalue_to_expr;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{CreateEdge, CreateNode, Node, SetMode, SetPropertyItem};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_add_vertex(input: Node, label: &str, lo: &Lowerer) -> Node {
    Node::GraphCreate {
        graph: "default".into(),
        nodes: vec![CreateNode {
            bind: Some(CURRENT.into()),
            label: label.into(),
            properties: partition_properties(lo),
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
    lo: &Lowerer,
) -> Node {
    Node::GraphCreate {
        graph: "default".into(),
        nodes: vec![],
        edges: vec![CreateEdge {
            bind: Some(CURRENT.into()),
            rel_type: label.into(),
            src: from.unwrap_or(CURRENT).into(),
            dst: to.unwrap_or(CURRENT).into(),
            properties: partition_properties(lo),
        }],
        input: input.boxed(),
    }
}

pub(super) fn lower_drop(input: Node) -> Node {
    Node::GraphFilter {
        condition: IrExpr::lit_bool(false),
        input: Node::GraphDelete {
            targets: vec![IrExpr::Binding(CURRENT.into())],
            detach: true,
            input: input.boxed(),
        }
        .boxed(),
    }
}

pub(super) fn lower_property_traversal(
    input: Node,
    key: &str,
    traversal: &[crate::language::gremlin::ast::Step],
    lo: &mut super::context::Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node> {
    use crate::ir::plan::{ApplyKind, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice};
    use crate::ir::policy::OptionalMissing;
    let value = lo.fresh("property_value");
    let child = super::sub_traversal::lower_child_traversal(
        traversal,
        lo,
        ctx,
        super::context::ChildTraversalKind::ByModulator,
    )?;
    let child = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: value.clone(),
            expr: IrExpr::Binding(CURRENT.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: Node::GraphSlice {
            slice: Slice {
                offset: 0,
                fetch: Some(1),
                tail: None,
            },
            input: child.boxed(),
        }
        .boxed(),
    };
    let input = Node::GraphApply {
        kind: ApplyKind::Inner,
        correlation: vec![CURRENT.into()],
        outputs: vec![value.clone()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: child.boxed(),
    };
    Ok(Node::GraphSetProperty {
        items: vec![SetPropertyItem {
            target: IrExpr::Binding(CURRENT.into()),
            key: key.into(),
            mode: SetMode::Property,
            value: IrExpr::Binding(value),
        }],
        input: input.boxed(),
    })
}

fn partition_properties(lo: &Lowerer) -> Option<IrExpr> {
    lo.partition_write.as_ref().map(|(key, value)| {
        gvalue_to_expr(&GValue::Map([(key.clone(), value.clone())].into()))
            .expect("validated partition value")
    })
}
