//! Real graph writes; mutation IR executes through the transactional graph runtime.
use super::context::{CURRENT, Lowerer};
use super::literals::gvalue_to_expr;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{CreateEdge, CreateNode, Node, SetMode, SetPropertyItem};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_add_vertex(input: Node, label: &str, lo: &Lowerer) -> Node {
    write_call(input,"gremlin.mutation.add_vertex",vec![IrExpr::lit_str(label),partition_properties(lo).unwrap_or(IrExpr::Lit(crate::ir::expr::Lit::Null))])
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

pub(super) fn argument(
    input: Node,
    value: &crate::language::gremlin::ast::MutationArgument,
    lo: &mut Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<(Node, IrExpr)> {
    use crate::ir::plan::{ApplyKind, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice};
    use crate::ir::policy::OptionalMissing;
    use crate::language::gremlin::ast::{MutationArgument, Pop, Step};
    let steps = match value {
        MutationArgument::Literal(value) => return Ok((input, gvalue_to_expr(value)?)),
        MutationArgument::Traversal(steps) => steps.clone(),
        MutationArgument::Label(label) => vec![Step::Select(label.clone(), Pop::Last)],
    };
    let binding = lo.fresh("mutation_argument");
    let child = super::sub_traversal::lower_child_traversal(
        &steps,
        lo,
        ctx,
        super::context::ChildTraversalKind::ByModulator,
    )?;
    let child = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: binding.clone(),
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
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Inner,
            correlation: vec![CURRENT.into()],
            outputs: vec![binding.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: child.boxed(),
        },
        IrExpr::Binding(binding),
    ))
}

pub(super) fn write_call(input: Node, name: &str, args: Vec<IrExpr>) -> Node {
    Node::GraphProcedureCall {
        name: name.into(),
        args: args
            .into_iter()
            .map(|value| crate::ir::plan::ProcedureArg { name: None, value })
            .collect(),
        yields: vec![CURRENT.into()],
        mode: crate::ir::plan::ProcedureMode::Write,
        input: Some(input.boxed()),
    }
}

pub(super) fn lower_dynamic_vertex(
    input: Node,
    label: &crate::language::gremlin::ast::MutationArgument,
    lo: &mut Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node> {
    let (input, label) = argument(input, label, lo, ctx)?;
    Ok(write_call(
        input,
        "gremlin.mutation.add_vertex",
        vec![
            label,
            partition_properties(lo).unwrap_or(IrExpr::Lit(crate::ir::expr::Lit::Null)),
        ],
    ))
}

pub(super) fn lower_dynamic_edge(
    input: Node,
    label: &crate::language::gremlin::ast::MutationArgument,
    from: Option<&crate::language::gremlin::ast::MutationArgument>,
    to: Option<&crate::language::gremlin::ast::MutationArgument>,
    lo: &mut Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node> {
    let (input, label) = argument(input, label, lo, ctx)?;
    let (input, src) = if let Some(from) = from {
        argument(input, from, lo, ctx)?
    } else {
        (input, IrExpr::Binding(CURRENT.into()))
    };
    let (input, dst) = if let Some(to) = to {
        argument(input, to, lo, ctx)?
    } else {
        (input, IrExpr::Binding(CURRENT.into()))
    };
    Ok(write_call(
        input,
        "gremlin.mutation.add_edge",
        vec![
            label,
            src,
            dst,
            partition_properties(lo).unwrap_or(IrExpr::Lit(crate::ir::expr::Lit::Null)),
        ],
    ))
}

pub(super) fn lower_dynamic_property(
    input: Node,
    key: &crate::language::gremlin::ast::MutationArgument,
    value: &crate::language::gremlin::ast::MutationArgument,
    lo: &mut Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node> {
    let (input, key) = argument(input, key, lo, ctx)?;
    let (input, value) = argument(input, value, lo, ctx)?;
    Ok(write_call(
        input,
        "gremlin.mutation.property",
        vec![IrExpr::Binding(CURRENT.into()), key, value],
    ))
}

pub(super) fn lower_native_property(input:Node,cardinality:&str,key:&crate::language::gremlin::ast::MutationArgument,value:&crate::language::gremlin::ast::MutationArgument,meta:&[(String,GValue)],lo:&mut Lowerer,ctx:&super::context::TraversalContext)->GremlinPlanResult<Node>{
    let (input,key)=argument(input,key,lo,ctx)?;
    let (input,value)=argument(input,value,lo,ctx)?;
    let meta=gvalue_to_expr(&GValue::Map(meta.iter().cloned().collect()))?;
    Ok(write_call(input,"gremlin.mutation.property_native",vec![IrExpr::Binding(CURRENT.into()),key,value,IrExpr::lit_str(cardinality),meta]))
}
