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

use std::iter::Peekable;

use super::context::{CURRENT, PATH, Lowerer, TraversalContext};
use super::helpers::{apply_project_by_spec, consume_by};
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{
    ApplyKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, UnionAlign,
};
use crate::ir::policy::{OptionalMissing, PropertyMissing};
use crate::language::gremlin::ast::Step;
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

/// Apply the traversal ring to each map entry, including requested tokens.
/// Each input map has its own correlated fold, so dropping every entry still
/// emits an empty map and missing properties do not consume a ring position.
pub(super) fn lower_value_map_modulators<'a, I>(
    input: Node,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let mut modulators = Vec::new();
    while let Some(spec) = consume_by(steps) {
        modulators.push(spec);
    }
    if modulators.is_empty() {
        return Ok(input);
    }
    let entry = lo.fresh("value_map_entry");
    let result = lo.fresh("value_map_result");
    let mut branches = Vec::new();
    for (slot, spec) in modulators.iter().enumerate() {
        let entries = Node::GraphUnwind {
            input_expr: IrExpr::Call {
                name: "value_map_modulator_entries".into(),
                args: vec![
                    IrExpr::Binding(CURRENT.into()),
                    IrExpr::Lit(Lit::Int(slot as i64)),
                    IrExpr::Lit(Lit::Int(modulators.len() as i64)),
                ],
            },
            bind: entry.clone(),
            outer: false,
            input: Node::GraphCorrelate {
                bindings: vec![CURRENT.into()],
            }
            .boxed(),
        };
        let entry_field = |name: &str| {
            IrExpr::property(
                entry.clone(),
                name.to_string(),
                PropertyMissing::NullOnMissing,
            )
        };
        let values = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: entry_field("value"),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: entries.boxed(),
        };
        // Preserve the entry key and distinguish an absent result from a
        // productive null, using the shared by-modulator contract.
        let (projected, value, productive) = apply_project_by_spec(values, spec, lo, ctx)?;
        branches.push(Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::List(vec![entry_field("key"), value, productive]),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: projected.boxed(),
        });
    }
    let mut branches = branches.into_iter();
    let mut combined = branches.next().expect("nonempty traversal ring");
    for branch in branches {
        combined = Node::GraphUnion {
            all: true,
            align: UnionAlign::ByPosition,
            left: combined.boxed(),
            right: branch.boxed(),
        };
    }
    let collected = Node::GraphProject {
        mode: ProjectMode::ReplaceScope,
        items: vec![ProjectionItem {
            alias: result.clone(),
            expr: IrExpr::Binding(CURRENT.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: super::reduce::lower_fold(combined).boxed(),
    };
    let applied = Node::GraphApply {
        kind: ApplyKind::Scalar,
        correlation: vec![],
        outputs: vec![result.clone()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: collected.boxed(),
    };
    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "value_map_modulator_result".into(),
            args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::Binding(result)],
        },
        fields: vec![CURRENT.into()],
        input: applied.boxed(),
    })
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
pub(super) fn lower_properties(input: Node, keys: &[String], lo: &mut Lowerer) -> Node {
    if lo.subgraph_vertex_property_filter.is_some() && keys.len() == 1 && keys[0] == "location" {
        let project = Node::GraphCurrentProject {
            expr: IrExpr::Call {
                name: "gremlin_visible_vertex_properties".to_string(),
                args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::lit_str("location")],
            },
            fields: vec![CURRENT.to_string()],
            input: input.boxed(),
        };
        return Node::GraphUnwind {
            input_expr: IrExpr::Binding(CURRENT.into()),
            bind: CURRENT.into(),
            outer: false,
            input: project.boxed(),
        };
    }
    let property = lo.fresh("property");
    let unwound = Node::GraphUnwind {
        input_expr: IrExpr::Call {
            name: "properties_list".into(),
            args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::List(keys.iter().map(IrExpr::lit_str).collect()), IrExpr::lit_bool(true), IrExpr::lit_bool(true), IrExpr::lit_bool(false)],
        },
        bind: property.clone(),
        outer: false,
        input: input.boxed(),
    };
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![
            ProjectionItem { alias: CURRENT.into(), expr: IrExpr::Binding(property.clone()) },
            ProjectionItem { alias: PATH.into(), expr: IrExpr::Call { name: "path_append_after".into(), args: vec![IrExpr::Binding(PATH.into()), IrExpr::Binding(CURRENT.into()), IrExpr::Binding(property)] } },
        ],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: unwound.boxed(),
    }
}

pub(super) fn lower_properties_value(input: Node, keys: &[String], lo: &mut Lowerer) -> Node {
    Node::GraphCurrentProject {
        expr: IrExpr::property(
            CURRENT,
            "value".to_string(),
            PropertyMissing::DropUnproductive,
        ),
        fields: vec![CURRENT.to_string()],
        input: lower_properties(input, keys, lo).boxed(),
    }
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
