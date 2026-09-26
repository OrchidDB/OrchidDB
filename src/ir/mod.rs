//! Graph IR: logical operators, graph catalog and DataFusion-backed execution.
//!
//! Reference: `docs/architecture.md`.

pub mod analysis;
pub mod bridge;
pub mod catalog;
pub mod df;
pub mod diagnostics;
pub mod procedures;
pub mod exec;
pub mod expr;
pub mod functions;
pub mod gremlin_semantics;
pub mod runtime;
pub mod jvm;
pub mod plan;
pub mod policy;
pub mod rel;
pub mod value;

pub use catalog::{
    CatalogError, CatalogResult, EdgeTable, NodeTable, PropertyGraph, edges_from_columns,
    nodes_from_columns,
};
pub use expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit, StringOp};
pub use runtime::{
    RuntimeError, IrResult, ReturnedBatches, Row, eval,
};
pub use plan::{
    ApplyKind, BindKind, Direction, DistinctMode, GraphPlan, LabelExpr, Length, Node, NullsOrder,
    ProjectMode, ProjectionItem, RdfGraphScope, RdfTerm, Slice, SortDir, SortKey, TargetMode,
    explain,
};
pub use policy::{
    GraphPlanPolicy, GraphScope, Language, MatchMode, Multiplicity, OptionalMissing, OutputNaming,
    PathMode, PropertyMissing, ProviderFeature, ResultForm,
};
pub use value::Value;

pub mod temporal;
