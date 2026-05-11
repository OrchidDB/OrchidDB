//! Graph IR — logical operators, an in-memory property graph store, and
//! an interpreter that runs the IR over Apache Arrow record batches.
//!
//! Reference: `docs/graph_ir_language_examples_v0_2_draft.md`. Per the
//! user's directive, SPARQL and GQL-only operators are intentionally not
//! implemented; Cypher and Gremlin coverage is the focus.

pub mod bridge;
pub mod catalog;
pub mod df;
pub mod expr;
pub mod interpreter;
pub mod plan;
pub mod policy;
pub mod value;

#[cfg(test)]
mod tests;

pub use catalog::{
    CatalogError, CatalogResult, EdgeTable, NodeTable, PropertyGraph, edges_from_columns,
    nodes_from_columns,
};
pub use expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit, StringOp};
pub use interpreter::{
    InterpretError, IrResult, ReturnedBatches, Row, eval, execute, execute_rows,
};
pub use plan::{
    ApplyKind, BindKind, Direction, DistinctMode, GraphPlan, LabelExpr, Length, Node, NullsOrder,
    ProjectMode, ProjectionItem, Slice, SortDir, SortKey, TargetMode, explain,
};
pub use policy::{
    GraphPlanPolicy, GraphScope, Language, MatchMode, Multiplicity, OptionalMissing, OutputNaming,
    PathMode, PropertyMissing, ProviderFeature, ResultForm,
};
pub use value::Value;
