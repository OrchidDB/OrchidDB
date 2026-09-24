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

pub(super) fn lower_values(input: Node, keys: &[String], _lo: &Lowerer) -> GremlinPlanResult<Node> {
    let project = Node::GraphCurrentProject {
        expr: IrExpr::Call {name:"requested_property_values".into(),args:vec![IrExpr::Binding(CURRENT.into()),IrExpr::List(keys.iter().map(|key|IrExpr::lit_str(key)).collect())]},
        fields:vec![CURRENT.into()],input:input.boxed(),
    };
    Ok(Node::GraphUnwind {input_expr:IrExpr::Binding(CURRENT.into()),bind:CURRENT.into(),outer:false,input:project.boxed()})
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
        items: vec![ProjectionItem {
            alias: CURRENT.into(),
            expr,
        }],
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
/// we emit a `CurrentProject` whose expression is `make_map(label_0,
/// key_0, label_1, key_1, ...)`.
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
    let mut entries: Vec<IrExpr> = Vec::with_capacity(labels.len() * 2);
    for label in labels {
        let spec = consume_by(steps);
        let (next_input, value_expr) = match spec {
            Some(spec) => apply_project_by_spec(input, &spec, lo, ctx)?,
            None => (input, IrExpr::Binding(CURRENT.into())),
        };
        input = next_input;
        entries.push(IrExpr::Lit(Lit::String(label.clone())));
        entries.push(value_expr);
    }
    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: if lo.productive_by {
                "make_map".into()
            } else {
                "make_project_map".into()
            },
            args: entries,
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}
