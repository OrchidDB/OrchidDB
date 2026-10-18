#![cfg(feature = "duckdb")]
use duckdb::Connection;
use orchiddb::{
    engine::GraphEngine,
    ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping},
};
use std::sync::Arc;

fn fixture() -> (GraphEngine, Connection) {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE SEQUENCE person_ids; CREATE SEQUENCE link_ids; CREATE TABLE people(id BIGINT PRIMARY KEY DEFAULT nextval('person_ids'), name VARCHAR NOT NULL, score BIGINT); CREATE TABLE links(id BIGINT PRIMARY KEY DEFAULT nextval('link_ids'), src BIGINT, dst BIGINT)").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(
        NodeMapping::table("Person", "people", "id")
            .property("identity", "id")
            .property("name", "name")
            .property("score", "score"),
    );
    mapping.map_edge(
        EdgeMapping::table("KNOWS", "links", "src", "dst", "Person", "Person").with_id("id"),
    );
    (
        GraphEngine::mapped(connection.try_clone().unwrap(), Arc::new(mapping)).unwrap(),
        connection,
    )
}
fn count(connection: &Connection, sql: &str) -> i64 {
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

#[tokio::test]
async fn omitted_node_and_edge_keys_use_database_sequences() {
    let (mut engine, connection) = fixture();
    let result = engine.cypher("CREATE (a:Person {name:'Alice',score:1}), (b:Person {name:'Bob',score:2}), (a)-[:KNOWS]->(b) RETURN a.identity,b.identity").await.unwrap();
    assert_eq!(
        count(&connection, "SELECT count(DISTINCT id) FROM people"),
        2
    );
    assert_eq!(
        count(
            &connection,
            "SELECT count(*) FROM links l JOIN people a ON a.id=l.src JOIN people b ON b.id=l.dst WHERE a.name='Alice' AND b.name='Bob'"
        ),
        1
    );
    assert!(
        result
            .stats
            .write_queries
            .iter()
            .any(|write| write.sql.contains("INSERT INTO \"people\"")
                && write.inputs.iter().any(|(_, rows)| *rows == 2))
    );
    engine
        .cypher("MERGE (p:Person {name:'Alice'}) ON MATCH SET p.score=3")
        .await
        .unwrap();
    assert_eq!(count(&connection, "SELECT count(*) FROM people"), 2);
    assert_eq!(
        count(&connection, "SELECT score FROM people WHERE name='Alice'"),
        3
    );
    engine
        .cypher("CREATE (:Person {identity:100,name:'Explicit'})")
        .await
        .unwrap();
    assert_eq!(
        count(&connection, "SELECT count(*) FROM people WHERE id=100"),
        1
    );
}

#[tokio::test]
async fn generated_keys_do_not_bypass_constraints_or_atomicity() {
    let (mut engine, connection) = fixture();
    assert!(
        engine
            .cypher("CREATE (:Person {name:'temporary'}), (:Person {name:null})")
            .await
            .is_err()
    );
    assert_eq!(count(&connection, "SELECT count(*) FROM people"), 0);
    engine
        .cypher("CREATE (:Person {name:'survivor'})")
        .await
        .unwrap();
    assert_eq!(count(&connection, "SELECT count(*) FROM people"), 1);
    connection
        .execute_batch("ALTER TABLE people ALTER COLUMN id DROP DEFAULT")
        .unwrap();
    assert!(
        engine
            .cypher("CREATE (:Person {name:'missing identity'})")
            .await
            .is_err()
    );
    assert_eq!(count(&connection, "SELECT count(*) FROM people"), 1);
}
