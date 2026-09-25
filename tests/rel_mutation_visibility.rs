#![cfg(feature = "duckdb")]

//! Relational read-after-write visibility regressions.
//!
//! Graph mutations (`CREATE`, `SET`, `DELETE`) live in the catalog overlay on
//! top of immutable Arrow fixtures. The relational backend must materialize
//! scans from the same overlay-aware view the interpreter uses, or a SQL read
//! run after a write silently returns the *pre-mutation* data.
//!
//! Each test applies mutations through the interpreter (the reference), then
//! runs the same read query through both the interpreter and `RelBackend` and
//! asserts the rendered rows agree. Coverage includes node/edge create, update
//! and delete (with the id gaps deletions leave behind), edge endpoint
//! resolution, and relationship types grouped across multiple endpoint tables.

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};

use orchiddb::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use orchiddb::ir::interpreter::{ReturnedBatches, execute};
use orchiddb::ir::plan::GraphPlan;
use orchiddb::ir::rel::RelBackend;
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

/// Apply a mutating statement through the interpreter (the reference writer).
fn apply(graph: &PropertyGraph, statement: &str) {
    execute(&plan(statement), graph).expect("mutate");
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

fn interpreter_rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    render(execute(&plan(query), graph).expect("interpret"))
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

#[tokio::test]
async fn create_node_is_visible_to_sql() {
    let graph = fixture();
    apply(&graph, "CREATE (n:Person {name: 'dave', age: 40})");

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}

#[tokio::test]
async fn set_property_update_is_visible_to_sql() {
    let graph = fixture();
    apply(&graph, "MATCH (p:Person {name: 'alice'}) SET p.age = 31");

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}

#[tokio::test]
async fn set_whole_map_replaces_property_bag_visible_to_sql() {
    let graph = fixture();
    apply(
        &graph,
        "MATCH (p:Person {name: 'alice'}) SET p = {name: 'alicia', age: 31}",
    );

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}

#[tokio::test]
async fn delete_node_leaves_id_gap_visible_to_sql() {
    let graph = fixture();
    // bob is the middle row (id 1). DETACH DELETE removes it and its edges,
    // leaving a gap: carol keeps id 2, the new node gets id 3, and an edge
    // written after the delete must resolve those original ids.
    apply(&graph, "MATCH (p:Person {name: 'bob'}) DETACH DELETE p");
    apply(&graph, "CREATE (d:Person {name: 'dave', age: 44})");
    apply(
        &graph,
        "MATCH (a:Person {name: 'alice'}), (d:Person {name: 'dave'}) CREATE (a)-[:KNOWS]->(d)",
    );

    let nodes = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_eq!(
        sql_rows(&graph, nodes).await,
        interpreter_rows(&graph, nodes)
    );

    let edges = "MATCH (a)-[e:KNOWS]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_eq!(
        sql_rows(&graph, edges).await,
        interpreter_rows(&graph, edges)
    );
}

#[tokio::test]
async fn create_edge_endpoints_and_property_visible_to_sql() {
    let graph = fixture();
    apply(&graph, "CREATE (d:Person {name: 'dave', age: 44})");
    apply(
        &graph,
        "MATCH (c:Person {name: 'carol'}), (d:Person {name: 'dave'}) CREATE (c)-[:KNOWS {since: 2021}]->(d)",
    );

    let query = "MATCH (a)-[e:KNOWS]->(b) RETURN a.name, e.since, b.name ORDER BY a.name, b.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}

#[tokio::test]
async fn delete_edge_is_visible_to_sql() {
    let graph = fixture();
    apply(
        &graph,
        "MATCH (:Person {name: 'alice'})-[e:KNOWS]->(:Person {name: 'bob'}) DELETE e",
    );

    let query = "MATCH (a)-[e:KNOWS]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}

#[tokio::test]
async fn grouped_relationship_types_scan_every_endpoint_table() {
    let graph = grouped_fixture();

    let query = "MATCH (a)-[:LIKES]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}

#[tokio::test]
async fn grouped_relationship_mutation_is_visible_to_sql() {
    let graph = grouped_fixture();
    apply(
        &graph,
        "MATCH (a:Person {name: 'alice'}), (c:City {name: 'paris'}) CREATE (a)-[:LIKES]->(c)",
    );

    let query = "MATCH (a)-[:LIKES]->(b) RETURN a.name, b.name ORDER BY a.name, b.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}

#[tokio::test]
async fn create_update_delete_then_sql_read_equals_interpreter() {
    let graph = fixture();
    apply(&graph, "CREATE (n:Person {name: 'dave', age: 40})");
    apply(&graph, "MATCH (p:Person {name: 'alice'}) SET p.age = 31");
    apply(&graph, "MATCH (p:Person {name: 'bob'}) DETACH DELETE p");

    let query = "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.name";
    assert_eq!(
        sql_rows(&graph, query).await,
        interpreter_rows(&graph, query)
    );
}
