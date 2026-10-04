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
mod tests {
    use super::*;
    use crate::ir::rel::mapping::GraphMapping;

    #[test]
    fn source_metadata_roundtrips_extensible_options() {
        let metadata = SourceMetadata {
            table: "documents".into(),
            format: Some("third_engine_format".into()),
            options: [("collection".into(), serde_json::json!("articles"))].into(),
            indexes: vec![IndexMetadata {
                column: "embedding".into(),
                metric: SearchMetric::Cosine,
                options: [("access_method".into(), serde_json::json!("vendor_ann"))].into(),
            }],
        };
        let mut mapping = GraphMapping::new();
        mapping.register_source_metadata(metadata.clone()).unwrap();
        let serialized = mapping.try_to_toml().unwrap();
        assert!(serialized.contains("source_metadata"));
        assert!(!serialized.contains("search_indexes"));
        let restored = GraphMapping::from_toml(&serialized).unwrap();
        assert_eq!(restored.source_metadata["documents"], metadata);
    }

    #[test]
    fn legacy_indexes_convert_to_source_metadata() {
        let mapping = GraphMapping::from_toml(
            r#"
[[search_indexes]]
table = "documents"
column = "embedding"
metric = "cosine"
backend = {kind = "lance", uri = "/tmp/documents.lance", nprobes = 8}
[[search_indexes]]
table = "documents"
column = "body"
metric = "bm25"
backend = {kind = "lance", uri = "/tmp/documents.lance"}
"#,
        )
        .unwrap();
        let metadata = &mapping.source_metadata["documents"];
        assert_eq!(metadata.format.as_deref(), Some("lance"));
        assert_eq!(metadata.indexes[0].options["nprobes"], serde_json::json!(8));
        assert_eq!(metadata.indexes.len(), 2);
        let restored = GraphMapping::from_toml(&mapping.to_toml()).unwrap();
        assert_eq!(restored.source_metadata, mapping.source_metadata);
    }

    #[test]
    fn legacy_pgvector_and_external_formats_preserve_index_intent() {
        for kind in ["pgvector", "third_engine_format"] {
            let input = format!(
                r#"
[[search_indexes]]
table = "documents"
column = "embedding"
metric = "cosine"
backend = {{kind = "{kind}"}}
"#
            );
            let mapping = GraphMapping::from_toml(&input).unwrap();
            let metadata = &mapping.source_metadata["documents"];
            assert_eq!(metadata.format.as_deref(), Some(kind));
            assert_eq!(metadata.indexes[0].metric, SearchMetric::Cosine);
            assert_eq!(metadata.indexes[0].column, "embedding");
        }
    }

    #[test]
    fn conflicting_legacy_sources_are_rejected() {
        let input = r#"
[[search_indexes]]
table = "documents"
column = "embedding"
metric = "cosine"
backend = {kind = "lance", uri = "/tmp/one.lance"}
[[search_indexes]]
table = "documents"
column = "body"
metric = "bm25"
backend = {kind = "lance", uri = "/tmp/two.lance"}
"#;
        assert!(
            GraphMapping::from_toml(input)
                .unwrap_err()
                .to_string()
                .contains("conflicting source metadata")
        );
    }

    #[test]
    fn json_null_options_remain_valid_metadata_but_fail_toml_serialization() {
        for value in [
            serde_json::Value::Null,
            serde_json::json!({"nested": [null]}),
        ] {
            let mut mapping = GraphMapping::new();
            let metadata = SourceMetadata {
                table: "documents".into(),
                format: Some("json_collection".into()),
                options: [("configuration".into(), value)].into(),
                indexes: vec![],
            };
            mapping.register_source_metadata(metadata.clone()).unwrap();
            assert_eq!(mapping.source_metadata["documents"], metadata);
            let error = mapping.try_to_toml().unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("source metadata cannot be represented in TOML")
            );
        }
    }

    #[test]
    fn duplicate_index_capabilities_are_rejected() {
        let index = IndexMetadata {
            column: "embedding".into(),
            metric: SearchMetric::Cosine,
            options: Default::default(),
        };
        let mut mapping = GraphMapping::new();
        assert!(
            mapping
                .register_source_metadata(SourceMetadata {
                    table: "documents".into(),
                    format: None,
                    options: Default::default(),
                    indexes: vec![index.clone(), index],
                })
                .is_err()
        );
    }
}
