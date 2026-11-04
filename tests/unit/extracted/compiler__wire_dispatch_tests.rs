
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn schema_commands_roundtrip_metadata_and_reject_query_fields() {
        let schema = json!({"tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"}]}],
                            "nodes":[{"label":"Person","table":"people","id":"id"}]});
        for op in ["validate_schema", "resolve_schema"] {
            let result = compile_json(&json!({"op":op,"schema":schema}).to_string()).await.unwrap();
            assert_eq!(serde_json::from_str::<serde_json::Value>(&result).unwrap(), schema);
            for bad in [json!(null), json!({"query":"RETURN 1"}), json!({"unknown":true})] {
                assert!(compile_json(&json!({"op":op,"schema":bad}).to_string()).await.is_err());
            }
        }
    }

    #[tokio::test]
    async fn adapter_commands_preserve_errors_and_empty_graph_batches() {
        let decoded = compile_json(&json!({"op":"decode_graph_values", "data_type":"int64", "values":[]}).to_string()).await.unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&decoded).unwrap(), json!([]));
        for (command, expected) in [
            (json!({"op":"decode_graph_values"}), "missing graph descriptor type"),
            (json!({"op":"decode_graph_values","data_type":"int64"}), "missing graph descriptor values"),
            (json!({"op":"bind"}), "missing bind plan"),
            (json!({"op":"bind","plan":{"version":2}}), "unsupported bind plan version"),
            (json!({"op":"bind_operation"}), "unsupported operation plan version"),
            (json!({"op":"bind_search","plan":{"version":1}}), "missing operation relation"),
            (json!({"op":"bind_operation","plan":{"version":1},"relation":"input"}), "missing transfers"),
        ] {
            assert_eq!(compile_json(&command.to_string()).await.unwrap_err(), expected, "{command}");
        }
        assert!(compile_json("{broken").await.is_err());
        assert!(compile_json("{}").await.unwrap_err().starts_with("invalid compiler request:"));
    }

    #[cfg(not(feature = "kafka"))]
    #[tokio::test]
    async fn disabled_streaming_returns_a_capability_error() {
        assert_eq!(compile_json(r#"{"op":"stream_status"}"#).await.unwrap_err(),
            "Kafka streaming support is disabled in this runtime");
    }

    #[cfg(not(feature = "orchid-catalog"))]
    #[tokio::test]
    async fn disabled_catalog_returns_a_capability_error() {
        assert_eq!(compile_json(r#"{"op":"catalog"}"#).await.unwrap_err(),
            "remote catalogs require the orchid-catalog feature");
    }
