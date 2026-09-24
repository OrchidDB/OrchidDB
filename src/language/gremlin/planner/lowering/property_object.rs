//! Property-object projections: `valueMap`, `elementMap`, `propertyMap`,
//! `properties`, `valueMapTokens`.
//!
//! Each step lowers to an `IrExpr::Call` whose runtime helper inspects
//! the bound element's catalog row to build the requested shape. We
//! rely on the interpreter's catalog access — at plan time the keys
//! list is already known (or empty, meaning "all keys").
//!
//! `valueMap`/`elementMap`/`propertyMap`/`valueMapTokens` produce a
//! single Map row per input. `properties()` fans out (one row per
//! `(element, key)` pair) and is therefore wrapped in `GraphUnwind`.

use super::context::{CURRENT, Lowerer, TraversalContext};
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{ApplyKind, Node, UnionAlign};
use crate::ir::policy::OptionalMissing;
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_value_map(input: Node, keys: &[String]) -> Node {
    project_map(input, "value_map", keys)
}

pub(super) fn lower_value_map_tokens(
    input: Node,
    keys: &[String],
    include_id: bool,
    include_label: bool,
    unfold_values: bool,
) -> Node {
    project_map_with_options(
        input,
        "value_map_tokens",
        keys,
        include_id,
        include_label,
        unfold_values,
    )
}

pub(super) fn lower_element_map(input: Node, keys: &[String]) -> Node {
    project_map(input, "element_map", keys)
}

pub(super) fn lower_property_map(input: Node, keys: &[String]) -> Node {
    project_map(input, "property_map", keys)
}

pub(super) fn lower_element(input: Node) -> Node {
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "property_element".to_string(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

/// `properties(keys...)` — fan-out: one row per (current, key) pair
/// where the property exists. Returns a list traverser via
/// `GraphUnwind` over a list-shaped helper.
pub(super) fn lower_properties(
    input: Node,
    keys: &[String],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let project = project_map(input, "properties_list", keys);
    let unwound = Node::GraphUnwind {
        input_expr: IrExpr::Binding(CURRENT.into()),
        bind: CURRENT.into(),
        outer: false,
        input: project.boxed(),
    };
    filter_vertex_properties(unwound, lo, ctx)
}

/// SubgraphStrategy evaluates its child traversal against native vertex
/// properties. Edge and meta-properties retain their ordinary visibility.
fn filter_vertex_properties(
    input: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let Some(predicate) = lo.subgraph_vertex_property_filter.take() else {
        return Ok(input);
    };
    let is_vertex_property = IrExpr::Binary {
        op: crate::ir::expr::BinaryOp::Eq,
        lhs: Box::new(IrExpr::Call {
            name: "element_kind".into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        }),
        rhs: Box::new(IrExpr::lit_str("VertexProperty")),
    };
    let native = Node::GraphFilter {
        condition: is_vertex_property.clone(),
        input: Node::GraphCorrelate {
            bindings: vec![CURRENT.into()],
        }
        .boxed(),
    };
    // Strategy evaluation itself must not recursively apply the same strategy.
    let filtered = super::filter::lower_where_traversal(native, &predicate, lo, ctx);
    lo.subgraph_vertex_property_filter = Some(predicate);
    Ok(Node::GraphApply {
        kind: ApplyKind::Semi,
        correlation: vec![CURRENT.into()],
        outputs: vec![],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: Node::GraphUnion {
            all: true,
            align: UnionAlign::ByPosition,
            left: Node::GraphFilter {
                condition: IrExpr::Not(Box::new(is_vertex_property)),
                input: Node::GraphCorrelate {
                    bindings: vec![CURRENT.into()],
                }
                .boxed(),
            }
            .boxed(),
            right: filtered?.boxed(),
        }
        .boxed(),
    })
}

pub(super) fn lower_properties_value(
    input: Node,
    keys: &[String],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "property_value".into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        },
        fields: vec![CURRENT.to_string()],
        input: lower_properties(input, keys, lo, ctx)?.boxed(),
    })
}

fn project_map(input: Node, helper: &str, keys: &[String]) -> Node {
    project_map_with_options(input, helper, keys, true, true, false)
}

fn project_map_with_options(
    input: Node,
    helper: &str,
    keys: &[String],
    include_id: bool,
    include_label: bool,
    unfold_values: bool,
) -> Node {
    let key_list = IrExpr::List(
        keys.iter()
            .map(|k| IrExpr::Lit(Lit::String(k.clone())))
            .collect(),
    );
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: helper.to_string(),
            args: vec![
                IrExpr::Binding(CURRENT.into()),
                key_list,
                IrExpr::Lit(Lit::Bool(include_id)),
                IrExpr::Lit(Lit::Bool(include_label)),
                IrExpr::Lit(Lit::Bool(unfold_values)),
            ],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}
