//! Public planner facade.
//!
//! `CypherPlanner::plan` and `GremlinPlanner::plan` accept a parsed surface
//! AST and emit a `GraphPlan` for DataFusion execution or the
//! logical-plan adapter under `crate::ir::df`.
//!
//! These types are the integration seam between the language frontends and
//! the Graph IR. The frontends are responsible for parsing the source
//! string and producing the planner-input AST defined in
//! `crate::ir::bridge::{cypher, gremlin}`.
//!
//! Language-specific planners lower query syntax to Graph IR; relational
//! lowering then produces SQL or DataFusion physical operators.

pub mod cypher;
pub mod gremlin;

pub use cypher::CypherPlanner;
pub use gremlin::GremlinPlanner;
