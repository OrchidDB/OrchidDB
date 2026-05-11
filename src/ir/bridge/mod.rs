//! Bridges from language ASTs to Graph IR.
//!
//! These modules show how a planner can lower a parsed Cypher / Gremlin
//! program into the IR. They operate on small, planner-friendly AST mirrors
//! defined alongside each bridge so that the IR module stays decoupled from
//! the WIP parser/planner code under `src/language/`. When the existing
//! cypher/gremlin planners stabilize, they can call these bridge functions
//! directly using the same surface AST.

pub mod cypher;
pub mod gremlin;
