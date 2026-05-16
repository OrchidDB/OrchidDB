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
/// Cypher's labels in a pattern (`MATCH (n:A:B)`) conjoin in the
/// standard; Kuzu's conformance suite however interprets them as a
/// disjunction (`A` *or* `B`) because Kuzu stores each label as a
/// separate node table. The Ladybug corpus expects that disjunction,
/// so we emit `AnyOf` for >0 labels and let the runtime treat the
/// label set as a union scan. Intersection semantics — when the
/// catalogue actually carries multi-label nodes — would route through
/// `AllOf` here, but the corpus' graph model never carries them.
pub fn label_expr(labels: Vec<String>) -> LabelExpr {
    if labels.is_empty() {
        LabelExpr::Any
    } else {
        LabelExpr::AnyOf(labels)
    }
}
