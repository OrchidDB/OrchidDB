#![cfg(feature = "duckdb")]
use orchiddb::engine::{GraphEngine, QueryResult};

fn rows(result: &QueryResult) -> Vec<String> {
    let batch = &result.returned.batch;
    (0..batch.num_rows()).map(|row| batch.columns().iter()
        .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
        .collect::<Vec<_>>().join("|")).collect()
}

#[tokio::test]
async fn count_after_delete_preserves_empty_identity() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:P), (:P), (:P)").await.unwrap();
    for expected in ["3", "0"] {
        let result = engine.cypher("MATCH (n:P) DELETE n RETURN count(*) AS c").await.unwrap();
        assert_eq!(rows(&result), [expected]);
    }
    assert_eq!(rows(&engine.cypher("MATCH (n:P) RETURN count(*)").await.unwrap()), ["0"]);
}

#[tokio::test]
async fn suffix_limit_zero_does_not_suppress_delete() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:P), (:P), (:P)").await.unwrap();
    let result = engine.cypher("MATCH (n:P) DELETE n RETURN count(*) AS c LIMIT 0").await.unwrap();
    assert!(rows(&result).is_empty());
    assert_eq!(rows(&engine.cypher("MATCH (n:P) RETURN count(*)").await.unwrap()), ["0"]);
}

#[tokio::test]
async fn count_after_set_preserves_duplicate_occurrence_effects() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:P {num: 0})").await.unwrap();
    let result = engine.cypher("MATCH (n:P) UNWIND [1,1,1] AS x SET n.num = n.num + 1 RETURN count(*) AS c").await.unwrap();
    assert_eq!(rows(&result), ["3"]);
    assert_eq!(rows(&engine.cypher("MATCH (n:P) RETURN n.num").await.unwrap()), ["3"]);
}
