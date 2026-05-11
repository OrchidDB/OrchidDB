//! Public planner facade.
//!
//! `CypherPlanner::plan` and `GremlinPlanner::plan` accept a parsed surface
//! AST and emit a `GraphPlan` ready for the interpreter (or for the
//! DataFusion adapter under `crate::ir::df`).
//!
//! These types are the integration seam between the language frontends and
//! the Graph IR. The frontends are responsible for parsing the source
//! string and producing the planner-input AST defined in
//! `crate::ir::bridge::{cypher, gremlin}`.
//!
//! The legacy SQL-island planner under `src/language/cypher/planner/` is a
//! separate, in-progress lowering target that goes from Cypher to DuckDB
//! SQL. The Graph IR planners here are the new lowering target.

pub mod cypher;
pub mod gremlin;

pub use cypher::CypherPlanner;
pub use gremlin::GremlinPlanner;
