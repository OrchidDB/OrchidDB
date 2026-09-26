#![cfg(feature = "duckdb")]

//! Relational read-after-write visibility regressions.
//!
//! Mutations run through the production DataFusion DAG. Reads through both
//! the DAG and DuckDB SQL must match explicit expected rows after writes.
//! Covers node/edge create, update, delete, identity gaps, and relationship
//! types spanning multiple endpoint tables.

#[path = "common/execution.rs"]
mod datafusion_test;
use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};

use crate::datafusion_test::execute_async as execute;
use orchiddb::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use orchiddb::ir::plan::GraphPlan;
use orchiddb::ir::rel::RelBackend;
use orchiddb::ir::runtime::ReturnedBatches;
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner;

/// `Person(name, age)` with `KNOWS` edges alice->bob, alice->carol, bob->carol.
fn fixture() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob", "carol"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![30, 28, 41]));
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "Person",
        vec![("name", names), ("age", ages)],
    ));
    graph
        .add_edges(edges_from_columns(
            "KNOWS",
            "Person",
            "Person",
            vec![0, 0, 1],
            vec![1, 2, 2],
            Vec::new(),
        ))
        .unwrap();
    graph
}

/// A relationship type split across two endpoint tables: `LIKES` from
/// `Person` to `Person` and `LIKES` from `Person` to `City`. Relational scans
/// must emit rows from both physical tables, not just the representative one.
fn grouped_fixture() -> PropertyGraph {
    let people: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob"]));
    let cities: ArrayRef = Arc::new(StringArray::from(vec!["paris"]));
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns("Person", vec![("name", people)]));
    graph.add_nodes(nodes_from_columns("City", vec![("name", cities)]));
    graph
        .add_edges(edges_from_columns(
            "LIKES",
            "Person",
            "Person",
            vec![0],
            vec![1],
            Vec::new(),
        ))
        .unwrap();
    graph
        .add_edges(edges_from_columns(
            "LIKES",
            "Person",
            "City",
            vec![1],
            vec![0],
            Vec::new(),
        ))
        .unwrap();
    graph
}

fn plan(query: &str) -> GraphPlan {
    let parsed = parse_query(query).expect("parse");
    CypherPlanner::new().plan(&parsed).expect("plan")
}

/// Apply a mutating statement through the production DataFusion DAG.
async fn apply(graph: &PropertyGraph, statement: &str) {
    execute(&plan(statement), graph).await.expect("mutate");
}

fn render(returned: ReturnedBatches) -> Vec<String> {
    let batch = returned.batch;
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(batch.column(col), row)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

async fn dag_rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    render(execute(&plan(query), graph).await.expect("DataFusion DAG"))
}

async fn sql_rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    use orchiddb::ir::rel::sql::{self, DuckDbExecutor};
    let lowered = RelBackend::new()
        .lower(&plan(query), graph)
        .expect("rel lower");
    render(
        sql::execute_lowered_sql(&mut DuckDbExecutor::new(), &lowered)
            .await
            .expect("DuckDB execute"),
    )
}

async fn assert_rows(graph: &PropertyGraph, query: &str, expected: &[&str]) {
    assert_eq!(sql_rows(graph, query).await, expected, "DuckDB: {query}");
    assert_eq!(
        dag_rows(graph, query).await,
        expected,
        "DataFusion DAG: {query}"
    );
}

#[tokio::test]
async fn create_node_is_visible_to_sql() {
    let graph = fixture();
    apply(&graph, "CREATE (n:Person {name: 'dave', age: 40})").await;

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_rows(
        &graph,
        query,
        &["alice|30", "bob|28", "carol|41", "dave|40"],
    )
    .await;
}

#[tokio::test]
async fn set_property_update_is_visible_to_sql() {
    let graph = fixture();
    apply(&graph, "MATCH (p:Person {name: 'alice'}) SET p.age = 31").await;

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_rows(&graph, query, &["alice|31", "bob|28", "carol|41"]).await;
}

#[tokio::test]
async fn set_whole_map_replaces_property_bag_visible_to_sql() {
    let graph = fixture();
    apply(
        &graph,
        "MATCH (p:Person {name: 'alice'}) SET p = {name: 'alicia', age: 31}",
    )
    .await;

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_rows(&graph, query, &["alicia|31", "bob|28", "carol|41"]).await;
}

#[tokio::test]
async fn delete_node_leaves_id_gap_visible_to_sql() {
    let graph = fixture();
    // bob is the middle row (id 1). DETACH DELETE removes it and its edges,
    // leaving a gap: carol keeps id 2, the new node gets id 3, and an edge
    // written after the delete must resolve those original ids.
    apply(&graph, "MATCH (p:Person {name: 'bob'}) DETACH DELETE p").await;
    apply(&graph, "CREATE (d:Person {name: 'dave', age: 44})").await;
    apply(
        &graph,
        "MATCH (a:Person {name: 'alice'}), (d:Person {name: 'dave'}) CREATE (a)-[:KNOWS]->(d)",
    )
    .await;

    let nodes = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_rows(&graph, nodes, &["alice|30", "carol|41", "dave|44"]).await;

    let edges = "MATCH (a)-[e:KNOWS]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_rows(&graph, edges, &["alice|carol", "alice|dave"]).await;
}

#[tokio::test]
async fn create_edge_endpoints_and_property_visible_to_sql() {
    let graph = fixture();
    apply(&graph, "CREATE (d:Person {name: 'dave', age: 44})").await;
    apply(
        &graph,
        "MATCH (c:Person {name: 'carol'}), (d:Person {name: 'dave'}) CREATE (c)-[:KNOWS {since: 2021}]->(d)",
    ).await;

    let query = "MATCH (a)-[e:KNOWS]->(b) RETURN a.name, e.since, b.name ORDER BY a.name, b.name";
    assert_rows(
        &graph,
        query,
        &[
            "alice||bob",
            "alice||carol",
            "bob||carol",
            "carol|2021|dave",
        ],
    )
    .await;
}

#[tokio::test]
async fn delete_edge_is_visible_to_sql() {
    let graph = fixture();
    apply(
        &graph,
        "MATCH (:Person {name: 'alice'})-[e:KNOWS]->(:Person {name: 'bob'}) DELETE e",
    )
    .await;

    let query = "MATCH (a)-[e:KNOWS]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_rows(&graph, query, &["alice|carol", "bob|carol"]).await;
}

#[tokio::test]
async fn grouped_relationship_types_scan_every_endpoint_table() {
    let graph = grouped_fixture();

    let query = "MATCH (a)-[:LIKES]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_rows(&graph, query, &["alice|bob", "bob|paris"]).await;
}

#[tokio::test]
async fn grouped_relationship_mutation_is_visible_to_sql() {
    let graph = grouped_fixture();
    apply(
        &graph,
        "MATCH (a:Person {name: 'alice'}), (c:City {name: 'paris'}) CREATE (a)-[:LIKES]->(c)",
    )
    .await;

    let query = "MATCH (a)-[:LIKES]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_rows(&graph, query, &["alice|bob", "alice|paris", "bob|paris"]).await;
}

#[tokio::test]
async fn create_update_delete_then_sql_read_has_expected_visibility() {
    let graph = fixture();
    apply(&graph, "CREATE (n:Person {name: 'dave', age: 40})").await;
    apply(&graph, "MATCH (p:Person {name: 'alice'}) SET p.age = 31").await;
    apply(&graph, "MATCH (p:Person {name: 'bob'}) DETACH DELETE p").await;

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_rows(&graph, query, &["alice|31", "carol|41", "dave|40"]).await;
}

#[tokio::test]
async fn scalar_ordering_preserves_numeric_order_and_cypher_null_placement() {
    let mut graph = PropertyGraph::new();
    let names: ArrayRef = Arc::new(StringArray::from(vec!["ten", "two", "missing"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![Some(10), Some(2), None]));
    graph.add_nodes(nodes_from_columns("Person", vec![("name", names), ("age", ages)]));
    assert_rows(
        &graph,
        "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.age ASC",
        &["two|2", "ten|10", "missing|"],
    ).await;
    assert_rows(
        &graph,
        "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.age DESC",
        &["missing|", "ten|10", "two|2"],
    ).await;
}
