//! Multi-engine graph language compiler and runtime.
//!
//! Cypher, Gremlin, and SPARQL lower through shared Graph IR and relational/kernel
//! programs. Compile to SQL using [`compiler`], execute through caller-owned
//! sessions using [`execution`], or integrate an engine through [`ir::rel::sql`].
//! The `duckdb` and `postgres` features enable optional database adapters.
//! The DuckDB extension is built separately from `extension/` using this core.

pub mod spargebra;
pub mod compiler;
pub mod execution;
pub mod federation;
pub mod operations;
#[cfg(any(feature = "quickwit", feature = "elasticsearch"))]
pub mod remote;
pub mod grammar;
pub mod ir;
pub mod language;
pub mod jvm_bridge;
pub mod planner;

#[cfg(feature = "duckdb")]
pub mod engine;
#[cfg(feature = "duckdb")]
pub mod mapped_engine;
#[cfg(feature = "duckdb")]
pub mod rdf_engine;
pub mod storage;

/// Minimal placeholder for the syntax-tree wrapper that the Gremlin AST
/// keeps alongside its lowered `Traversal`. The full parser tree lives
/// outside this crate's currently-built modules; this struct only exists
/// so the AST `Program` types compile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedGraphProgram {
    pub entry_rule: String,
}
