#![allow(dead_code)]
//! ANTLR parse-tree to Cypher AST lowering surface.
//!
//! These modules are split by Cypher grammar family so each syntactic surface
//! lowers through a focused, structured path.

pub mod arithmetic;
pub mod cases;
pub mod clauses;
pub mod collections;
pub mod context;
pub mod diagnostics;
pub mod dispatch;
pub mod expressions;
pub mod frames;
pub mod functions;
pub mod labels;
pub mod literals;
pub mod names;
pub mod operators;
pub mod parameters;
pub mod patterns;
pub mod predicates;
pub mod procedures;
pub mod projections;
pub mod properties;
pub mod queries;
pub mod ranges;
pub mod schema;
pub mod source;
pub mod statements;
pub mod subqueries;
pub mod text;
pub mod updating;
pub mod visitor;

use crate::grammar::generated::cypher::cypherparser::OC_CypherContext;
use crate::language::cypher::ast::Query;
use crate::language::cypher::parser::Result;

pub(crate) fn lower_cypher(_ctx: &OC_CypherContext<'_>) -> Result<Query> {
    statements::lower_cypher(_ctx)
}
