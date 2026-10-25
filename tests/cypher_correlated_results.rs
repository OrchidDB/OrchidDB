#![cfg(feature = "duckdb")]
use orchiddb::engine::{GraphEngine, QueryResult};
use serde_json::{Value, json};
fn values(result: &QueryResult) -> Vec<Value> {
    fn value(v: &Value) -> Value {
        match v["type"].as_str().unwrap() {
            "list" => json!(
                v["value"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(value)
                    .collect::<Vec<_>>()
            ),
            "null" => Value::Null,
            _ => v["value"].clone(),
        }
    }
    let native: Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"],
    )
    .unwrap();
    let mut rows = native
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            json!(
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(value)
                    .collect::<Vec<_>>()
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(Value::to_string);
    rows
}
#[tokio::test]
async fn pattern_comprehension_count_preserves_apply_output_and_duplicates() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (x:X), (x)-[:T]->(), (x)-[:T]->(), (x)-[:T]->(), (x)-[:OTHER]->()")
        .await
        .unwrap();
    for (query, expected) in [
        (
            "MATCH (a:X) RETURN size([(a)-->() | 1]) AS length",
            vec![json!([4])],
        ),
        (
            "MATCH (a:X) RETURN size([(a)-[:T]->() | 1]) AS length",
            vec![json!([3])],
        ),
        (
            "MATCH (a:X) RETURN size([(a)-[:T|OTHER]->() | 1]) AS length",
            vec![json!([4])],
        ),
        (
            "UNWIND [1,1] AS i MATCH (a:X) RETURN i,size([(a)-[:T]->() | 1]) AS length",
            vec![json!([1, 3]), json!([1, 3])],
        ),
    ] {
        let result = engine.cypher(query).await.unwrap();
        assert_eq!(
            values(&result),
            expected,
            "{query}\n{}",
            result.stats.physical_plan
        );
    }
}
#[tokio::test]
async fn correlated_node_and_edge_collection_keep_null_elements_and_empty_lists() {
    for (setup, query) in [
        (
            "CREATE (a), (b {name:'val'}), (c), (a)-[:T]->(b), (b)-[:T]->(c)",
            "MATCH(n) RETURN [(n)-[:T]->(b)|b.name] AS list",
        ),
        (
            "CREATE (a), (b), (c), (a)-[:T {name:'val'}]->(b), (b)-[:T]->(c)",
            "MATCH(n) RETURN [(n)-[r:T]->()|r.name] AS list",
        ),
    ] {
        let mut engine = GraphEngine::in_memory().unwrap();
        engine.cypher(setup).await.unwrap();
        let result = engine.cypher(query).await.unwrap();
        let mut expected = vec![json!([["val"]]), json!([[null]]), json!([[]])];
        expected.sort_by_key(Value::to_string);
        assert_eq!(values(&result), expected, "{query}");
    }
}
#[tokio::test]
async fn correlated_comprehension_boolean_keeps_the_empty_input_row() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (a:X {num:42}),(:X {num:43}),(a)-[:T]->()")
        .await
        .unwrap();
    let result = engine
        .cypher("MATCH(n:X) RETURN n.num,size([(n)--()|1])>0 AS connected")
        .await
        .unwrap();
    assert_eq!(values(&result), vec![json!([42, true]), json!([43, false])]);
}
#[tokio::test]
async fn correlated_degree_does_not_hide_invalid_aggregate_argument() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("UNWIND range(0,10) AS i CREATE(s:S) WITH s,i UNWIND range(0,i) AS j CREATE(s)-[:REL]->()").await.unwrap();
    let error=engine.cypher("MATCH(n:S) WITH n,size([(n)-->()|1]) AS deg WHERE deg>2 WITH deg LIMIT 100 RETURN percentileDisc(0.90,deg),deg").await.err().expect("invalid percentile must be evaluated for surviving degree rows");
    assert!(error.to_ascii_lowercase().contains("percentile"), "{error}");
}

/// Correlation over node-valued lists must not become a host loop. Duplicate
/// outer rows and reversed/repeated list elements make accidental regrouping or
/// ordering by node identity observable.
#[tokio::test]
async fn nested_pattern_lists_remain_one_sql_region() {
    use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping, EdgeMapping};
    use std::sync::Arc;
    let storage = duckdb::Connection::open_in_memory().unwrap();
    storage.execute_batch("CREATE TABLE nodes(id BIGINT PRIMARY KEY, n BIGINT, labels VARCHAR[]); INSERT INTO nodes VALUES (1,1,['X']), (2,NULL,['Y']), (3,NULL,['Y']), (4,NULL,['Y']), (5,2,['X']), (6,NULL,[]), (7,NULL,['L']), (8,NULL,['Y']); CREATE TABLE links(id BIGINT PRIMARY KEY,src BIGINT,dst BIGINT); INSERT INTO links VALUES (1,1,2),(2,2,3),(3,2,4),(4,5,6),(5,6,7),(6,6,8);").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("N", "nodes", "id").labels_column("labels").property("n", "n"));
    mapping.map_edge(EdgeMapping::table("T", "links", "src", "dst", "N", "N").with_id("id"));
    let mut engine = GraphEngine::mapped(storage, Arc::new(mapping)).unwrap();
    for (query, expected) in [
        ("MATCH p=(n:X)-->() RETURN n.n, [x IN nodes(p) | size([(x)-->(:Y) | 1])] AS list",
         vec![json!([1,[1,2]]),json!([2,[0,1]])]),
        ("MATCH p=(n:X)-->() RETURN n.n, [x IN nodes(p) | size([(x)-[:T*1..2]->(:Y) | 1])] AS list",
         vec![json!([1,[3,2]]),json!([2,[1,1]])]),
        ("MATCH p=(n:X)-->() RETURN n.n, [x IN nodes(p)+[null] | size([(x)-->(:Y) | 1])] AS list",
         vec![json!([1,[1,2,0]]),json!([2,[0,1,0]])]),
        ("MATCH p=(n:X)-->() RETURN n.n, [x IN reverse(nodes(p)) | size([(x)-->(:Y) | 1])] AS list",
         vec![json!([1,[2,1]]),json!([2,[1,0]])]),
        ("MATCH p=(n:X)-->() RETURN n.n, [x IN nodes(p)+nodes(p) | size([(x)-->(:Y) | 1])] AS list",
         vec![json!([1,[1,2,1,2]]),json!([2,[0,1,0,1]])]),
        ("MATCH p=(n:X)-->() RETURN n.n, [x IN nodes(p) WHERE size([(x)-->(:Y) | 1]) > 9 | x] AS list",
         vec![json!([1,[]]),json!([2,[]])]),
    ] {
        let result = engine.cypher(query).await.unwrap();
        assert_eq!(values(&result), expected, "{query}");
        for sql in &result.stats.sql_queries {
            for forbidden in ["__orchiddb_value(", "__orchiddb_predicate(", "__orchiddb_value_aggregate("] {
                assert!(!sql.contains(forbidden), "Generic expression interpreter in SQL: {sql}");
            }
        }
        assert_eq!(result.stats.cost.sql_executions, 1);
        assert_eq!(result.stats.cost.native_kernel_calls, 0);
        assert_eq!(result.stats.sql_queries.len(), 1, "{query}\n{}",result.stats.physical_plan);
        assert!(!result.stats.physical_plan.contains("LateralApply"), "{query}\n{}",result.stats.physical_plan);
        assert!(!result.stats.physical_plan.contains("EncodeSqlBindings"), "{}",result.stats.physical_plan);
        assert!(result.stats.native_source_queries.is_empty(), "{query}: {:?}",result.stats.native_source_queries);
    }
    for count in [1, 64] {
        let query=format!("UNWIND range(1,{count}) AS i MATCH p=(n:X)-->() RETURN i, n.n, [x IN nodes(p) | size([(x)-->(:Y) | 1])] AS list");
        let result=engine.cypher(&query).await.unwrap();
        let mut expected = (1..=count).flat_map(|i| [json!([i,1,[1,2]]), json!([i,2,[0,1]])]).collect::<Vec<_>>();
        expected.sort_by_key(Value::to_string);
        assert_eq!(values(&result), expected);
        for sql in &result.stats.sql_queries {
            for forbidden in ["__orchiddb_value(", "__orchiddb_predicate(", "__orchiddb_value_aggregate("] {
                assert!(!sql.contains(forbidden), "Generic expression interpreter in SQL: {sql}");
            }
        }
        assert_eq!(result.stats.cost.sql_executions, 1);
        assert_eq!(result.stats.cost.native_kernel_calls, 0);
        assert_eq!(result.stats.sql_queries.len(), 1, "{}",result.stats.physical_plan);
        assert!(!result.stats.physical_plan.contains("LateralApply"));
        assert!(!result.stats.physical_plan.contains("EncodeSqlBindings"));
        assert!(result.stats.native_source_queries.is_empty());
    }
}
