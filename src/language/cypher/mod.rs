//! Cypher frontend.
//!
//! The parser is built on the generated ANTLR visitor and produces a compact
//! Cypher AST. The planner lowers that AST directly into the shared Graph IR.

pub mod ast;
pub mod parser;
pub mod planner;
pub mod semantics;

pub use ast::{CypherProgram, Query};
pub use parser::{CypherParseError, CypherSyntax, CypherToken, parse_cypher, parse_query};
pub use planner::{CypherPlanError, CypherPlanResult, CypherPlanner};
