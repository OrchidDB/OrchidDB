#![cfg(feature = "duckdb")]
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::{
    ir::rel::{
        mapping::{EdgeMapping, GraphMapping, NodeMapping},
        sql::DuckDbExecutor,
    },
    mapped_engine::MappedGraphEngine,
};
use std::sync::Arc;
fn engine() -> MappedGraphEngine {
    let mut m = GraphMapping::new();
    for (table, cols) in [
        (
            "people",
            vec![
                ("person_id", DataType::Int64),
                ("full_name", DataType::Utf8),
                ("score", DataType::Int64),
            ],
        ),
        (
            "teams",
            vec![("team_id", DataType::Int64), ("title", DataType::Utf8)],
        ),
        (
            "memberships",
            vec![
                ("membership_id", DataType::Int64),
                ("person_fk", DataType::Int64),
                ("team_fk", DataType::Int64),
                ("strength", DataType::Int64),
            ],
        ),
    ] {
        m.register_table_schema(
            table,
            Arc::new(Schema::new(
                cols.into_iter()
                    .map(|(n, t)| Field::new(n, t, true))
                    .collect::<Vec<_>>(),
            )),
        );
    }
    m.map_node(
        NodeMapping::table("Person", "people", "person_id")
            .property("identity", "person_id")
            .property("name", "full_name")
            .property("score", "score"),
    );
    m.map_node(NodeMapping::table("Team", "teams", "team_id").property("name", "title"));
    m.map_edge(
        EdgeMapping::table(
            "MEMBER",
            "memberships",
            "person_fk",
            "team_fk",
            "Person",
            "Team",
        )
        .with_id("membership_id")
        .property("weight", "strength"),
    );
    let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(m));
    e.execute_sql("CREATE TABLE people(person_id BIGINT PRIMARY KEY,full_name VARCHAR NOT NULL,score BIGINT DEFAULT 10); CREATE TABLE teams(team_id BIGINT PRIMARY KEY,title VARCHAR NOT NULL); CREATE TABLE memberships(membership_id BIGINT PRIMARY KEY,person_fk BIGINT REFERENCES people(person_id),team_fk BIGINT REFERENCES teams(team_id),strength BIGINT)").unwrap();
    e
}
fn scalar(e: &mut MappedGraphEngine, sql: &str) -> i64 {
    e.executor_mut()
        .connection()
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}
#[tokio::test]
async fn cypher_writes_resolve_all_three_tables_and_preserve_read_mapping() {
    let mut e = engine();
    e.cypher("CREATE (p:Person {identity:42,name:'Alice',score:7}), (t:Team {name:'Engineering'}), (p)-[:MEMBER {weight:3}]->(t) RETURN p.name,t.name").await.unwrap();
    assert_eq!(
        scalar(
            &mut e,
            "SELECT count(*) FROM people WHERE person_id=42 AND full_name='Alice'"
        ),
        1
    );
    assert_eq!(
        scalar(
            &mut e,
            "SELECT count(*) FROM memberships WHERE person_fk=42 AND team_fk=1 AND strength=3"
        ),
        1
    );
    let r = e
        .gremlin("g.V().hasLabel('Person').out('MEMBER').values('name')")
        .await
        .unwrap();
    assert_eq!(r.batch.num_rows(), 1);
    e.cypher("MATCH (p:Person)-[r:MEMBER]->(t:Team) SET p.score=p.score+2, t.name='Platform', r.weight=8 RETURN p.score").await.unwrap();
    assert_eq!(scalar(&mut e, "SELECT score FROM people"), 9);
    assert_eq!(scalar(&mut e, "SELECT strength FROM memberships"), 8);
    assert!(e.cypher("MATCH (p:Person) DELETE p").await.is_err());
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 1);
    e.cypher("MATCH ()-[r:MEMBER]->() DELETE r").await.unwrap();
    e.cypher("MATCH (p:Person) DELETE p").await.unwrap();
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 0);
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM memberships"), 0);
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM teams"), 1);
}
#[tokio::test]
async fn mapped_writes_rollback_all_tables_on_failure_and_join_transactions() {
    let mut e = engine();
    assert!(
        e.cypher("CREATE (:Person {name:'temporary'}), (:Team {name:null})")
            .await
            .is_err()
    );
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 0);
    e.executor_mut().begin().unwrap();
    e.cypher("CREATE (:Person {name:'temporary'})")
        .await
        .unwrap();
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 1);
    e.executor_mut().rollback().unwrap();
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 0);
}
#[tokio::test]
async fn gremlin_inserts_updates_and_deletes_use_mapped_rows() {
    let mut e = engine();
    e.gremlin("g.addV('Person').property('name','Bob').property('score',20)")
        .await
        .unwrap();
    assert_eq!(
        scalar(
            &mut e,
            "SELECT count(*) FROM people WHERE full_name='Bob' AND score=20"
        ),
        1
    );
    e.gremlin("g.V().hasLabel('Person').has('name','Bob').property('score',21)")
        .await
        .unwrap();
    assert_eq!(scalar(&mut e, "SELECT score FROM people"), 21);
    e.gremlin("g.V().hasLabel('Person').drop()").await.unwrap();
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 0);
}
#[tokio::test]
async fn no_match_writes_preserve_return_and_aggregate_bindings() {
    let mut e = engine();
    for q in [
        "MATCH (p:Person) SET p.score=99 RETURN p.score",
        "MATCH (p:Person) DELETE p RETURN p.name",
        "MATCH (p:Person) CREATE (t:Team {name:'unused'}) RETURN t.name",
    ] {
        let result = e.cypher(q).await.unwrap();
        assert_eq!(result.batch.num_rows(), 0, "{q}");
    }
    let result = e
        .cypher("MATCH (p:Person) SET p.score=99 RETURN count(p) AS n")
        .await
        .unwrap();
    assert_eq!(result.batch.num_rows(), 1);
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM teams"), 0);
}
#[tokio::test]
async fn replacements_read_old_values_and_unknown_properties_rollback() {
    let mut e = engine();
    e.cypher("CREATE (:Person {name:'A',score:2})")
        .await
        .unwrap();
    e.cypher("MATCH (p:Person) SET p = {name:p.name,score:p.score+3} RETURN p.score")
        .await
        .unwrap();
    assert_eq!(
        scalar(&mut e, "SELECT score FROM people WHERE full_name='A'"),
        5
    );
    assert!(
        e.cypher("MATCH (p:Person) SET p.score=9, p.unmapped='bad'")
            .await
            .is_err()
    );
    assert_eq!(scalar(&mut e, "SELECT score FROM people"), 5);
    assert!(
        e.cypher("MATCH (p:Person) SET p.identity=55")
            .await
            .is_err()
    );
    assert_eq!(scalar(&mut e, "SELECT person_id FROM people"), 1);
    assert_eq!(
        scalar(
            &mut e,
            "SELECT count(*) FROM information_schema.tables WHERE table_name LIKE '__crabgraph_%'"
        ),
        0
    );
}
#[tokio::test]
async fn gremlin_creates_edges_between_existing_mapped_nodes() {
    let mut e = engine();
    e.cypher("CREATE (:Person {name:'A'}), (:Team {name:'T'})")
        .await
        .unwrap();
    e.gremlin("g.V().hasLabel('Person').as('p').V().hasLabel('Team').addE('MEMBER').from('p').property('weight',4)").await.unwrap();
    assert_eq!(
        scalar(
            &mut e,
            "SELECT count(*) FROM memberships WHERE person_fk=1 AND team_fk=1 AND strength=4"
        ),
        1
    );
    e.gremlin("g.E().hasLabel('MEMBER').property('weight',6)")
        .await
        .unwrap();
    assert_eq!(scalar(&mut e, "SELECT strength FROM memberships"), 6);
    e.gremlin("g.E().hasLabel('MEMBER').drop()").await.unwrap();
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM memberships"), 0);
}
#[tokio::test]
async fn detach_deletes_incident_rows_without_requiring_an_edge_id() {
    let mut m = GraphMapping::new();
    m.register_table_schema(
        "nodes",
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
    );
    m.register_table_schema(
        "links",
        Arc::new(Schema::new(vec![
            Field::new("src", DataType::Int64, false),
            Field::new("dst", DataType::Int64, false),
        ])),
    );
    m.map_node(NodeMapping::table("N", "nodes", "id").property("identity", "id"));
    m.map_edge(EdgeMapping::table("LINK", "links", "src", "dst", "N", "N"));
    let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(m));
    e.execute_sql("CREATE TABLE nodes(id BIGINT); INSERT INTO nodes VALUES (1),(2); CREATE TABLE links(src BIGINT,dst BIGINT); INSERT INTO links VALUES (1,2),(2,1),(1,1)").unwrap();
    assert!(
        e.cypher("MATCH (n:N) WHERE n.identity=1 DELETE n")
            .await
            .is_err()
    );
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM links"), 3);
    e.cypher("MATCH (n:N) WHERE n.identity=1 DETACH DELETE n")
        .await
        .unwrap();
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM links"), 0);
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM nodes WHERE id=2"), 1);
}
#[tokio::test]
async fn mapped_insert_parameters_are_data_and_failed_transaction_is_rolled_back() {
    use orchiddb::ir::Value;
    use std::collections::BTreeMap;
    let mut e = engine();
    let name = "x'); DROP TABLE people; --\0suffix";
    let parameters = BTreeMap::from([("name".into(), Value::String(name.into()))]);
    e.cypher_with_params("CREATE (:Person {name:$name})", &parameters)
        .await
        .unwrap();
    let stored: String = e
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT full_name FROM people", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored, name);
    e.executor_mut().begin().unwrap();
    e.cypher("CREATE (:Person {name:'rolled back'})")
        .await
        .unwrap();
    assert!(e.cypher("MATCH (p:Person) SET p.unknown=1").await.is_err());
    assert!(!e.executor().in_transaction());
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 1);
}
#[tokio::test]
async fn optional_unbound_targets_are_no_op_writes() {
    let mut e = engine();
    e.cypher("CREATE (:Person {name:'A'})").await.unwrap();
    let result=e.cypher("MATCH (p:Person) OPTIONAL MATCH (p)-[r:MEMBER]->(t:Team) SET t.name='absent' RETURN p.name").await.unwrap();
    assert_eq!(result.batch.num_rows(), 1);
    e.cypher("MATCH (p:Person) OPTIONAL MATCH (p)-[r:MEMBER]->(t:Team) DELETE t")
        .await
        .unwrap();
    assert_eq!(scalar(&mut e, "SELECT count(*) FROM people"), 1);
}
