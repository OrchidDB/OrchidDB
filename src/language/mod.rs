//! Language frontends.
//!
//! Gremlin is the primary compiled frontend. Cypher is exposed for parser
//! and Graph IR planning work while its broader language coverage remains
//! incremental.

pub mod cypher;
pub mod gremlin;
