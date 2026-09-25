#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};
use orchiddb::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use orchiddb::ir::rel::{RelBackend, sql};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

fn fixture() -> PropertyGraph {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![(
            "name",
            Arc::new(StringArray::from(vec!["a", "b"])) as ArrayRef,
        )],
    ));
    graph.add_nodes(nodes_from_columns(
        "Q",
        vec![("name", Arc::new(StringArray::from(vec!["q"])) as ArrayRef)],
    ));
    graph
        .add_edges(edges_from_columns(
            "R",
            "P",
            "P",
            vec![0, 0, 0],
            vec![0, 1, 1],
            vec![(
                "weight",
                Arc::new(Int64Array::from(vec![10, 20, 30])) as ArrayRef,
            )],
        ))
        .unwrap();
    // The endpoint IDs coincide but the labels differ: this is not a loop.
    graph
        .add_edges(edges_from_columns(
            "R",
            "P",
            "Q",
            vec![0],
            vec![0],
            vec![("weight", Arc::new(Int64Array::from(vec![40])) as ArrayRef)],
        ))
        .unwrap();
    graph
}

async fn rows(query: &str) -> Vec<String> {
    let graph = fixture();
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new().lower(&plan, &graph).unwrap();
    let prepared = sql::prepare(&lowered, sql::SqlDialect::DuckDb)
        .await
        .unwrap();
    if std::env::var("SHOW_JOIN_SQL").is_ok() {
        eprintln!("{query}\n{}", prepared.query);
    }
    let result = sql::execute_prepared(&mut sql::DuckDbExecutor::new(), &prepared).unwrap();
    let mut rows = (0..result.batch.num_rows())
        .map(|row| {
            result
                .batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[tokio::test]
async fn undirected_edges_preserve_loops_parallel_edges_and_endpoint_labels() {
    assert_eq!(
        rows("MATCH (a)-[r:R]-(b) RETURN a.name, b.name, r.weight").await,
        [
            "a|a|10", "a|b|20", "a|b|30", "a|q|40", "b|a|20", "b|a|30", "q|a|40"
        ],
    );
    assert_eq!(
        rows("MATCH (a:P), (b:P) MATCH (a)-[r:R]-(b) RETURN a.name, b.name, r.weight").await,
        ["a|a|10", "a|b|20", "a|b|30", "b|a|20", "b|a|30"],
    );
}

#[tokio::test]
async fn optional_joins_preserve_duplicate_and_null_correlations() {
    assert_eq!(
        rows("UNWIND [1, 1, null] AS x MATCH (a:P {name: 'a'}) OPTIONAL MATCH (a)-[:R]->(b:P) OPTIONAL MATCH (b)-[:R]->(c:P) RETURN count(*)").await,
        ["15"],
    );
    assert_eq!(
        rows("UNWIND [1, null, 2] AS x RETURN count(*)").await,
        ["3"]
    );
    assert_eq!(rows("MATCH (p:P), (q:Q) RETURN count(*)").await, ["2"]);
    assert_eq!(
        rows("MATCH (p:P) WHERE p.name = 'missing' RETURN count(*)").await,
        ["0"]
    );
}

#[tokio::test]
async fn single_hop_existence_preserves_outer_multiplicity() {
    assert_eq!(rows("UNWIND [1, 1, null] AS x MATCH (a:P), (b:Q) WHERE EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["3"]);
    assert_eq!(rows("UNWIND [1, 1, null] AS x MATCH (a:P), (b:Q) WHERE NOT EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["3"]);
    assert_eq!(
        rows("MATCH (a:P {name: 'b'}) OPTIONAL MATCH (a)-[:R]->(b:Q) RETURN a.name, b.name").await,
        ["b|"]
    );
}

#[tokio::test]
async fn existence_can_filter_before_an_unrelated_expansion() {
    assert_eq!(rows("MATCH (a:P)-[:R]-(b:P)-[:R]->(q:Q) WHERE NOT EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["0"]);
    assert_eq!(
        rows(
            "MATCH (a:P)-[:R]-(b:P)-[:R]->(q:Q) WHERE EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)"
        )
        .await,
        ["3"]
    );
    assert_eq!(rows("MATCH (a:P), (b:P), (c:P)-[:R]->(q:Q) WHERE NOT EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["1"]);
    assert_eq!(rows("MATCH (a:P), (b:P), (c:P)-[:R]->(q:Q) WHERE EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["3"]);
}
