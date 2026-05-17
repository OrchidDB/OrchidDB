//! Cypher-side dataset builder.
//!
//! The Ladybug case corpus references a long tail of fixtures
//! (`tinysnb`, `demo-db/csv`, `ldbc-sf01`, …) that ship as plain CSV
//! files plus a Kuzu-flavoured `schema.cypher`. We resolve the
//! metadata `dataset` field against a directory under
//! `tests/data/ladybug/dataset` and let the schema-driven loader
//! ([`super::loader`]) build a `PropertyGraph` from the schema and
//! `copy.cypher`. Names that don't map to a real directory surface
//! as `Skipped`, mirroring how the Gremlin harness treats `grateful`
//! / `sink`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use new_graph::ir::catalog::PropertyGraph;

use super::initializer;
use super::loader;

const LADYBUG_ROOT: &str = "tests/data/ladybug/dataset";

#[derive(Debug)]
pub struct DatasetError(pub String);

impl std::fmt::Display for DatasetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Per-process cache mapping a resolved fixture directory to the
/// loaded `PropertyGraph`. Without this, every case file re-parses
/// `schema.cypher` and re-loads every CSV — multiplying the harness
/// runtime by orders of magnitude on big datasets like `ldbc-sf01`.
type DatasetCache = Mutex<HashMap<String, Result<PropertyGraph, String>>>;

fn cache() -> &'static DatasetCache {
    static CACHE: OnceLock<DatasetCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[allow(dead_code)]
pub fn build(name: &str) -> Result<PropertyGraph, DatasetError> {
    build_with_initializer(name, None)
}

/// Build a `PropertyGraph` for a case, preferring a per-scenario inline
/// graph initializer when one is provided.
///
/// The existing schema/copy-backed loader path remains the default for
/// Ladybug fixtures with real CSVs (e.g. `tinysnb`, `demo-db/csv`,
/// `ldbc-sf01`). When the case file ships an explicit
/// `--- graph_initializer` section, we ignore the named dataset and
/// build a fresh `PropertyGraph` from the structured DSL handled by
/// [`super::initializer`]. The `name` argument is still consulted on
/// the empty-initializer path so the runner falls back transparently
/// for fixtures that don't need scenario setup.
pub fn build_with_initializer(
    name: &str,
    inline_source: Option<&str>,
) -> Result<PropertyGraph, DatasetError> {
    if let Some(source) = inline_source {
        let trimmed = source.trim();
        if !trimmed.is_empty() {
            let scenario = initializer::parse(trimmed)?;
            return initializer::build(&scenario);
        }
    }
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(PropertyGraph::new());
    }
    if is_empty_dataset(trimmed) {
        return Ok(PropertyGraph::new());
    }
    let Some(dir) = resolve_directory(trimmed) else {
        return Err(DatasetError(format!(
            "no fixture directory matched `{name}`"
        )));
    };

    let mut guard = cache().lock().expect("dataset cache poisoned");
    if !guard.contains_key(&dir) {
        let loaded = loader::build(&dir).map_err(|err| err.0);
        guard.insert(dir.clone(), loaded);
    }
    match guard.get(&dir).expect("just inserted") {
        Ok(graph) => Ok(graph.clone()),
        Err(err) => Err(DatasetError(err.clone())),
    }
}

fn is_empty_dataset(name: &str) -> bool {
    if name.eq_ignore_ascii_case("empty") {
        return true;
    }
    for prefix in ["csv ", "parquet ", "npy ", "json ", "binary ", "tsv "] {
        if let Some(rest) = strip_prefix_ci(name, prefix) {
            return rest.eq_ignore_ascii_case("empty");
        }
    }
    false
}

/// Translate a metadata `dataset` field (e.g. `"CSV tinysnb"`,
/// `"PARQUET demo-db/parquet"`, `"tinysnb"`, `"empty"`) into a
/// directory name under `tests/data/ladybug/dataset`. We try the raw
/// name first, then progressively strip loader-prefixes
/// (`csv `, `parquet `, `npy `, `json `) and lower the surviving
/// component, since the directory tree is consistently lower-case.
fn resolve_directory(name: &str) -> Option<String> {
    let candidates = candidates_for(name);
    let root = PathBuf::from(LADYBUG_ROOT);
    for candidate in candidates {
        if root.join(&candidate).is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn candidates_for(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let original = name.trim().to_string();
    push_unique(&mut out, original.clone());
    push_unique(&mut out, original.to_ascii_lowercase());

    // Strip loader-hint prefixes ("CSV ", "PARQUET ", …) — case
    // insensitively — and try the surviving fragment.
    for prefix in ["csv ", "parquet ", "npy ", "json ", "binary ", "tsv "] {
        if let Some(rest) = strip_prefix_ci(&original, prefix) {
            push_unique(&mut out, rest.to_string());
            push_unique(&mut out, rest.to_ascii_lowercase());
        }
    }
    out
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() < prefix.len() {
        return None;
    }
    let head = &text[..prefix.len()];
    if head.eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

fn push_unique(out: &mut Vec<String>, value: String) {
    if !out.iter().any(|existing| existing == &value) {
        out.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_empty_builds_empty_graph() {
        let graph = build_with_initializer("CSV empty", None).unwrap();
        assert!(graph.node_label_order().is_empty());
    }
}
