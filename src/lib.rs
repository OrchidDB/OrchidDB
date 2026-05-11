//! `new-graph` graph IR.
//!
//! Implements the IR described in `docs/graph_ir_language_examples_v0_2_draft.md`.
//! The Gremlin frontend (`crate::language::gremlin`) lowers parsed
//! traversals into Graph IR `Node` values; the interpreter under
//! `crate::ir::interpreter` runs them against an Apache Arrow backed
//! `PropertyGraph`.

pub mod grammar;
pub mod ir;
pub mod language;
pub mod planner;

/// Minimal placeholder for the syntax-tree wrapper that the Gremlin AST
/// keeps alongside its lowered `Traversal`. The full parser tree lives
/// outside this crate's currently-built modules; this struct only exists
/// so the AST `Program` types compile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedGraphProgram {
    pub entry_rule: String,
}
