//! Scalar projections: `values`, `id`, `label`, `identity`, `constant`,
//! and the `project([labels...])` step that synthesizes a map keyed by
//! its sibling `by(...)` modulators.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, PATH, TraversalContext};
use super::helpers::{apply_project_by_spec, consume_by};
use super::literals::gvalue_to_expr;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::language::gremlin::ast::Step;
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_values(input: Node, keys: &[String], lo: &mut Lowerer, ctx: &TraversalContext) -> GremlinPlanResult<Node> {
    if lo.subgraph_vertex_property_filter.is_some() {
        return super::property_object::lower_properties_value(input, keys, lo, ctx);
    }
    // Fan out native records without coercion or replaying the input traversal.
    let value = lo.fresh("property_value");
    let unwound = Node::GraphUnwind {
        input_expr: IrExpr::Call {
            name: "requested_property_values".into(),
            args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::List(keys.iter().map(IrExpr::lit_str).collect())],
        },
        bind: value.clone(),
        outer: false,
        input: input.boxed(),
    };
    Ok(project_value_with_path(unwound, IrExpr::Binding(value)))
}

pub(super) fn project_value_with_path(input: Node, value: IrExpr) -> Node {
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![
            ProjectionItem { alias: CURRENT.into(), expr: value.clone() },
            ProjectionItem { alias: PATH.into(), expr: IrExpr::Call {
                name: "path_append_after".into(),
                args: vec![IrExpr::Binding(PATH.into()), IrExpr::Binding(CURRENT.into()), value],
            } },
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
/// whose entries are `{label: by-key}`. Trailing `by(...)` modulators
/// form a traversal ring, cycled across the keys and reset for each row.
/// An empty ring defaults to `current`.
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
    let mut modulators = Vec::new();
    while let Some(spec) = consume_by(steps) {
        modulators.push(spec);
    }
    for (index, label) in labels.iter().enumerate() {
        let spec = (!modulators.is_empty()).then(|| &modulators[index % modulators.len()]);
        let (next_input, value_expr, productive) = match spec {
            Some(spec) => apply_project_by_spec(input, spec, lo, ctx)?,
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
