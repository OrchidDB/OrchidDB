//! Shared modules used by the `cypher_ladybug_cases` integration test.
//!
//! The `case_file`, `format`, and `compare` helpers are reused verbatim
//! from the gremlin case runner via `#[path = ...]` mounts in the test
//! crate root. The cypher-side modules here drive dataset construction
//! against the Ladybug fixture tree.

pub mod dataset;
pub mod loader;
