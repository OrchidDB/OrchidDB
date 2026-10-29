//! Cypher lowering and Arrow execution library.
//!
//! Cypher lowers through shared Graph IR and relational/kernel
//! programs. Compile to SQL using [`compiler`], execute through caller-owned
//! sessions using [`execution`], or integrate an engine through [`ir::rel::sql`].
//! DuckDB is the sole relational completion engine; full source pushdown can
//! eliminate its work. Trino and PostgreSQL are SQL sources.
//! DataFusion supplies logical compilation only; remote adapters remain sources.
//! The DuckDB extension is built separately from `extension/` using this core.

#[doc(hidden)]
pub mod compiler;
pub mod session;
pub mod catalog;
pub mod execution;
pub mod federation;
pub mod operations;
mod observability;
pub mod grammar;
pub mod ir;
pub mod language;
pub mod jvm_bridge;
pub mod planner;

#[cfg(feature = "duckdb")]
pub mod engine;
#[cfg(feature = "duckdb")]
pub mod mapped_engine;
pub mod storage;
pub mod streaming;

/// Minimal placeholder for the syntax-tree wrapper that the Cypher AST
/// keeps alongside its lowered `Traversal`. The full parser tree lives
/// outside this crate's currently-built modules; this struct only exists
/// so the AST `Program` types compile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedGraphProgram {
    pub entry_rule: String,
}
