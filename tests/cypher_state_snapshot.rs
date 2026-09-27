#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::GraphEngine,
    ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
const OBSERVATIONS: [(&str, &str); 5] = [
    ("nodes", "MATCH (n) RETURN id(n)"),
    ("relationships", "MATCH ()-[r]->() RETURN id(r)"),
    (
        "labels",
        "MATCH (n) UNWIND labels(n) AS l RETURN DISTINCT l",
    ),
    (
        "node_properties",
        "MATCH (n) UNWIND keys(n) AS k RETURN id(n),k,n[k]",
    ),
    (
        "edge_properties",
        "MATCH ()-[r]->() UNWIND keys(r) AS k RETURN id(r),k,r[k]",
    ),
];
fn normalize(value: &Value) -> Value {
    let kind = value["type"].as_str().unwrap();
    let v = &value["value"];
    match kind {
        "null" => Value::Null,
        "boolean" | "string" | "byte" | "short" | "int" | "long" | "uint8" | "uint16"
        | "uint32" => v.clone(),
        "float" | "double" => json!(v.as_str().unwrap().parse::<f64>().unwrap()),
        "cypher_temporal" => v["text"].clone(),
        "list" | "set" => json!(
            v.as_array()
                .unwrap()
                .iter()
                .map(normalize)
                .collect::<Vec<_>>()
        ),
        "map" => json!(
            v.as_array()
                .unwrap()
                .iter()
                .map(|pair| (
                    normalize(&pair[0]).as_str().unwrap().to_string(),
                    normalize(&pair[1])
                ))
                .collect::<BTreeMap<_, _>>()
        ),
        "scalar" => value.clone(),
        _ => panic!("unhandled test value {value}"),
    }
}
fn rows(values: &Value) -> BTreeSet<String> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            serde_json::to_string(
                &row.as_array()
                    .unwrap()
                    .iter()
                    .map(normalize)
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        })
        .collect()
}
async fn assert_matches_observation_queries(engine: &mut GraphEngine) {
    let snapshot = serde_json::to_value(engine.cypher_state_snapshot().unwrap()).unwrap();
    for (key, query) in OBSERVATIONS {
        let result = engine.cypher(query).await.unwrap();
        let schema = result.returned.batch.schema();
        let native: Value =
            serde_json::from_str(&schema.metadata()["orchiddb.cypher.typed_rows.v1"]).unwrap();
        assert_eq!(
            rows(&snapshot[key]),
            rows(&native),
            "{key}: snapshot {snapshot}"
        );
    }
}
#[tokio::test]
async fn snapshot_matches_current_labels_properties_deletes_and_temporals() {
    let mut engine = GraphEngine::in_memory().unwrap();
    assert_matches_observation_queries(&mut engine).await;
    engine.cypher("CREATE (a:A:B {id:7,name:'a',xs:[1,2],date:date('2020-02-03')}), (b {name:'b'}), (a)-[:R {n:2}]->(b)").await.unwrap();
    assert_matches_observation_queries(&mut engine).await;
    engine
        .cypher("MATCH (a:A) SET a:C,a.name='changed',a.xs=null REMOVE a:B")
        .await
        .unwrap();
    assert_matches_observation_queries(&mut engine).await;
    engine.cypher("MATCH (a:A) DETACH DELETE a").await.unwrap();
    assert_matches_observation_queries(&mut engine).await;
}
#[tokio::test]
async fn snapshot_observes_transaction_state_and_rollback() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:A {n:1})").await.unwrap();
    let before = serde_json::to_value(engine.cypher_state_snapshot().unwrap()).unwrap();
    engine.begin().unwrap();
    engine
        .cypher("MATCH(n) SET n.n=2 CREATE (:B)")
        .await
        .unwrap();
    assert_matches_observation_queries(&mut engine).await;
    assert_ne!(
        serde_json::to_value(engine.cypher_state_snapshot().unwrap()).unwrap(),
        before
    );
    engine.rollback().unwrap();
    assert_eq!(
        serde_json::to_value(engine.cypher_state_snapshot().unwrap()).unwrap(),
        before
    );
}
#[tokio::test]
async fn mapped_snapshot_reads_actual_tables_and_scalar_identity() {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE people(id VARCHAR PRIMARY KEY,name VARCHAR,age BIGINT); INSERT INTO people VALUES ('a','Alice',NULL),('b','Bob',9); CREATE TABLE links(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR,n BIGINT); INSERT INTO links VALUES ('r','a','b',2)").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(
        NodeMapping::table("Person", "people", "id")
            .property("id", "id")
            .property("name", "name")
            .property("age", "age"),
    );
    mapping.map_edge(
        EdgeMapping::table("R", "links", "src", "dst", "Person", "Person")
            .with_id("id")
            .property("n", "n"),
    );
    let mut engine = GraphEngine::mapped(connection, Arc::new(mapping)).unwrap();
    assert_matches_observation_queries(&mut engine).await;
    engine
        .cypher("MATCH(p:Person {id:'a'}) SET p.age=42")
        .await
        .unwrap();
    assert_matches_observation_queries(&mut engine).await;
}

#[tokio::test]
async fn mapped_snapshot_keeps_database_scalar_key_formatting() {
    for (kind, literal) in [
        ("BIGINT", "9007199254740993"),
        ("BOOLEAN", "true"),
        ("DATE", "DATE '2020-02-03'"),
        ("DECIMAL(38,4)", "1234567890123456789012345.1250"),
        ("BLOB", "from_hex('00FF41')"),
    ] {
        let connection = duckdb::Connection::open_in_memory().unwrap();
        connection.execute_batch(&format!("CREATE TABLE items(id {kind} PRIMARY KEY,p VARCHAR); INSERT INTO items VALUES ({literal},'value')")).unwrap();
        let mut mapping=GraphMapping::new();
        mapping.map_node(NodeMapping::table("Item","items","id").property("p","p"));
        let mut engine=GraphEngine::mapped(connection,Arc::new(mapping)).unwrap();
        assert_matches_observation_queries(&mut engine).await;
    }
}
