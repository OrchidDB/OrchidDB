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

use super::context::CURRENT;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::Node;
use crate::ir::policy::PropertyMissing;

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
pub(super) fn lower_properties(input: Node, keys: &[String]) -> Node {
    let project = project_map(input, "properties_list", keys);
    Node::GraphUnwind {
        input_expr: IrExpr::Binding(CURRENT.into()),
        bind: CURRENT.into(),
        outer: false,
        input: project.boxed(),
    }
}

pub(super) fn lower_properties_value(input: Node, keys: &[String]) -> Node {
    Node::GraphCurrentProject {
        expr: IrExpr::property(
            CURRENT,
            "value".to_string(),
            PropertyMissing::DropUnproductive,
        ),
        fields: vec![CURRENT.to_string()],
        input: lower_properties(input, keys).boxed(),
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
