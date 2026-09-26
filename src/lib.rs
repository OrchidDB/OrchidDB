//! OrchidDB graph IR.
//!
//! Compile graph queries to SQL with [`compiler`]; callers own execution through
//! their driver or the optional [`execution`] interface. See `docs/architecture.md`.
//! Graph IR is lowered to SQL or DataFusion plans; shared runtime kernels
//! supply scalar semantics without a recursive Graph IR interpreter.


pub mod spargebra;
pub mod compiler;
pub mod execution;
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
