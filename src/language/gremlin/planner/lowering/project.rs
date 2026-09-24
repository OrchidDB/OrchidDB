//! Scalar projections: `values`, `id`, `label`, `identity`, `constant`,
//! and the `project([labels...])` step that synthesizes a map keyed by
//! its sibling `by(...)` modulators.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, PATH, TraversalContext};
use super::helpers::{apply_project_by_spec, consume_by};
use super::literals::gvalue_to_expr;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{ApplyKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, UnionAlign};
use crate::ir::policy::{OptionalMissing, PropertyMissing};
use crate::language::gremlin::ast::Step;
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_values(input: Node, keys: &[String], lo: &Lowerer) -> GremlinPlanResult<Node> {
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
    } else if keys.is_empty() {
        // `values()` (no keys) — runtime helper enumerates the bound
        // element's properties and returns a list, then `Unwind`
        // fan-outs one row per (element, property).
        let project = Node::GraphCurrentProject {
            expr: IrExpr::Call {
                name: "all_property_values".into(),
                args: vec![IrExpr::Binding(CURRENT.into())],
            },
            fields: vec![CURRENT.to_string()],
            input: input.boxed(),
        };
        Ok(Node::GraphUnwind {
            input_expr: IrExpr::Binding(CURRENT.into()),
            bind: CURRENT.into(),
            outer: false,
            input: project.boxed(),
        })
    } else {
        // values('a','b'): one row per (input, key) where the property is
        // non-null. Correlate the probes so the upstream traversal and its
        // mutations/side effects execute once, independent of key count.
        let source = Node::GraphCorrelate { bindings: vec![CURRENT.into()] };
        let mut iter = keys.iter();
        let first = iter.next().unwrap();
        let mut acc = current_project_property(source.clone(), first);
        for k in iter {
            let next = current_project_property(source.clone(), k);
            acc = Node::GraphUnion {
                all: true,
                align: UnionAlign::ByPosition,
                left: acc.boxed(),
                right: next.boxed(),
            };
        }
        Ok(Node::GraphApply {
            kind: ApplyKind::Inner,
            correlation: vec![CURRENT.into()],
            outputs: vec![CURRENT.into()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: acc.boxed(),
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
