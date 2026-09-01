//! Language frontends.
//!
//! Each frontend parses its source language and lowers supported constructs
//! into the shared Graph IR.

pub mod cypher;
pub mod gremlin;
pub mod sparql;
