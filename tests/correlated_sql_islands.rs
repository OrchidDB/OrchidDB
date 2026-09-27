#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
#[tokio::test]
async fn pattern_predicates_keep_boolean_bindings_across_sql_native_boundaries() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (a:TheLabel {id:0}), (b:TheLabel {id:1}), (c:TheLabel {id:2}) CREATE (a)-[:T]->(b), (b)-[:T]->(c)").await.unwrap();
    for query in [
        "MATCH (a), (b) WHERE a.id=0 AND (a)-[:T]->(b:TheLabel) OR (a)-[:T*]->(b:MissingLabel) RETURN DISTINCT b",
        "MATCH (a), (b) WITH a,b WHERE a.id=0 AND (a)-[:T]->(b:TheLabel) OR (a)-[:T*]->(b:MissingLabel) RETURN DISTINCT b",
    ] {
        let result = engine.cypher(query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 1, "{query}");
    }
}

#[tokio::test]
async fn bound_endpoint_labels_filter_single_and_variable_length_patterns() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (a:A {id:0})-[:T]->(b:B {id:1})")
        .await
        .unwrap();
    for pattern in [
        "(a)-[:T]->(b:Missing)",
        "(a)-[:T]-(b:Missing)",
        "(a)-[:T*]->(b:Missing)",
        "(a)-[:T*0..2]->(b:Missing)",
    ] {
        let query = format!("MATCH (a), (b) WHERE {pattern} RETURN b");
        let result = engine.cypher(&query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 0, "{query}");
    }
    for pattern in ["(a)-[:T]->(b:B)", "(a)-[:T]-(b:B)", "(a)-[:T*]->(b:B)"] {
        let query = format!("MATCH (a), (b) WHERE {pattern} RETURN b.id");
        let result = engine.cypher(&query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 1, "{query}");
        assert_eq!(
            arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0)
                .unwrap(),
            "1"
        );
    }
}
