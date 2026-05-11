//! Source-node builders shared across the Cypher lowering modules.
//!
//! Cypher patterns produce anchored node scans (`MATCH (n:Label)`) and
//! label/type predicates. This module owns the tiny vocabulary that other
//! lowering files use to construct those leaves so policy stays consistent.

use crate::ir::plan::{LabelExpr, Node};
use crate::language::cypher::semantics::DEFAULT_GRAPH;

pub fn node_scan(binding: impl Into<String>, labels: Vec<String>) -> Node {
    Node::GraphNodeScan {
        graph: DEFAULT_GRAPH.to_string(),
        binding: binding.into(),
        labels: label_expr(labels),
    }
}

/// Lower a list of relationship type names into the IR `LabelExpr`.
///
/// Cypher relationship types are joined by `|` and represent a disjunction
/// (`-[:REL_A|REL_B]->`). We translate to `LabelExpr::AnyOf` for >0 and
/// `Any` for 0; the planner does not produce `AllOf` for relationship
/// types because relationships only carry a single type.
pub fn rel_types_expr(types: Vec<String>) -> LabelExpr {
    if types.is_empty() {
        LabelExpr::Any
    } else {
        LabelExpr::AnyOf(types)
    }
}

/// Lower a list of node labels into the IR `LabelExpr`.
///
/// Cypher allows multi-label nodes (`MATCH (n:A:B)`); the conjunction of
/// labels lowers to `LabelExpr::AllOf`. A single label uses `AnyOf` so
/// downstream rewriters that expect the sugar form keep working.
pub fn label_expr(labels: Vec<String>) -> LabelExpr {
    match labels.len() {
        0 => LabelExpr::Any,
        1 => LabelExpr::AnyOf(labels),
        _ => LabelExpr::AllOf(labels),
    }
}
