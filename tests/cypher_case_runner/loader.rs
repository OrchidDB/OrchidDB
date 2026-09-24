//! Schema-driven Ladybug dataset loader.
//!
//! Each Ladybug fixture under `tests/data/ladybug/dataset/<dir>` ships
//! with two control files:
//!
//! - `schema.cypher` — a sequence of `create node table` /
//!   `create rel table` statements describing labels, columns, and
//!   primary keys.
//! - `copy.cypher`   — `COPY <table> FROM "<file>";` lines wiring CSV
//!   files (with optional `(header = true)` and similar options) to
//!   their destination tables.
//!
//! This module parses both, then loads the referenced CSVs into an
//! in-memory `PropertyGraph`. The interpreter only models scalar
//! Arrow types (`Int64`, `Float64`, `Bool`, `String`); anything richer
//! in the schema (struct, union, list, map, interval, uuid, bytea, …)
//! is collapsed to the raw CSV string so projection still surfaces
//! the literal text the cases expect.
//!
//! The loader is intentionally lenient: malformed rows / unresolvable
//! edge endpoints are dropped rather than failing the whole load,
//! since the case files can still exercise interesting query shapes
//! against partially-loaded data.

use std::fs;
use std::path::PathBuf;

use new_graph::ir::catalog::PropertyGraph;
use super::dataset::DatasetError;

mod assembly;
mod files;
mod schema;
mod tables;
mod temporal;
mod values;

use assembly::{apply_inline_schema_creates, build_property_graph};
use schema::{parse_copies, parse_schema};

const LADYBUG_ROOT: &str = "tests/data/ladybug/dataset";
const KUZU_MAP_ENTRIES_KEY: &str = "\u{0}kuzu_map_entries";

pub fn build(directory_name: &str) -> Result<PropertyGraph, DatasetError> {
    let root = PathBuf::from(LADYBUG_ROOT).join(directory_name);
    if !root.is_dir() {
        return Err(DatasetError(format!(
            "no fixture directory at `{}`",
            root.display()
        )));
    }
    let schema_text = fs::read_to_string(root.join("schema.cypher"))
        .map_err(|err| DatasetError(format!("read schema.cypher: {err}")))?;
    let copy_text = fs::read_to_string(root.join("copy.cypher")).unwrap_or_default();
    let schema = parse_schema(&schema_text);
    let copies = parse_copies(&copy_text);
    let mut graph = build_property_graph(&schema, &copies, &root)?;
    apply_inline_schema_creates(&schema_text, &mut graph)?;
    Ok(graph)
}
