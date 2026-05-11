//! Gremlin language frontend.
//!
//! - `ast` — the parsed traversal AST.
//! - `semantics` — supporting types (`Direction`, `GValue`, `Predicate`).
//! - `planner` — lowers a `Traversal` into a `crate::ir::plan::GraphPlan`.

pub mod ast;
pub mod parser;
pub mod planner;
pub mod semantics;

pub use ast::{GremlinProgram, Traversal};
pub use parser::{
    GremlinParseError, GremlinSyntax, GremlinToken, parse_query_list, parse_traversal,
    parse_traversal_with_bindings,
};
pub use planner::{GremlinPlanError, GremlinPlanResult, GremlinPlanner};
