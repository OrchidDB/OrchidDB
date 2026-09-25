#![cfg(feature = "duckdb")]

//! End-to-end tests for the public mapped-table graph query engine.
//!
//! A user-owned relational schema (`users`, `orders`, `follows`) is mapped
//! onto graph labels and edge types with a `GraphMapping` and stood up in an
//! in-memory DuckDB database through `MappedGraphEngine::execute_sql`. Real
//! Cypher, Gremlin, and SPARQL queries then run against that schema, and the
//! writes persist in the same mapped SQL tables.

use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};

use orchiddb::ir::plan::Direction;
use orchiddb::ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping};
use orchiddb::ir::rel::sql::DuckDbExecutor;
use orchiddb::language::sparql::OntologyMapping;
use orchiddb::mapped_engine::MappedGraphEngine;

/// The user's schema, as raw SQL applied via `execute_sql`.
const DDL: &str = r#"
CREATE TABLE users (id BIGINT, name VARCHAR, age BIGINT);
INSERT INTO users VALUES (1, 'alice', 30), (2, 'bob', 28), (3, 'carol', 41);
CREATE TABLE orders (order_id BIGINT, user_id BIGINT, total DOUBLE);
INSERT INTO orders VALUES (100, 1, 50.0), (101, 1, 120.0), (102, 2, 80.0), (103, 3, 500.0);
CREATE TABLE follows (src BIGINT, dst BIGINT);
INSERT INTO follows VALUES (1, 2), (1, 3), (2, 3);
"#;

fn schema(fields: Vec<Field>) -> Arc<Schema> {
    Arc::new(Schema::new(fields))
}

/// The graph -> relational mapping. Only schemas are registered: the data
/// itself lives in DuckDB, and `physical_table_names` keeps these tables from
/// being re-materialized by the SQL layer.
fn mapping() -> Arc<GraphMapping> {
    let mut mapping = GraphMapping::new();
    mapping
        .register_table_schema(
            "users",
            schema(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("name", DataType::Utf8, true),
                Field::new("age", DataType::Int64, true),
            ]),
        )
        .register_table_schema(
            "orders",
            schema(vec![
                Field::new("order_id", DataType::Int64, false),
                Field::new("user_id", DataType::Int64, false),
                Field::new("total", DataType::Float64, true),
            ]),
        )
        .register_table_schema(
            "follows",
            schema(vec![
                Field::new("src", DataType::Int64, false),
                Field::new("dst", DataType::Int64, false),
            ]),
        );
    mapping
        .map_node(
            NodeMapping::table("Person", "users", "id")
                .property("name", "name")
                .property("age", "age"),
        )
        .map_node(NodeMapping::table("Order", "orders", "order_id").property("total", "total"))
        .map_edge(
            EdgeMapping::table(
                "ORDERED", "orders", "user_id", "order_id", "Person", "Order",
            )
            .with_id("order_id")
            .property("total", "total"),
        )
        .map_edge(EdgeMapping::table(
            "FOLLOWS", "follows", "src", "dst", "Person", "Person",
        ));
    Arc::new(mapping)
}

fn engine() -> MappedGraphEngine {
    MappedGraphEngine::new(DuckDbExecutor::new(), mapping())
}

async fn setup_engine() -> MappedGraphEngine {
    let mut engine = engine();
    engine.execute_sql(DDL).expect("apply schema DDL");
    engine
}

fn lines(batch: &RecordBatch) -> Vec<String> {
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| cell_to_string(batch.column(col), row))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

fn cell_to_string(array: &ArrayRef, row: usize) -> String {
    if array.is_null(row) {
        return "null".to_string();
    }
    match array.data_type() {
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Float64 => {
            let value = array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(row);
            format!("{value}")
        }
        other => panic!("unhandled result type in mapped_engine test: {other}"),
    }
}

#[tokio::test]
async fn cypher_match_expand_filter_over_user_tables() {
    let mut engine = setup_engine().await;
    let result = engine
        .cypher(
            "MATCH (p:Person)-[:ORDERED]->(o) WHERE o.total > 100.0 \
             RETURN p.name, o.total ORDER BY o.total",
        )
        .await
        .expect("cypher");
    assert_eq!(lines(&result.batch), vec!["alice|120", "carol|500"]);
}

#[tokio::test]
async fn cypher_edge_properties_and_aggregates_over_user_tables() {
    let mut engine = setup_engine().await;
    let result = engine
        .cypher(
            "MATCH (p:Person)-[r:ORDERED]->(:Order) \
             RETURN p.name AS name, sum(r.total) AS spent ORDER BY name",
        )
        .await
        .expect("cypher");
    assert_eq!(
        lines(&result.batch),
        vec!["alice|170", "bob|80", "carol|500"]
    );
}

#[tokio::test]
async fn cypher_follow_join_table_between_persons() {
    let mut engine = setup_engine().await;
    let result = engine
        .cypher(
            "MATCH (a:Person)-[:FOLLOWS]->(b:Person) WHERE a.name = 'alice' \
             RETURN b.name ORDER BY b.name",
        )
        .await
        .expect("cypher");
    assert_eq!(lines(&result.batch), vec!["bob", "carol"]);
}

#[tokio::test]
async fn gremlin_count_and_traversal_over_user_tables() {
    let mut engine = setup_engine().await;

    let counted = engine
        .gremlin("g.V().hasLabel('Person').count()")
        .await
        .expect("gremlin count");
    assert_eq!(lines(&counted.batch), vec!["3"]);

    let traversed = engine
        .gremlin("g.V().hasLabel('Person').has('name', 'alice').out('FOLLOWS').values('name')")
        .await
        .expect("gremlin traversal");
    let mut names = lines(&traversed.batch);
    names.sort();
    assert_eq!(names, vec!["bob", "carol"]);
}

fn ontology() -> OntologyMapping {
    OntologyMapping::new()
        .class("https://example.com/Person", "Person")
        .property("https://example.com/name", "Person", "name")
        .relationship_between(
            "https://example.com/follows",
            "FOLLOWS",
            Direction::Out,
            "Person",
            "Person",
        )
}

#[tokio::test]
async fn sparql_property_and_relationship_over_user_tables() {
    let mut engine = setup_engine().await;

    let names = engine
        .sparql(
            "PREFIX ex: <https://example.com/> \
             SELECT ?name WHERE { ?p a ex:Person . ?p ex:name ?name . } ORDER BY ?name",
            ontology(),
        )
        .await
        .expect("sparql property");
    assert_eq!(lines(&names.batch), vec!["alice", "bob", "carol"]);

    let friends = engine
        .sparql(
            "PREFIX ex: <https://example.com/> \
             SELECT DISTINCT ?friendName WHERE { \
               ?a a ex:Person . ?a ex:follows ?friend . \
               ?friend a ex:Person . ?friend ex:name ?friendName . \
             } ORDER BY ?friendName",
            ontology(),
        )
        .await
        .expect("sparql relationship");
    assert_eq!(lines(&friends.batch), vec!["bob", "carol"]);
}

#[tokio::test]
async fn explain_cypher_returns_generated_sql_referencing_user_tables() {
    let mut engine = setup_engine().await;
    let sql = engine
        .explain_cypher("MATCH (p:Person)-[:ORDERED]->(o) RETURN p.name, o.total ORDER BY o.total")
        .await
        .expect("explain");
    assert!(
        sql.contains("users"),
        "generated SQL should scan the user `users` table, got:\n{sql}"
    );
    assert!(
        sql.contains("orders"),
        "generated SQL should scan the user `orders` table, got:\n{sql}"
    );
    assert!(
        !sql.contains("node_person") && !sql.contains("edge_"),
        "generated SQL must not materialize synthetic catalog tables, got:\n{sql}"
    );
}

#[tokio::test]
async fn unsupported_merge_is_rejected_and_data_is_untouched() {
    let mut engine = setup_engine().await;
    for query in [
        "MERGE (p:Person {name: 'dave'})",
    ] {
        let err = engine.cypher(query).await.expect_err(query);
        assert!(
            err.contains("mutation"),
            "expected a mutation rejection for `{query}`, got: {err}"
        );
    }
    let result = engine
        .cypher("MATCH (p:Person) RETURN count(p) AS n")
        .await
        .expect("count");
    assert_eq!(lines(&result.batch), vec!["3"]);
}

#[tokio::test]
async fn schema_setup_is_visible_to_subsequent_queries() {
    let mut engine = engine();
    engine
        .execute_sql("CREATE TABLE users (id BIGINT, name VARCHAR, age BIGINT)")
        .unwrap();
    engine
        .execute_sql("INSERT INTO users VALUES (1, 'alice', 30), (2, 'bob', 28)")
        .unwrap();

    let result = engine
        .cypher("MATCH (p:Person) RETURN count(p) AS n")
        .await
        .expect("count");
    assert_eq!(lines(&result.batch), vec!["2"]);

    engine
        .execute_sql("INSERT INTO users VALUES (3, 'carol', 41)")
        .unwrap();
    let result = engine
        .cypher("MATCH (p:Person) RETURN count(p) AS n")
        .await
        .expect("count");
    assert_eq!(lines(&result.batch), vec!["3"]);
}
