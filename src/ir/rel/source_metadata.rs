//! Physical source facts consumed by engine relational lowering.
//!
//! The catalog records storage format and index capabilities, not an execution
//! backend. Engine ownership and the adapter registry determine execution.
use super::search::SearchMetric;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMetadata {
    pub table: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<IndexMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexMetadata {
    pub column: String,
    pub metric: SearchMetric,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
#[path = "../../../tests/unit/extracted/ir__rel__source_metadata__tests.rs"]
mod tests;
