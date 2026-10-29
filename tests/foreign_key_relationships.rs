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
        bad.map_edge(bad.edge("LINK").unwrap().clone().with_id("parent_id"));
        assert!(bad.validate_foreign_keys().is_err());
        let mut bad = mapping(false);
        bad.map_edge(bad.edge("LINK").unwrap().clone().with_id("link_id"));
        bad.map_edge(EdgeMapping::table("SECOND", "children", "second_parent_id", "id", "Parent", "Child")
            .foreign_key(ForeignKeyEndpoint::Destination).with_id("link_id"));
        assert!(bad.validate_foreign_keys().is_err());
    }
}

#[tokio::test]
async fn foreign_key_relationships_preserve_independent_edge_identity() {
    for reverse in [false,true] {
        let mut m=mapping(reverse);
        m.map_edge(m.edge("LINK").unwrap().clone().with_id("link_id").property("id","link_id"));
        m.validate_foreign_keys().unwrap();
        assert_eq!(GraphMapping::from_toml(&m.to_toml()).unwrap().edge("LINK"),m.edge("LINK"));
        let mut e=MappedGraphEngine::new(DuckDbExecutor::new(),Arc::new(m));
        e.execute_sql("CREATE SEQUENCE link_ids START 100; CREATE TABLE parents(id VARCHAR PRIMARY KEY); CREATE TABLE children(id VARCHAR PRIMARY KEY,parent_id VARCHAR REFERENCES parents(id),name VARCHAR,weight BIGINT,link_id BIGINT UNIQUE DEFAULT nextval('link_ids')); INSERT INTO parents VALUES ('p'); INSERT INTO children VALUES ('c','p','child',7,91)").unwrap();
        let read=e.cypher("MATCH ()-[r:LINK]->() RETURN r.id").await.unwrap();
        assert_eq!(arrow::util::display::array_value_to_string(read.batch.column(0),0).unwrap(),"91");
        query(&mut e,"MATCH ()-[r:LINK]->() SET r.weight=9").await;
        assert_eq!(scalar(&mut e,"SELECT weight::VARCHAR FROM children WHERE id='c'"),"9");
        query(&mut e,"MATCH ()-[r:LINK]->() DELETE r").await;
        assert_eq!(scalar(&mut e,"SELECT count(*)::VARCHAR FROM children WHERE id='c' AND parent_id IS NULL AND link_id IS NULL"),"1");
        let pattern=if reverse {"(c)-[:LINK {weight:11}]->(p)"}else{"(p)-[:LINK {weight:11}]->(c)"};
        query(&mut e,&format!("MATCH (p:Parent),(c:Child) CREATE {pattern}")).await;
        assert_eq!(scalar(&mut e,"SELECT link_id::VARCHAR || ':' || weight::VARCHAR FROM children"),"100:11");
        assert!(e.cypher(&format!("MATCH (p:Parent),(c:Child) CREATE {pattern}")).await.is_err());
        assert_eq!(scalar(&mut e,"SELECT link_id::VARCHAR || ':' || weight::VARCHAR FROM children"),"100:11");
        query(&mut e,"MATCH (c:Child) DETACH DELETE c").await;
        assert_eq!(scalar(&mut e,"SELECT count(*)::VARCHAR FROM children"),"0");
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
async fn enforced_cyclic_writes_roll_back_rows() {
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
    assert!(err.to_string().to_lowercase().contains("constraint"), "{err}");
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

#[tokio::test]
async fn partitioned_reference_writes_route_keys_columns_defaults_and_deletes() {
    for reverse in [false, true] {
        let mut mapping = GraphMapping::new();
        mapping.map_node(NodeMapping::table("City", "cities", "city_id").property("id", "city_id"));
        for (label, table, key, reference, identity, weight) in [
            ("Person", "people", "person_id", "home_city", "residence_id", "person_weight"),
            ("Company", "companies", "company_id", "registered_city", "location_id", "company_weight"),
        ] {
            mapping.map_node(NodeMapping::table(label, table, key).property("id", key));
            let edge = if reverse {
                EdgeMapping::table("LIVES_IN", table, reference, key, "City", label)
                    .foreign_key(ForeignKeyEndpoint::Destination)
            } else {
                EdgeMapping::table("LIVES_IN", table, key, reference, label, "City")
                    .foreign_key(ForeignKeyEndpoint::Source)
            };
            mapping.map_edge_partition(edge.with_id(identity).property("weight", weight));
        }
        let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping));
        e.execute_sql("CREATE SEQUENCE person_edges START 100; CREATE SEQUENCE company_edges START 200;
            CREATE TABLE cities(city_id VARCHAR PRIMARY KEY);
            CREATE TABLE people(person_id VARCHAR PRIMARY KEY, home_city VARCHAR DEFAULT 'city', residence_id BIGINT UNIQUE DEFAULT nextval('person_edges'), person_weight BIGINT DEFAULT 7);
            CREATE TABLE companies(company_id VARCHAR PRIMARY KEY, registered_city VARCHAR DEFAULT 'city', location_id BIGINT UNIQUE DEFAULT nextval('company_edges'), company_weight BIGINT DEFAULT 9 CHECK(company_weight<20));
            INSERT INTO cities VALUES ('city');").unwrap();
        let pattern = if reverse {
            "(c)-[:LIVES_IN]->(:Person {id:'same'}), (c)-[:LIVES_IN]->(:Company {id:'same'})"
        } else {
            "(:Person {id:'same'})-[:LIVES_IN]->(c), (:Company {id:'same'})-[:LIVES_IN]->(c)"
        };
        query(&mut e, &format!("MATCH (c:City) CREATE {pattern}")).await;
        assert_eq!(scalar(&mut e, "SELECT home_city || ':' || residence_id || ':' || person_weight FROM people"), "city:100:7");
        assert_eq!(scalar(&mut e, "SELECT registered_city || ':' || location_id || ':' || company_weight FROM companies"), "city:200:9");
        assert_eq!(e.cypher("MATCH ()-[r:LIVES_IN]->() RETURN r.weight").await.unwrap().batch.num_rows(), 2);
        query(&mut e, "MATCH ()-[r:LIVES_IN]->() SET r.weight=r.weight+1").await;
        assert_eq!(scalar(&mut e, "SELECT person_weight::VARCHAR FROM people"), "8");
        assert_eq!(scalar(&mut e, "SELECT company_weight::VARCHAR FROM companies"), "10");

        assert!(e.cypher("MATCH ()-[r:LIVES_IN]->() SET r.weight=30").await.is_err());
        assert_eq!(scalar(&mut e, "SELECT person_weight::VARCHAR FROM people"), "8");
        assert_eq!(scalar(&mut e, "SELECT company_weight::VARCHAR FROM companies"), "10");

        // Deleting one partition's relationship must preserve both node rows
        // and the other partition, even with identical node primary keys.
        let pattern = if reverse { "(:City)-[r:LIVES_IN]->(:Person)" } else { "(:Person)-[r:LIVES_IN]->(:City)" };
        query(&mut e, &format!("MATCH {pattern} DELETE r")).await;
        assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM people WHERE home_city IS NULL AND residence_id IS NULL"), "1");
        assert_eq!(scalar(&mut e, "SELECT registered_city FROM companies"), "city");
        let pattern = if reverse { "(c)-[:LIVES_IN {weight:15}]->(p)" } else { "(p)-[:LIVES_IN {weight:15}]->(c)" };
        query(&mut e, &format!("MATCH (p:Person), (c:City) CREATE {pattern}")).await;
        assert_eq!(scalar(&mut e, "SELECT person_weight::VARCHAR FROM people"), "15");
        query(&mut e, "CREATE (:Person {id:'unlinked'}), (:Company {id:'unlinked'})").await;
        assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM people WHERE person_id='unlinked' AND home_city IS NULL"), "1");
        assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM companies WHERE company_id='unlinked' AND registered_city IS NULL"), "1");
        query(&mut e, "MATCH (n) DETACH DELETE n").await;
        assert_eq!(scalar(&mut e, "SELECT ((SELECT count(*) FROM people)+(SELECT count(*) FROM companies)+(SELECT count(*) FROM cities))::VARCHAR"), "0");
    }
}

#[tokio::test]
async fn partitioned_junction_writes_use_their_own_columns() {
    let mut m = GraphMapping::new();
    for label in ["A", "B", "C"] {
        m.map_node(NodeMapping::table(label, label.to_lowercase(), "id").property("id", "id"));
    }
    m.map_edge_partition(EdgeMapping::table("R", "ab", "from_a", "to_b", "A", "B")
        .with_id("ab_id").property("id", "ab_id").property("weight", "ab_weight"));
    m.map_edge_partition(EdgeMapping::table("R", "ac", "source_a", "target_c", "A", "C")
        .with_id("ac_id").property("id", "ac_id").property("weight", "ac_weight"));
    let mut e = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(m));
    e.execute_sql("CREATE TABLE a(id BIGINT PRIMARY KEY); CREATE TABLE b(id BIGINT PRIMARY KEY); CREATE TABLE c(id BIGINT PRIMARY KEY);
        CREATE TABLE ab(ab_id BIGINT PRIMARY KEY, from_a BIGINT, to_b BIGINT, ab_weight BIGINT DEFAULT 4);
        CREATE TABLE ac(ac_id BIGINT PRIMARY KEY, source_a BIGINT, target_c BIGINT, ac_weight BIGINT DEFAULT 8);").unwrap();
    query(&mut e, "CREATE (a:A {id:1}), (b:B {id:1}), (c:C {id:1}), (a)-[:R {id:11}]->(b), (a)-[:R {id:22}]->(c)").await;
    assert_eq!(scalar(&mut e, "SELECT ab_id || ':' || from_a || ':' || to_b || ':' || ab_weight FROM ab"), "11:1:1:4");
    assert_eq!(scalar(&mut e, "SELECT ac_id || ':' || source_a || ':' || target_c || ':' || ac_weight FROM ac"), "22:1:1:8");
    query(&mut e, "MATCH ()-[r:R]->() SET r.weight=r.weight+1").await;
    assert_eq!(scalar(&mut e, "SELECT ac_weight::VARCHAR FROM ac"), "9");
    query(&mut e, "MATCH (:A)-[r:R]->(:C) DELETE r").await;
    assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM ac"), "0");
    assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM ab"), "1");
    query(&mut e, "MATCH (n) DETACH DELETE n").await;
    assert_eq!(scalar(&mut e, "SELECT count(*)::VARCHAR FROM ab"), "0");
}
