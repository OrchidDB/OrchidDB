//! Scalar projections: `values`, `id`, `label`, `identity`, `constant`,
//! and the `project([labels...])` step that synthesizes a map keyed by
//! its sibling `by(...)` modulators.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, PATH, TraversalContext};
use super::helpers::{apply_project_by_spec, consume_by};
use super::literals::gvalue_to_expr;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::Step;
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_values(
    input: Node,
    keys: &[String],
    lo: &mut Lowerer,
) -> GremlinPlanResult<Node> {
    if keys.len() == 1 {
        if has_vertex_property_filter(lo) && keys[0] == "location" {
            let project = Node::GraphCurrentProject {
                expr: IrExpr::Call {
                    name: "gremlin_visible_vertex_property_values".into(),
                    args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::lit_str("location")],
                },
                fields: vec![CURRENT.to_string()],
                input: input.boxed(),
            };
            return Ok(Node::GraphUnwind {
                input_expr: IrExpr::Binding(CURRENT.into()),
                bind: CURRENT.into(),
                outer: false,
                input: project.boxed(),
            });
        }
        Ok(current_project_property(input, &keys[0]))
    } else {
        // A relational UNION coerces heterogeneous property columns to one
        // SQL type. Keep each requested value native and fan out once per
        // input traverser instead; this also avoids replaying mutations.
        let value = lo.fresh("property_value");
        let unwound = Node::GraphUnwind {
            input_expr: IrExpr::Call {
                name: "requested_property_values".into(),
                args: vec![
                    IrExpr::Binding(CURRENT.into()),
                    IrExpr::List(keys.iter().map(|key| IrExpr::lit_str(key)).collect()),
                ],
            },
            bind: value.clone(),
            outer: false,
            input: input.boxed(),
        };
        Ok(Node::GraphProject {
            mode: ProjectMode::ReplaceCurrent,
            items: vec![
                ProjectionItem {
                    alias: CURRENT.into(),
                    expr: IrExpr::Binding(value.clone()),
                },
                ProjectionItem {
                    alias: PATH.into(),
                    expr: IrExpr::Call {
                        name: "path_append_after".into(),
                        args: vec![
                            IrExpr::Binding(PATH.into()),
                            IrExpr::Binding(CURRENT.into()),
                            IrExpr::Binding(value),
                        ],
                    },
                },
            ],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: unwound.boxed(),
        })
    }
}

fn has_vertex_property_filter(lo: &Lowerer) -> bool {
    lo.subgraph_vertex_property_filter.is_some()
}

fn current_project_property(input: Node, key: &str) -> Node {
    let expr = IrExpr::property(CURRENT, key.to_string(), PropertyMissing::DropUnproductive);
    let input = Node::GraphFilter {
        condition: IrExpr::IsNotNull(Box::new(expr.clone())),
        input: input.boxed(),
    };
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![
            ProjectionItem {
                alias: CURRENT.into(),
                expr: expr.clone(),
            },
            ProjectionItem {
                alias: PATH.into(),
                expr: IrExpr::Call {
                    name: "path_append_after".into(),
                    args: vec![
                        IrExpr::Binding(PATH.into()),
                        IrExpr::Binding(CURRENT.into()),
                        expr,
                    ],
                },
            },
        ],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

pub(super) fn lower_id(input: Node) -> Node {
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "gremlin_id".into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

pub(super) fn lower_label(input: Node) -> Node {
    Node::GraphCurrentProject {
        expr: IrExpr::Label(CURRENT.into()),
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

pub(super) fn lower_constant(input: Node, value: &GValue) -> GremlinPlanResult<Node> {
    let expr = gvalue_to_expr(value)?;
    Ok(Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem { alias: CURRENT.into(), expr: expr.clone() },
            ProjectionItem { alias: PATH.into(), expr: IrExpr::Call {
                name: "path_extend_after".into(),
                args: vec![IrExpr::Binding(PATH.into()), IrExpr::Binding(CURRENT.into()), expr],
            }}],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    })
}

/// `project("a", "b", ...)` — fan one input row into a single map row
/// whose entries are `{label: by-key}`. We consume up to N trailing
/// `by(...)` modulators (one per label); missing modulators default to
/// `current`.
///
/// Each `by(__.t)` lowers via `apply_by_spec` to a fresh probe binding
/// joined onto the input via `Apply Optional`. Once all keys resolve,
/// each child also returns a productivity flag. This distinguishes a valid
/// null result from an absent result when building the projected map.
pub(super) fn lower_project<'a, I>(
    input: Node,
    labels: &[String],
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let mut input = input;
    let mut entries: Vec<IrExpr> = Vec::with_capacity(labels.len() * 3);
    for label in labels {
        let spec = consume_by(steps);
        let (next_input, value_expr, productive) = match spec {
            Some(spec) => apply_project_by_spec(input, &spec, lo, ctx)?,
            None => (input, IrExpr::Binding(CURRENT.into()), IrExpr::lit_bool(true)),
        };
        input = next_input;
        entries.push(IrExpr::Lit(Lit::String(label.clone())));
        entries.push(value_expr);
        entries.push(productive);
    }
    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "make_project_map_productive".into(),
            args: entries,
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}
