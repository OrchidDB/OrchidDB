//! Shared modules used by the `gremlin_tinkerpop_cases` integration test.
//!
//! Lives under `tests/` rather than `src/` because none of these helpers
//! are useful outside the test harness. Cargo skips subdirectories of
//! `tests/` for test-binary auto-discovery, so importing this as a `mod`
//! from a sibling integration test does not produce an extra binary.

pub mod case_file;
pub mod compare;
pub mod dataset;
pub mod format;
pub mod parse;
