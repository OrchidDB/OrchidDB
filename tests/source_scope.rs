//! Built-in connections stay small; engine-owned function rewrites stay open.
use orchiddb::{compiler::compile_json, ir::rel::sql::SqlDialect};
use serde_json::{Value, json};

const REMOVED: &[&str] = &[
    "weaviate",
    "elasticsearch",
    "quickwit",
    "prometheus",
    "loki",
    "starrocks",
];

#[tokio::test]
async fn removed_sources_are_not_registered() {
    for adapter in REMOVED {
        assert!(SqlDialect::resolve(adapter).is_err(), "{adapter}");
        assert!(
            orchiddb::operations::resolve(adapter).unwrap().is_none(),
            "{adapter}"
        );
        let request = json!({"version":1,"dialect":"duckdb","language":"cypher",
            "query":"RETURN 1 AS n","sources":{"db":{"adapter":adapter}}});
        assert!(
            compile_json(&request.to_string()).await.is_err(),
            "{adapter}"
        );
    }
}

#[tokio::test]
async fn retained_sources_lower_custom_engine_function_signatures() {
    for adapter in ["postgres", "duckdb", "trino"] {
        assert!(SqlDialect::resolve(adapter).is_ok());
        let request = json!({"version":1,"dialect":adapter,"language":"cypher",
            "native_values":true,
            "query":"MATCH(p:Person) RETURN custom.measure(p.name) AS n",
            "sources":{"db":{"adapter":adapter}},
            "tables":[{"name":"people","source":"db","columns":[
                {"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
            "nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name"}}],
            "functions":[{"name":"custom.measure","target":"extension_measure",
                "parameters":["string"],"returns":"int64"}]});
        let result = compile_json(&request.to_string())
            .await
            .unwrap_or_else(|e| panic!("{adapter}: {e}"));
        let result: Value = serde_json::from_str(&result).unwrap();
        let nodes = result["execution_plan"]["nodes"].as_array().unwrap();
        assert!(
            nodes
                .iter()
                .all(|n| n["kind"] == "source_sql" || n["kind"] == "local_project"),
            "{result}"
        );
        assert!(
            nodes
                .iter()
                .filter_map(|n| n["sql"].as_str())
                .any(|sql| sql.contains("extension_measure")),
            "{result}"
        );
    }
}

#[tokio::test]
async fn only_cypher_and_duckdb_completion_are_accepted() {
    for language in ["gremlin", "sparql"] {
        let request = json!({"version":1,"dialect":"duckdb","language":language,
            "query":"RETURN 1 AS n","tables":[]});
        let error = compile_json(&request.to_string()).await.unwrap_err();
        assert!(error.contains("Only Cypher"), "{error}");
    }
    for engine in ["none", "postgres", "trino", "datafusion"] {
        let request = json!({"version":1,"dialect":"duckdb","language":"cypher",
            "engine":engine,"query":"RETURN 1 AS n","tables":[]});
        assert!(
            compile_json(&request.to_string()).await.is_err(),
            "{engine}"
        );
    }
}

#[tokio::test]
async fn retired_rdf_configuration_is_rejected_instead_of_silently_ignored() {
    for field in [
        "rdf",
        "rdf_sources",
        "rdf_graph_names",
        "dataset",
        "ontology",
    ] {
        let mut request = json!({"version":1,"dialect":"duckdb","language":"cypher",
            "query":"RETURN 1 AS n","tables":[]});
        request[field] = Value::Null;
        let error = compile_json(&request.to_string()).await.unwrap_err();
        assert!(error.contains("unknown field"), "{field}: {error}");
    }
}
