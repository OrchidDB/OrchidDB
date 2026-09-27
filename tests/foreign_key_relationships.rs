#![cfg(feature = "duckdb")]

use orchiddb::{
    ir::rel::{
        mapping::{EdgeMapping, ForeignKeyEndpoint, GraphMapping, NodeMapping},
        sql::DuckDbExecutor,
    },
    mapped_engine::MappedGraphEngine,
};
use std::sync::Arc;

fn mapping(reverse: bool) -> GraphMapping {
    let mut m = GraphMapping::new();
    m.map_node(NodeMapping::table("Parent", "parents", "id").property("id", "id"));
    m.map_node(
        NodeMapping::table("Child", "children", "id")
            .property("id", "id")
            .property("name", "name"),
    );
    let edge = if reverse {
        EdgeMapping::table("LINK", "children", "id", "parent_id", "Child", "Parent")
            .foreign_key(ForeignKeyEndpoint::Source)
    } else {
        EdgeMapping::table("LINK", "children", "parent_id", "id", "Parent", "Child")
            .foreign_key(ForeignKeyEndpoint::Destination)
    };
    m.map_edge(edge.property("weight", "weight"));
    m
}
fn engine(reverse: bool, required: bool) -> MappedGraphEngine {
    let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping(reverse)));
    let required = if required { "NOT NULL" } else { "" };
    e.execute_sql(&format!("CREATE TABLE parents(id VARCHAR PRIMARY KEY); CREATE TABLE children(id VARCHAR PRIMARY KEY, parent_id VARCHAR {required} REFERENCES parents(id), name VARCHAR NOT NULL, weight BIGINT); INSERT INTO parents VALUES ('a'),('b')")).unwrap();
    e
}
fn scalar(e: &mut MappedGraphEngine, sql: &str) -> String {
    e.executor_mut()
        .connection()
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}
async fn query(e: &mut MappedGraphEngine, q: &str) {
    e.cypher(q).await.unwrap_or_else(|err| panic!("{q}: {err}"));
}

#[tokio::test]
async fn fk_writes_coalesce_and_preserve_child_in_both_directions() {
    for reverse in [false, true] {
        let mut e = engine(reverse, false);
        let pattern = if reverse {
            "(c:Child {id:'c',name:'child'})-[r:LINK {weight:7}]->(p)"
        } else {
            "(p)-[r:LINK {weight:7}]->(c:Child {id:'c',name:'child'})"
        };
        query(
            &mut e,
            &format!("MATCH (p:Parent {{id:'a'}}) CREATE {pattern}"),
        )
        .await;
        assert_eq!(
            scalar(
                &mut e,
                "SELECT parent_id || ':' || name || ':' || weight::VARCHAR FROM children"
            ),
            "a:child:7"
        );
        query(&mut e, "MATCH ()-[r:LINK]->() SET r.weight=9").await;
        assert_eq!(scalar(&mut e, "SELECT weight::VARCHAR FROM children"), "9");
        query(&mut e, "MATCH ()-[r:LINK]->() DELETE r").await;
        assert_eq!(
            scalar(
                &mut e,
                "SELECT count(*)::VARCHAR FROM children WHERE parent_id IS NULL AND name='child'"
            ),
            "1"
        );
        let r = e.cypher("MATCH ()-[r:LINK]->() RETURN r").await.unwrap();
        assert_eq!(r.batch.num_rows(), 0);
        let r = e.gremlin("g.E().hasLabel('LINK').count()").await.unwrap();
        assert_eq!(
            arrow::util::display::array_value_to_string(r.batch.column(0), 0).unwrap(),
            "0"
        );
        let pattern = if reverse {
            "(c)-[:LINK]->(p)"
        } else {
            "(p)-[:LINK]->(c)"
        };
        query(
            &mut e,
            &format!("MATCH (p:Parent {{id:'b'}}),(c:Child {{id:'c'}}) CREATE {pattern}"),
        )
        .await;
        assert_eq!(scalar(&mut e, "SELECT parent_id FROM children"), "b");
        query(&mut e, "MATCH (c:Child) DETACH DELETE c").await;
        assert_eq!(
            scalar(&mut e, "SELECT count(*)::VARCHAR FROM children"),
            "0"
        );
        assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM parents"), "2");
    }
}

#[tokio::test]
async fn required_fk_reparenting_and_failures_are_atomic() {
    let mut e = engine(false, true);
    query(
        &mut e,
        "CREATE (p:Parent {id:'new'})-[:LINK]->(:Child {id:'c',name:'child'})",
    )
    .await;
    for q in [
        "CREATE (:Child {id:'missing',name:'bad'})",
        "MATCH ()-[r:LINK]->() DELETE r",
        "MATCH (p:Parent {id:'b'}),(c:Child) CREATE (p)-[:LINK]->(c)",
        "MATCH (p:Parent {id:'new'}) DETACH DELETE p",
    ] {
        assert!(e.cypher(q).await.is_err(), "{q}");
        assert_eq!(
            scalar(&mut e, "SELECT parent_id FROM children WHERE id='c'"),
            "new"
        );
        assert_eq!(
            scalar(&mut e, "SELECT count(*)::VARCHAR FROM children"),
            "1"
        );
    }
    query(&mut e, "MATCH (old:Parent)-[r:LINK]->(c:Child),(p:Parent {id:'b'}) DELETE r CREATE (p)-[:LINK]->(c)").await;
    assert_eq!(scalar(&mut e, "SELECT parent_id FROM children"), "b");
    e.executor_mut().begin().unwrap();
    query(&mut e, "MATCH (old:Parent)-[r:LINK]->(c:Child),(p:Parent {id:'a'}) DELETE r CREATE (p)-[:LINK]->(c)").await;
    e.executor_mut().rollback().unwrap();
    assert_eq!(scalar(&mut e, "SELECT parent_id FROM children"), "b");
    let error = e
        .cypher("MATCH (p:Parent {id:'b'})-[r:LINK]->(c:Child) DETACH DELETE c,p")
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("foreign key constraint"),
        "{error}"
    );
    assert_eq!(scalar(&mut e, "SELECT parent_id FROM children"), "b");
    assert_eq!(
        scalar(&mut e, "SELECT count(*)::VARCHAR FROM parents WHERE id='b'"),
        "1"
    );
    // DuckDB's FK index requires the child removal to commit first.
    query(&mut e, "MATCH (c:Child) DETACH DELETE c").await;
    query(&mut e, "MATCH (p:Parent {id:'b'}) DELETE p").await;
    assert_eq!(
        scalar(&mut e, "SELECT count(*)::VARCHAR FROM children"),
        "0"
    );
    assert_eq!(
        scalar(&mut e, "SELECT count(*)::VARCHAR FROM parents WHERE id='b'"),
        "0"
    );
}

#[test]
fn mapping_roundtrip_and_ownership_validation() {
    for reverse in [false, true] {
        let m = mapping(reverse);
        m.validate_foreign_keys().unwrap();
        let text = m.to_toml();
        let restored = GraphMapping::from_toml(&text).unwrap();
        assert_eq!(restored.edge("LINK"), m.edge("LINK"));
        assert!(
            GraphMapping::from_toml(
                &text
                    .replace("foreign_key = \"dst\"", "foreign_key = \"invalid\"")
                    .replace("foreign_key = \"src\"", "foreign_key = \"invalid\"")
            )
            .is_err()
        );
        let mut bad = mapping(reverse);
        bad.map_node(NodeMapping::table("Child", "children", "id").property("parent", "parent_id"));
        assert!(bad.validate_foreign_keys().is_err());
        let mut bad = mapping(reverse);
        bad.map_edge(bad.edge("LINK").unwrap().clone().with_id("weight"));
        assert!(bad.validate_foreign_keys().is_err());
    }
}

#[tokio::test]
async fn multiple_required_links_and_batched_inserts() {
    let mut m = mapping(false);
    m.map_edge(
        EdgeMapping::table("SECOND", "children", "second_id", "id", "Parent", "Child")
            .foreign_key(ForeignKeyEndpoint::Destination),
    );
    let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(m));
    e.execute_sql("CREATE TABLE parents(id VARCHAR PRIMARY KEY); CREATE TABLE children(id VARCHAR PRIMARY KEY, parent_id VARCHAR NOT NULL REFERENCES parents(id), second_id VARCHAR NOT NULL REFERENCES parents(id), name VARCHAR NOT NULL, weight BIGINT)").unwrap();
    query(&mut e, "CREATE (p:Parent {id:'a'}),(q:Parent {id:'b'}) WITH p,q UNWIND ['c','d','e'] AS key CREATE (p)-[:LINK {weight:5}]->(c:Child {id:key,name:'child'}),(q)-[:SECOND]->(c)").await;
    assert_eq!(
        scalar(
            &mut e,
            "SELECT count(*)::VARCHAR FROM children WHERE parent_id='a' AND second_id='b' AND weight=5"
        ),
        "3"
    );
    assert!(
        e.cypher(
            "MATCH (p:Parent {id:'a'}) CREATE (p)-[:LINK]->(:Child {id:'incomplete',name:'bad'})"
        )
        .await
        .is_err()
    );
    assert_eq!(
        scalar(&mut e, "SELECT count(*)::VARCHAR FROM children"),
        "3"
    );
}

#[tokio::test]
async fn nullable_fk_defaults_do_not_create_implicit_edges() {
    let mut e = engine(false, false);
    e.execute_sql("ALTER TABLE children ALTER COLUMN parent_id SET DEFAULT 'a'")
        .unwrap();
    query(&mut e, "CREATE (:Child {id:'orphan',name:'child'})").await;
    assert_eq!(
        scalar(
            &mut e,
            "SELECT count(*)::VARCHAR FROM children WHERE parent_id IS NULL"
        ),
        "1"
    );
}

#[tokio::test]
async fn cyclic_writes_fail_before_persisting_rows() {
    let mut m = GraphMapping::new();
    m.map_node(NodeMapping::table("N", "nodes", "id").property("id", "id"));
    m.map_edge(
        EdgeMapping::table("LINK", "nodes", "id", "parent_id", "N", "N")
            .foreign_key(ForeignKeyEndpoint::Source),
    );
    let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(m));
    e.execute_sql(
        "CREATE TABLE nodes(id BIGINT PRIMARY KEY, parent_id BIGINT NOT NULL REFERENCES nodes(id))",
    )
    .unwrap();
    let err = e
        .cypher("CREATE (a:N {id:1})-[:LINK]->(b:N {id:2})-[:LINK]->(a)")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cyclic mapped writes"), "{err}");
    assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM nodes"), "0");
}

#[tokio::test]
async fn foreign_key_edges_keep_scalar_child_keys() {
    for (kind, parent, child) in [
        ("BOOLEAN", "true", "false"),
        ("UBIGINT", "18446744073709551615", "18446744073709551614"),
        (
            "DECIMAL(30,4)",
            "12345678901234567890.1234",
            "12345678901234567890.5678",
        ),
        ("DATE", "DATE '2020-01-01'", "DATE '2020-01-02'"),
        ("BLOB", "from_hex('ff00')", "from_hex('00ff')"),
    ] {
        let mut m = mapping(false);
        m.map_edge(m.edge("LINK").unwrap().clone().property("id", "id"));
        let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(m));
        e.execute_sql(&format!("CREATE TABLE parents(id {kind} PRIMARY KEY); CREATE TABLE children(id {kind} PRIMARY KEY,parent_id {kind} REFERENCES parents(id),name VARCHAR,weight BIGINT); INSERT INTO parents VALUES({parent}); INSERT INTO children VALUES({child},NULL,'child',NULL)")).unwrap();
        query(&mut e, "MATCH (p:Parent),(c:Child) CREATE (p)-[:LINK]->(c)").await;
        assert_eq!(
            scalar(
                &mut e,
                "SELECT count(*)::VARCHAR FROM children c JOIN parents p ON c.parent_id=p.id"
            ),
            "1",
            "{kind}"
        );
        let result = e
            .cypher("MATCH (:Parent)-[r:LINK]->(c:Child) RETURN r.id,c.id")
            .await
            .unwrap();
        assert_eq!(result.batch.num_rows(), 1, "{kind}");
        assert_eq!(result.batch.column(0), result.batch.column(1), "{kind}");
        query(&mut e, "MATCH ()-[r:LINK]->() DELETE r").await;
        assert_eq!(
            scalar(
                &mut e,
                "SELECT count(*)::VARCHAR FROM children WHERE parent_id IS NULL"
            ),
            "1",
            "{kind}"
        );
    }
}
