//! Real graph writes; mutation IR executes through the transactional graph runtime.
use super::context::{CURRENT, Lowerer};
use super::literals::gvalue_to_expr;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{Node, SetMode, SetPropertyItem};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_add_vertex(input: Node, label: &str, lo: &Lowerer) -> Node {
    write_call(
        input,
        "gremlin.mutation.add_vertex",
        vec![
            IrExpr::lit_str(label),
            partition_properties(lo).unwrap_or(IrExpr::Lit(crate::ir::expr::Lit::Null)),
        ],
    )
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

pub(super) fn lower_native_property(
    input: Node,
    cardinality: &str,
    key: &crate::language::gremlin::ast::MutationArgument,
    value: &crate::language::gremlin::ast::MutationArgument,
    meta: &[(String, GValue)],
    lo: &mut Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node> {
    let (input, key) = argument(input, key, lo, ctx)?;
    let (input, value) = argument(input, value, lo, ctx)?;
    let meta = gvalue_to_expr(&GValue::Map(meta.iter().cloned().collect()))?;
    Ok(write_call(
        input,
        "gremlin.mutation.property_native",
        vec![
            IrExpr::Binding(CURRENT.into()),
            key,
            value,
            IrExpr::lit_str(if cardinality == "default" {
                "single"
            } else {
                cardinality
            }),
            meta,
        ],
    ))
}

/// Fold addV property parameters exactly where GraphTraversal.property does:
/// their child traversals observe the incoming object, before vertex creation.
pub(super) fn lower_vertex_with_properties<'a, I>(
    input: Node,
    label: &crate::language::gremlin::ast::MutationArgument,
    steps: &mut std::iter::Peekable<I>,
    lo: &mut Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a crate::language::gremlin::ast::Step>,
{
    let node = lower_dynamic_vertex(input, label, lo, ctx)?;
    lower_creation_with_properties(node, steps, lo, ctx)
}

pub(super) fn lower_creation_with_properties<'a, I>(
    mut node: Node,
    steps: &mut std::iter::Peekable<I>,
    lo: &mut Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a crate::language::gremlin::ast::Step>,
{
    use crate::ir::plan::{ProjectErrorPolicy, ProjectMode, ProjectionItem};
    use crate::language::gremlin::ast::{MutationArgument, Step};
    let edge_creation =
        matches!(&node,Node::GraphProcedureCall{name,..} if name=="gremlin.mutation.add_edge");
    let Node::GraphProcedureCall {
        input: ref mut source,
        ..
    } = node
    else {
        unreachable!("creation procedure");
    };
    let mut input = *source.take().expect("creation input");
    let mut parameters = Vec::new();
    let mut remaining = Vec::new();
    while matches!(
        steps.peek(),
        Some(Step::PropertyNative { .. } | Step::As(_))
    ) {
        let step = steps.next().expect("peeked a property or label");
        let Step::PropertyNative {
            cardinality,
            key,
            value,
            meta,
        } = step
        else {
            remaining.push(step);
            continue;
        };
        let foldable = (!edge_creation || matches!(value, MutationArgument::Literal(_)))
            && meta.is_empty()
            && (matches!(
                key,
                MutationArgument::Traversal(_) | MutationArgument::Literal(GValue::Token(_))
            ) || (cardinality == "default"
                && matches!(key, MutationArgument::Literal(GValue::String(_)))));
        if !foldable {
            remaining.push(step);
            continue;
        }
        let (next, key) = argument(input, key, lo, ctx)?;
        let (next, value) = argument(next, value, lo, ctx)?;
        let key_binding = lo.fresh("vertex_property_key");
        let value_binding = lo.fresh("vertex_property_value");
        input = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![
                ProjectionItem {
                    alias: key_binding.clone(),
                    expr: key,
                },
                ProjectionItem {
                    alias: value_binding.clone(),
                    expr: value,
                },
            ],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: next.boxed(),
        };
        parameters.push((IrExpr::Binding(key_binding), IrExpr::Binding(value_binding)));
    }
    let Node::GraphProcedureCall {
        args,
        input: source,
        ..
    } = &mut node
    else {
        unreachable!();
    };
    *source = Some(input.boxed());
    args.push(crate::ir::plan::ProcedureArg {
        name: None,
        value: IrExpr::List(
            parameters
                .iter()
                .map(|(key, value)| IrExpr::List(vec![key.clone(), value.clone()]))
                .collect(),
        ),
    });
    for (key, value) in parameters {
        node = write_call(
            node,
            "gremlin.mutation.property_native",
            vec![
                IrExpr::Binding(CURRENT.into()),
                key,
                value,
                IrExpr::lit_str("list"),
                gvalue_to_expr(&GValue::Map(Default::default()))?,
                IrExpr::Lit(crate::ir::expr::Lit::Bool(true)),
            ],
        );
    }
    let mut remaining = remaining.into_iter().peekable();
    while let Some(step) = remaining.next() {
        node = super::dispatch::lower_step_with_context(node, step, &mut remaining, lo, ctx)?;
    }
    Ok(node)
}
